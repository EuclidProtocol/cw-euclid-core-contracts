use cosmwasm_std::{ensure, to_json_binary, Addr, DepsMut, Env, MessageInfo, Response};
use euclid::{
    cross_chain_user::CrossChainUser,
    error::ContractError,
    fee::BPS_100_PERCENT,
    msgs::{
        cross_chain_config::CrossChainConfig,
        pool_factory::PoolFactoryReply,
        vlp::base::{PoolKey, PoolType},
    },
    token::PairWithDenomAndAmount,
};

use crate::{
    outbound,
    state::{
        ConcentratedPoolCreateRequest, CONCENTRATED_VLPS, MAIN_FACTORY_ADDRESS,
        PENDING_CONCENTRATED_POOL_REQUESTS,
    },
};

/// Mirrors main factory's `validate_concentrated_fee_and_spacing`. The check is
/// duplicated here so pool_factory can reject malformed delegations even if
/// main factory's stub forgets to.
fn validate_fee_and_spacing(fee_tier_bps: u64, tick_spacing: u64) -> Result<(), ContractError> {
    let expected_tick_spacing = match fee_tier_bps {
        100 => 1,
        500 => 10,
        3_000 => 60,
        10_000 => 200,
        _ => return Err(ContractError::new("Invalid concentrated fee tier")),
    };
    ensure!(
        tick_spacing == expected_tick_spacing,
        ContractError::new(
            format!(
                "Invalid tick spacing {tick_spacing} for fee tier {fee_tier_bps}. \
                 Expected {expected_tick_spacing}"
            )
            .as_str()
        )
    );
    Ok(())
}

/// Handles a CLP `RequestConcentratedPoolCreation` delegated from main factory.
///
/// Caller MUST be the configured main factory. Pool factory:
///   1. validates the request (pair, pool_key shape, slippage, fee/spacing),
///   2. records the request in `PENDING_CONCENTRATED_POOL_REQUESTS`,
///   3. builds the outbound `RouterReceiveMsg::RequestConcentratedPoolCreation`
///      via `outbound::request_concentrated_pool_creation`,
///   4. returns the packet as `Response::data` typed as
///      `PoolFactoryReply::SendPacket` so main factory's reply handler can
///      drive the outbound dispatch.
///
/// Note: the singleton position-token NFT contract is instantiated by main
/// factory at its own instantiate time and remains authoritative there. The
/// `POSITION_TOKEN_CONTRACT` mirror on pool_factory is populated by Slice 8's
/// migration; no instantiate happens from this handler in Slice 4.
#[allow(clippy::too_many_arguments)]
pub fn on_request_concentrated_pool_creation(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    tx_id: String,
    sender: Addr,
    pair_with_denom_and_amount: PairWithDenomAndAmount,
    pool_key: PoolKey,
    slippage_tolerance_bps: u64,
    initial_tick: Option<i64>,
    cross_chain_config: CrossChainConfig,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;
    let main_factory = MAIN_FACTORY_ADDRESS.load(deps.storage)?;
    ensure!(info.sender == main_factory, ContractError::Unauthorized {});

    ensure!(
        slippage_tolerance_bps <= BPS_100_PERCENT,
        ContractError::InvalidSlippageTolerance {}
    );

    let pair = pair_with_denom_and_amount.get_pair()?;
    pair.validate()?;
    ensure!(
        pair.get_tupple() == pool_key.pair.get_tupple(),
        ContractError::new("Pair does not match pool key")
    );

    let (fee_tier_bps, tick_spacing) = match pool_key.pool_type {
        PoolType::Concentrated {
            fee_tier_bps,
            tick_spacing,
        } => (fee_tier_bps, tick_spacing),
        _ => return Err(ContractError::new("Pool key must be concentrated")),
    };
    validate_fee_and_spacing(fee_tier_bps, tick_spacing)?;

    ensure!(
        !PENDING_CONCENTRATED_POOL_REQUESTS.has(deps.storage, (sender.clone(), tx_id.clone())),
        ContractError::TxAlreadyExist {}
    );
    ensure!(
        !CONCENTRATED_VLPS.has(deps.storage, pool_key.to_map_key()),
        ContractError::PoolAlreadyExists {}
    );

    let main_state: euclid::msgs::factory::StateResponse = deps.querier.query_wasm_smart(
        main_factory.clone(),
        &euclid::msgs::factory::QueryMsg::GetState {},
    )?;
    let cross_chain_sender = CrossChainUser::new(main_state.chain_uid, sender.to_string());

    PENDING_CONCENTRATED_POOL_REQUESTS.save(
        deps.storage,
        (sender.clone(), tx_id.clone()),
        &ConcentratedPoolCreateRequest {
            tx_id: tx_id.clone(),
            sender: sender.clone(),
            pair_info: pair_with_denom_and_amount.clone(),
            pool_key: pool_key.clone(),
        },
    )?;

    let packet = outbound::request_concentrated_pool_creation(
        cross_chain_sender,
        tx_id.clone(),
        pair_with_denom_and_amount,
        pool_key,
        slippage_tolerance_bps,
        initial_tick,
    )?;

    let reply_data = to_json_binary(&PoolFactoryReply::SendPacket {
        msg: packet,
        timeout: cross_chain_config.timeout,
        ack_response: cross_chain_config.ack_response,
        sender,
    })?;

    Ok(Response::new()
        .add_attribute("method", "on_request_concentrated_pool_creation")
        .add_attribute("tx_id", tx_id)
        .set_data(reply_data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::{
        testing::{message_info, mock_dependencies, mock_env},
        to_json_binary, ContractResult, SystemResult, Uint256, WasmQuery,
    };
    use euclid::{
        chain::ChainUid,
        msgs::factory::StateResponse,
        token::{Pair, Token, TokenType, TokenWithDenomAndAmount},
    };

    use crate::testing::helpers::init_with_main_factory;

    fn sample_pair_info() -> PairWithDenomAndAmount {
        PairWithDenomAndAmount {
            token_1: TokenWithDenomAndAmount {
                token: Token::create("aaa".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uaaa".to_string(),
                    decimals: None,
                },
                amount: Uint256::from(100u128),
            },
            token_2: TokenWithDenomAndAmount {
                token: Token::create("bbb".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "ubbb".to_string(),
                    decimals: None,
                },
                amount: Uint256::from(100u128),
            },
        }
    }

    fn sample_pool_key() -> PoolKey {
        PoolKey {
            pair: Pair::new(
                Token::create("aaa".to_string()).unwrap(),
                Token::create("bbb".to_string()).unwrap(),
            )
            .unwrap(),
            pool_type: PoolType::Concentrated {
                fee_tier_bps: 500,
                tick_spacing: 10,
            },
        }
    }

    fn install_main_factory_state_querier(deps: &mut crate::testing::helpers::MockDeps) {
        let chain_uid = ChainUid::create("testchain".to_string()).unwrap();
        deps.querier.update_wasm(move |q| match q {
            WasmQuery::Smart { .. } => {
                let resp = StateResponse {
                    chain_uid: chain_uid.clone(),
                    router_contract: "router".to_string(),
                    relayer_contract: cosmwasm_std::Addr::unchecked("relayer"),
                    admin: euclid::admin::EuclidAdmin::default(cosmwasm_std::Addr::unchecked(
                        "admin",
                    )),
                    escrow_code_id: 10,
                    lp_code_id: 11,
                    is_native: false,
                };
                SystemResult::Ok(ContractResult::Ok(to_json_binary(&resp).unwrap()))
            }
            _ => panic!("unexpected query"),
        });
    }

    #[test]
    fn test_on_request_concentrated_pool_creation_rejects_non_main_factory_caller() {
        let mut deps = mock_dependencies();
        let main_factory = deps.api.addr_make("main_factory");
        init_with_main_factory(&mut deps, &main_factory);

        let stranger = deps.api.addr_make("stranger");
        let user = deps.api.addr_make("user");
        let info = message_info(&stranger, &[]);
        let res = on_request_concentrated_pool_creation(
            deps.as_mut(),
            mock_env(),
            info,
            "tx1".to_string(),
            user,
            sample_pair_info(),
            sample_pool_key(),
            50,
            None,
            CrossChainConfig::new(None, None, None),
        );
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});
    }

    #[test]
    fn test_on_request_concentrated_pool_creation_invalid_fee_tier() {
        let mut deps = mock_dependencies();
        let main_factory = deps.api.addr_make("main_factory");
        init_with_main_factory(&mut deps, &main_factory);

        let user = deps.api.addr_make("user");
        let info = message_info(&main_factory, &[]);
        let bad_key = PoolKey {
            pair: sample_pool_key().pair,
            pool_type: PoolType::Concentrated {
                fee_tier_bps: 999,
                tick_spacing: 10,
            },
        };
        let res = on_request_concentrated_pool_creation(
            deps.as_mut(),
            mock_env(),
            info,
            "tx_bad".to_string(),
            user,
            sample_pair_info(),
            bad_key,
            50,
            None,
            CrossChainConfig::new(None, None, None),
        );
        assert!(res.is_err());
    }

    #[test]
    fn test_on_request_concentrated_pool_creation_pair_mismatch() {
        let mut deps = mock_dependencies();
        let main_factory = deps.api.addr_make("main_factory");
        init_with_main_factory(&mut deps, &main_factory);

        let user = deps.api.addr_make("user");
        let info = message_info(&main_factory, &[]);
        let mismatched_pool_key = PoolKey {
            pair: Pair::new(
                Token::create("ccc".to_string()).unwrap(),
                Token::create("ddd".to_string()).unwrap(),
            )
            .unwrap(),
            pool_type: PoolType::Concentrated {
                fee_tier_bps: 500,
                tick_spacing: 10,
            },
        };
        let res = on_request_concentrated_pool_creation(
            deps.as_mut(),
            mock_env(),
            info,
            "tx_mis".to_string(),
            user,
            sample_pair_info(),
            mismatched_pool_key,
            50,
            None,
            CrossChainConfig::new(None, None, None),
        );
        assert!(res.is_err());
    }

    #[test]
    fn test_on_request_concentrated_pool_creation_happy_path_sets_reply_data() {
        let mut deps = mock_dependencies();
        let main_factory = deps.api.addr_make("main_factory");
        init_with_main_factory(&mut deps, &main_factory);
        install_main_factory_state_querier(&mut deps);

        let user = deps.api.addr_make("user");
        let info = message_info(&main_factory, &[]);
        let res = on_request_concentrated_pool_creation(
            deps.as_mut(),
            mock_env(),
            info,
            "tx_happy".to_string(),
            user.clone(),
            sample_pair_info(),
            sample_pool_key(),
            50,
            None,
            CrossChainConfig::new(None, None, None),
        )
        .unwrap();

        // Reply-data path: handler emits no submessages of its own; main
        // factory's reply handler is the only thing that runs the outbound
        // dispatch.
        assert!(res.messages.is_empty());

        // Pending entry still recorded.
        let pending = PENDING_CONCENTRATED_POOL_REQUESTS
            .load(
                deps.as_ref().storage,
                (user.clone(), "tx_happy".to_string()),
            )
            .unwrap();
        assert_eq!(pending.tx_id, "tx_happy");

        // Data payload is a PoolFactoryReply::SendPacket carrying a
        // RequestConcentratedPoolCreation packet for this sender / tx_id.
        let data = res.data.expect("reply data must be set");
        let reply: euclid::msgs::pool_factory::PoolFactoryReply =
            cosmwasm_std::from_json(&data).unwrap();
        match reply {
            euclid::msgs::pool_factory::PoolFactoryReply::SendPacket {
                msg,
                timeout,
                ack_response,
                sender,
            } => {
                assert_eq!(sender, user);
                assert!(timeout.is_none());
                assert!(ack_response.is_none());
                let router_msg: euclid_ibc::wire::envelope::router::RouterReceiveMsg =
                    cosmwasm_std::from_json(&msg).unwrap();
                assert!(matches!(
                    router_msg,
                    euclid_ibc::wire::envelope::router::RouterReceiveMsg::RequestConcentratedPoolCreation { .. }
                ));
                assert_eq!(router_msg.get_tx_id(), "tx_happy");
                assert!(router_msg.is_pool_variant());
            }
        }
    }

    #[test]
    fn test_on_request_concentrated_pool_creation_duplicate_tx_id_rejected() {
        let mut deps = mock_dependencies();
        let main_factory = deps.api.addr_make("main_factory");
        init_with_main_factory(&mut deps, &main_factory);
        install_main_factory_state_querier(&mut deps);

        let user = deps.api.addr_make("user");
        let info = message_info(&main_factory, &[]);
        on_request_concentrated_pool_creation(
            deps.as_mut(),
            mock_env(),
            info.clone(),
            "tx_dup".to_string(),
            user.clone(),
            sample_pair_info(),
            sample_pool_key(),
            50,
            None,
            CrossChainConfig::new(None, None, None),
        )
        .unwrap();
        let res = on_request_concentrated_pool_creation(
            deps.as_mut(),
            mock_env(),
            info,
            "tx_dup".to_string(),
            user,
            sample_pair_info(),
            sample_pool_key(),
            50,
            None,
            CrossChainConfig::new(None, None, None),
        );
        assert_eq!(res.unwrap_err(), ContractError::TxAlreadyExist {});
    }

    #[test]
    fn test_on_request_concentrated_pool_creation_pool_already_exists() {
        let mut deps = mock_dependencies();
        let main_factory = deps.api.addr_make("main_factory");
        init_with_main_factory(&mut deps, &main_factory);

        // Seed the concentrated map so the existence check trips.
        CONCENTRATED_VLPS
            .save(
                deps.as_mut().storage,
                sample_pool_key().to_map_key(),
                &"vlp_clp".to_string(),
            )
            .unwrap();

        let user = deps.api.addr_make("user");
        let info = message_info(&main_factory, &[]);
        let res = on_request_concentrated_pool_creation(
            deps.as_mut(),
            mock_env(),
            info,
            "tx_exists".to_string(),
            user,
            sample_pair_info(),
            sample_pool_key(),
            50,
            None,
            CrossChainConfig::new(None, None, None),
        );
        assert_eq!(res.unwrap_err(), ContractError::PoolAlreadyExists {});
    }
}
