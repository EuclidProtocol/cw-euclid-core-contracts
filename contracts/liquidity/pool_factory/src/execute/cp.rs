use cosmwasm_std::{
    ensure, to_json_binary, Addr, CosmosMsg, DepsMut, Env, MessageInfo, Response, SubMsg, Uint256,
    WasmMsg,
};
use euclid::{
    cross_chain_user::CrossChainUser,
    error::ContractError,
    fee::BPS_100_PERCENT,
    liquidity::{AddLiquidityRequest, RemoveLiquidityRequest},
    msgs::{
        cross_chain_config::CrossChainConfig, factory::ExecuteMsg as FactoryExecuteMsg,
        pool_factory::PoolFactoryReply,
    },
    token::{Pair, PairWithDenomAndAmount},
};

use crate::{
    outbound,
    state::{
        PoolCreateRequest, MAIN_FACTORY_ADDRESS, PAIR_TO_VLP, PENDING_ADD_LIQUIDITY,
        PENDING_POOL_REQUESTS, PENDING_REMOVE_LIQUIDITY,
    },
};

/// Handles a CP/Stable `RequestPoolCreation` delegated from main factory.
///
/// Caller MUST be the configured main factory. Pool factory:
///   1. validates the request and records it in `PENDING_POOL_REQUESTS`,
///   2. builds the outbound `RouterCrossChainExecuteMsg::RequestPoolCreation`
///      via `outbound::request_pool_creation`,
///   3. returns the packet as `Response::data` typed as
///      `PoolFactoryReply::SendPacket` so main factory's reply handler can
///      drive the outbound dispatch. Replaces the previous round-trip
///      through `factory::ExecuteMsg::ProxySendPacket`.
#[allow(clippy::too_many_arguments)]
pub fn on_request_pool_creation(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    tx_id: String,
    sender: Addr,
    pair_with_denom_and_amount: PairWithDenomAndAmount,
    pool_config: euclid::msgs::vlp::base::PoolConfig,
    _lp_token_name: String,
    _lp_token_symbol: String,
    _lp_token_decimal: u8,
    _lp_token_marketing: Option<cw20_base::msg::InstantiateMarketingInfo>,
    slippage_tolerance_bps: u64,
    cross_chain_config: CrossChainConfig,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;
    let main_factory = MAIN_FACTORY_ADDRESS.load(deps.storage)?;
    ensure!(info.sender == main_factory, ContractError::Unauthorized {});

    let pair = pair_with_denom_and_amount.get_pair()?;
    pair.validate()?;

    ensure!(
        !PENDING_POOL_REQUESTS.has(deps.storage, (sender.clone(), tx_id.clone())),
        ContractError::TxAlreadyExist {}
    );
    ensure!(
        !PAIR_TO_VLP.has(deps.storage, pair.get_tupple()),
        ContractError::PoolAlreadyExists {}
    );

    // Build a CrossChainUser for the outbound packet using main factory's
    // chain_uid (queried via the existing state surface). Pool factory does
    // not maintain its own chain identity — main factory is the source of
    // truth.
    let main_state: euclid::msgs::factory::StateResponse = deps.querier.query_wasm_smart(
        main_factory.clone(),
        &euclid::msgs::factory::QueryMsg::GetState {},
    )?;
    let cross_chain_sender = CrossChainUser::new(main_state.chain_uid.clone(), sender.to_string());

    PENDING_POOL_REQUESTS.save(
        deps.storage,
        (sender.clone(), tx_id.clone()),
        &PoolCreateRequest {
            tx_id: tx_id.clone(),
            sender: sender.clone(),
            pair_info: pair_with_denom_and_amount.clone(),
            lp_token_instantiate_msg: cw20_base::msg::InstantiateMsg {
                name: _lp_token_name,
                symbol: _lp_token_symbol,
                decimals: _lp_token_decimal,
                initial_balances: vec![],
                mint: None,
                marketing: _lp_token_marketing,
            },
        },
    )?;

    let packet = outbound::request_pool_creation(
        cross_chain_sender,
        tx_id.clone(),
        pair_with_denom_and_amount,
        pool_config,
        slippage_tolerance_bps,
    )?;

    let reply_data = to_json_binary(&PoolFactoryReply::SendPacket {
        msg: packet,
        timeout: cross_chain_config.timeout,
        ack_response: cross_chain_config.ack_response,
        sender,
    })?;

    Ok(Response::new()
        .add_attribute("method", "on_request_pool_creation")
        .add_attribute("tx_id", tx_id)
        .set_data(reply_data))
}

/// Handles a CP/Stable `AddLiquidity` delegated from main factory. Caller
/// MUST be the configured main factory; main factory has already deposited
/// funds to escrow upstream and generated `tx_id`.
///
/// Pool factory:
///   1. validates slippage and pair existence,
///   2. records the request in `PENDING_ADD_LIQUIDITY` keyed by (sender, tx_id),
///   3. builds the outbound `RouterCrossChainExecuteMsg::AddLiquidity` via
///      `outbound::add_liquidity`,
///   4. hands the packet to `main_factory::ProxySendPacket` for dispatch.
pub fn on_add_liquidity(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    tx_id: String,
    sender: Addr,
    pair_with_denom_and_amount: PairWithDenomAndAmount,
    slippage_tolerance_bps: u64,
    cross_chain_config: CrossChainConfig,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;
    let main_factory = MAIN_FACTORY_ADDRESS.load(deps.storage)?;
    ensure!(info.sender == main_factory, ContractError::Unauthorized {});

    ensure!(
        slippage_tolerance_bps >= 1 && slippage_tolerance_bps <= BPS_100_PERCENT,
        ContractError::InvalidSlippageTolerance {}
    );

    let pair = pair_with_denom_and_amount.get_pair()?;
    pair.validate()?;

    ensure!(
        !PENDING_ADD_LIQUIDITY.has(deps.storage, (sender.clone(), tx_id.clone())),
        ContractError::TxAlreadyExist {}
    );
    ensure!(
        PAIR_TO_VLP.has(deps.storage, pair.get_tupple()),
        ContractError::PoolDoesNotExist {}
    );

    // Build a CrossChainUser for the outbound packet using main factory's
    // chain_uid.
    let main_state: euclid::msgs::factory::StateResponse = deps.querier.query_wasm_smart(
        main_factory.clone(),
        &euclid::msgs::factory::QueryMsg::GetState {},
    )?;
    let cross_chain_sender = CrossChainUser::new(main_state.chain_uid, sender.to_string());

    PENDING_ADD_LIQUIDITY.save(
        deps.storage,
        (sender.clone(), tx_id.clone()),
        &AddLiquidityRequest {
            sender: sender.to_string(),
            tx_id: tx_id.clone(),
            pair_info: pair_with_denom_and_amount.clone(),
        },
    )?;

    let packet = outbound::add_liquidity(
        cross_chain_sender,
        tx_id.clone(),
        pair_with_denom_and_amount,
        slippage_tolerance_bps,
    )?;

    let proxy_msg = FactoryExecuteMsg::ProxySendPacket {
        msg: packet,
        timeout: cross_chain_config.timeout,
        ack_response: cross_chain_config.ack_response,
        sender,
    };
    let proxy_wasm = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: main_factory.into_string(),
        msg: to_json_binary(&proxy_msg)?,
        funds: vec![],
    });

    Ok(Response::new()
        .add_attribute("method", "on_add_liquidity")
        .add_attribute("tx_id", tx_id)
        .add_submessage(SubMsg::new(proxy_wasm)))
}

/// Handles a CP/Stable `RemoveLiquidity` delegated from main factory. Caller
/// MUST be the configured main factory; main factory already received the LP
/// cw20 tokens via the `cw20::Send` hook (and now holds them) and generated
/// `tx_id`.
///
/// Pool factory:
///   1. validates pair existence and lp_allocation > 0,
///   2. records the request in `PENDING_REMOVE_LIQUIDITY` keyed by
///      (sender, tx_id),
///   3. builds the outbound `RouterCrossChainExecuteMsg::RemoveLiquidity`
///      via `outbound::remove_liquidity`,
///   4. hands the packet to `main_factory::ProxySendPacket` for dispatch.
#[allow(clippy::too_many_arguments)]
pub fn on_remove_liquidity(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    tx_id: String,
    sender: Addr,
    pair: Pair,
    lp_allocation: Uint256,
    lp_token: Addr,
    recipient: CrossChainUser,
    cross_chain_config: CrossChainConfig,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;
    let main_factory = MAIN_FACTORY_ADDRESS.load(deps.storage)?;
    ensure!(info.sender == main_factory, ContractError::Unauthorized {});

    pair.validate()?;
    recipient.validate()?;

    ensure!(!lp_allocation.is_zero(), ContractError::ZeroAssetAmount {});

    ensure!(
        !PENDING_REMOVE_LIQUIDITY.has(deps.storage, (sender.clone(), tx_id.clone())),
        ContractError::TxAlreadyExist {}
    );
    ensure!(
        PAIR_TO_VLP.has(deps.storage, pair.get_tupple()),
        ContractError::PoolDoesNotExist {}
    );

    let main_state: euclid::msgs::factory::StateResponse = deps.querier.query_wasm_smart(
        main_factory.clone(),
        &euclid::msgs::factory::QueryMsg::GetState {},
    )?;
    let cross_chain_sender = CrossChainUser::new(main_state.chain_uid, sender.to_string());

    PENDING_REMOVE_LIQUIDITY.save(
        deps.storage,
        (sender.clone(), tx_id.clone()),
        &RemoveLiquidityRequest {
            sender: sender.to_string(),
            tx_id: tx_id.clone(),
            lp_allocation,
            pair: pair.clone(),
            lp_token,
        },
    )?;

    let packet = outbound::remove_liquidity(
        cross_chain_sender,
        tx_id.clone(),
        pair,
        lp_allocation,
        recipient,
    )?;

    let proxy_msg = FactoryExecuteMsg::ProxySendPacket {
        msg: packet,
        timeout: cross_chain_config.timeout,
        ack_response: cross_chain_config.ack_response,
        sender,
    };
    let proxy_wasm = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: main_factory.into_string(),
        msg: to_json_binary(&proxy_msg)?,
        funds: vec![],
    });

    Ok(Response::new()
        .add_attribute("method", "on_remove_liquidity")
        .add_attribute("tx_id", tx_id)
        .add_submessage(SubMsg::new(proxy_wasm)))
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
        token::{Token, TokenType, TokenWithDenomAndAmount},
    };

    use crate::testing::helpers::init_with_main_factory;

    fn sample_pair() -> PairWithDenomAndAmount {
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
    fn test_on_request_pool_creation_rejects_non_main_factory_caller() {
        let mut deps = mock_dependencies();
        let main_factory = deps.api.addr_make("main_factory");
        init_with_main_factory(&mut deps, &main_factory);

        let stranger = deps.api.addr_make("stranger");
        let user = deps.api.addr_make("user");
        let info = message_info(&stranger, &[]);
        let res = on_request_pool_creation(
            deps.as_mut(),
            mock_env(),
            info,
            "tx1".to_string(),
            user,
            sample_pair(),
            euclid::msgs::vlp::base::PoolConfig::ConstantProduct {},
            "LP".to_string(),
            "LP".to_string(),
            6,
            None,
            50,
            euclid::msgs::cross_chain_config::CrossChainConfig::new(None, None, None),
        );
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});
    }

    #[test]
    fn test_on_request_pool_creation_happy_path_sets_reply_data() {
        let mut deps = mock_dependencies();
        let main_factory = deps.api.addr_make("main_factory");
        init_with_main_factory(&mut deps, &main_factory);
        install_main_factory_state_querier(&mut deps);

        let user = deps.api.addr_make("user");
        let info = message_info(&main_factory, &[]);
        let res = on_request_pool_creation(
            deps.as_mut(),
            mock_env(),
            info,
            "tx_happy".to_string(),
            user.clone(),
            sample_pair(),
            euclid::msgs::vlp::base::PoolConfig::ConstantProduct {},
            "LP".to_string(),
            "LP".to_string(),
            6,
            None,
            50,
            euclid::msgs::cross_chain_config::CrossChainConfig::new(None, None, None),
        )
        .unwrap();

        // Reply-data path: handler emits no submessages of its own; main
        // factory's reply handler is the only thing that runs the outbound
        // dispatch.
        assert!(res.messages.is_empty());

        // Pending entry still recorded.
        let pending = PENDING_POOL_REQUESTS
            .load(
                deps.as_ref().storage,
                (user.clone(), "tx_happy".to_string()),
            )
            .unwrap();
        assert_eq!(pending.tx_id, "tx_happy");

        // Data payload is a PoolFactoryReply::SendPacket carrying a
        // RequestPoolCreation packet for this sender / tx_id.
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
                let router_msg: euclid_ibc::router_ibc::RouterCrossChainExecuteMsg =
                    cosmwasm_std::from_json(&msg).unwrap();
                assert!(matches!(
                    router_msg,
                    euclid_ibc::router_ibc::RouterCrossChainExecuteMsg::RequestPoolCreation { .. }
                ));
                assert_eq!(router_msg.get_tx_id(), "tx_happy");
                assert!(router_msg.is_pool_variant());
            }
        }
    }

    #[test]
    fn test_on_add_liquidity_rejects_non_main_factory_caller() {
        let mut deps = mock_dependencies();
        let main_factory = deps.api.addr_make("main_factory");
        init_with_main_factory(&mut deps, &main_factory);

        let stranger = deps.api.addr_make("stranger");
        let user = deps.api.addr_make("user");
        let info = message_info(&stranger, &[]);
        let res = on_add_liquidity(
            deps.as_mut(),
            mock_env(),
            info,
            "tx_unauth".to_string(),
            user,
            sample_pair(),
            50,
            euclid::msgs::cross_chain_config::CrossChainConfig::new(None, None, None),
        );
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});
    }

    #[test]
    fn test_on_add_liquidity_pool_does_not_exist() {
        let mut deps = mock_dependencies();
        let main_factory = deps.api.addr_make("main_factory");
        init_with_main_factory(&mut deps, &main_factory);

        let user = deps.api.addr_make("user");
        let info = message_info(&main_factory, &[]);
        let res = on_add_liquidity(
            deps.as_mut(),
            mock_env(),
            info,
            "tx_no_pool".to_string(),
            user,
            sample_pair(),
            50,
            euclid::msgs::cross_chain_config::CrossChainConfig::new(None, None, None),
        );
        assert_eq!(res.unwrap_err(), ContractError::PoolDoesNotExist {});
    }

    #[test]
    fn test_on_add_liquidity_invalid_slippage() {
        let mut deps = mock_dependencies();
        let main_factory = deps.api.addr_make("main_factory");
        init_with_main_factory(&mut deps, &main_factory);

        let user = deps.api.addr_make("user");
        let info = message_info(&main_factory, &[]);
        let res = on_add_liquidity(
            deps.as_mut(),
            mock_env(),
            info,
            "tx_slip".to_string(),
            user,
            sample_pair(),
            0,
            euclid::msgs::cross_chain_config::CrossChainConfig::new(None, None, None),
        );
        assert_eq!(res.unwrap_err(), ContractError::InvalidSlippageTolerance {});
    }

    #[test]
    fn test_on_add_liquidity_happy_path_emits_proxy_send() {
        let mut deps = mock_dependencies();
        let main_factory = deps.api.addr_make("main_factory");
        init_with_main_factory(&mut deps, &main_factory);
        install_main_factory_state_querier(&mut deps);

        // Seed the pair so the existence check passes.
        let pair = sample_pair().get_pair().unwrap();
        PAIR_TO_VLP
            .save(
                deps.as_mut().storage,
                pair.get_tupple(),
                &"vlp_addr".to_string(),
            )
            .unwrap();

        let user = deps.api.addr_make("user");
        let info = message_info(&main_factory, &[]);
        let res = on_add_liquidity(
            deps.as_mut(),
            mock_env(),
            info,
            "tx_al_happy".to_string(),
            user.clone(),
            sample_pair(),
            50,
            euclid::msgs::cross_chain_config::CrossChainConfig::new(None, None, None),
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        let pending = PENDING_ADD_LIQUIDITY
            .load(deps.as_ref().storage, (user, "tx_al_happy".to_string()))
            .unwrap();
        assert_eq!(pending.tx_id, "tx_al_happy");
    }

    #[test]
    fn test_on_add_liquidity_duplicate_tx_id_rejected() {
        let mut deps = mock_dependencies();
        let main_factory = deps.api.addr_make("main_factory");
        init_with_main_factory(&mut deps, &main_factory);
        install_main_factory_state_querier(&mut deps);

        let pair = sample_pair().get_pair().unwrap();
        PAIR_TO_VLP
            .save(
                deps.as_mut().storage,
                pair.get_tupple(),
                &"vlp_addr".to_string(),
            )
            .unwrap();

        let user = deps.api.addr_make("user");
        let info = message_info(&main_factory, &[]);
        on_add_liquidity(
            deps.as_mut(),
            mock_env(),
            info.clone(),
            "tx_dup".to_string(),
            user.clone(),
            sample_pair(),
            50,
            euclid::msgs::cross_chain_config::CrossChainConfig::new(None, None, None),
        )
        .unwrap();
        let res = on_add_liquidity(
            deps.as_mut(),
            mock_env(),
            info,
            "tx_dup".to_string(),
            user,
            sample_pair(),
            50,
            euclid::msgs::cross_chain_config::CrossChainConfig::new(None, None, None),
        );
        assert_eq!(res.unwrap_err(), ContractError::TxAlreadyExist {});
    }

    fn sample_recipient() -> CrossChainUser {
        CrossChainUser {
            chain_uid: ChainUid::create("testchain".to_string()).unwrap(),
            address: "user1".to_string(),
        }
    }

    #[test]
    fn test_on_remove_liquidity_rejects_non_main_factory_caller() {
        let mut deps = mock_dependencies();
        let main_factory = deps.api.addr_make("main_factory");
        init_with_main_factory(&mut deps, &main_factory);

        let stranger = deps.api.addr_make("stranger");
        let user = deps.api.addr_make("user");
        let lp_token = deps.api.addr_make("lp_token");
        let info = message_info(&stranger, &[]);
        let res = on_remove_liquidity(
            deps.as_mut(),
            mock_env(),
            info,
            "tx_unauth".to_string(),
            user,
            sample_pair().get_pair().unwrap(),
            Uint256::from(100u128),
            lp_token,
            sample_recipient(),
            euclid::msgs::cross_chain_config::CrossChainConfig::new(None, None, None),
        );
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});
    }

    #[test]
    fn test_on_remove_liquidity_pool_does_not_exist() {
        let mut deps = mock_dependencies();
        let main_factory = deps.api.addr_make("main_factory");
        init_with_main_factory(&mut deps, &main_factory);

        let user = deps.api.addr_make("user");
        let lp_token = deps.api.addr_make("lp_token");
        let info = message_info(&main_factory, &[]);
        let res = on_remove_liquidity(
            deps.as_mut(),
            mock_env(),
            info,
            "tx_no_pool".to_string(),
            user,
            sample_pair().get_pair().unwrap(),
            Uint256::from(100u128),
            lp_token,
            sample_recipient(),
            euclid::msgs::cross_chain_config::CrossChainConfig::new(None, None, None),
        );
        assert_eq!(res.unwrap_err(), ContractError::PoolDoesNotExist {});
    }

    #[test]
    fn test_on_remove_liquidity_zero_amount_rejected() {
        let mut deps = mock_dependencies();
        let main_factory = deps.api.addr_make("main_factory");
        init_with_main_factory(&mut deps, &main_factory);
        let pair = sample_pair().get_pair().unwrap();
        PAIR_TO_VLP
            .save(deps.as_mut().storage, pair.get_tupple(), &"vlp".to_string())
            .unwrap();

        let user = deps.api.addr_make("user");
        let lp_token = deps.api.addr_make("lp_token");
        let info = message_info(&main_factory, &[]);
        let res = on_remove_liquidity(
            deps.as_mut(),
            mock_env(),
            info,
            "tx_zero".to_string(),
            user,
            pair,
            Uint256::zero(),
            lp_token,
            sample_recipient(),
            euclid::msgs::cross_chain_config::CrossChainConfig::new(None, None, None),
        );
        assert_eq!(res.unwrap_err(), ContractError::ZeroAssetAmount {});
    }

    #[test]
    fn test_on_remove_liquidity_happy_path_emits_proxy_send() {
        let mut deps = mock_dependencies();
        let main_factory = deps.api.addr_make("main_factory");
        init_with_main_factory(&mut deps, &main_factory);
        install_main_factory_state_querier(&mut deps);

        let pair = sample_pair().get_pair().unwrap();
        PAIR_TO_VLP
            .save(deps.as_mut().storage, pair.get_tupple(), &"vlp".to_string())
            .unwrap();

        let user = deps.api.addr_make("user");
        let lp_token = deps.api.addr_make("lp_token");
        let info = message_info(&main_factory, &[]);
        let res = on_remove_liquidity(
            deps.as_mut(),
            mock_env(),
            info,
            "tx_rm_happy".to_string(),
            user.clone(),
            pair,
            Uint256::from(123u128),
            lp_token,
            sample_recipient(),
            euclid::msgs::cross_chain_config::CrossChainConfig::new(None, None, None),
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        let pending = PENDING_REMOVE_LIQUIDITY
            .load(deps.as_ref().storage, (user, "tx_rm_happy".to_string()))
            .unwrap();
        assert_eq!(pending.tx_id, "tx_rm_happy");
        assert_eq!(pending.lp_allocation, Uint256::from(123u128));
    }

    #[test]
    fn test_on_remove_liquidity_duplicate_tx_id_rejected() {
        let mut deps = mock_dependencies();
        let main_factory = deps.api.addr_make("main_factory");
        init_with_main_factory(&mut deps, &main_factory);
        install_main_factory_state_querier(&mut deps);
        let pair = sample_pair().get_pair().unwrap();
        PAIR_TO_VLP
            .save(deps.as_mut().storage, pair.get_tupple(), &"vlp".to_string())
            .unwrap();

        let user = deps.api.addr_make("user");
        let lp_token = deps.api.addr_make("lp_token");
        let info = message_info(&main_factory, &[]);
        on_remove_liquidity(
            deps.as_mut(),
            mock_env(),
            info.clone(),
            "tx_dup".to_string(),
            user.clone(),
            pair.clone(),
            Uint256::from(10u128),
            lp_token.clone(),
            sample_recipient(),
            euclid::msgs::cross_chain_config::CrossChainConfig::new(None, None, None),
        )
        .unwrap();
        let res = on_remove_liquidity(
            deps.as_mut(),
            mock_env(),
            info,
            "tx_dup".to_string(),
            user,
            pair,
            Uint256::from(10u128),
            lp_token,
            sample_recipient(),
            euclid::msgs::cross_chain_config::CrossChainConfig::new(None, None, None),
        );
        assert_eq!(res.unwrap_err(), ContractError::TxAlreadyExist {});
    }

    #[test]
    fn test_on_request_pool_creation_duplicate_tx_id_rejected() {
        let mut deps = mock_dependencies();
        let main_factory = deps.api.addr_make("main_factory");
        init_with_main_factory(&mut deps, &main_factory);
        install_main_factory_state_querier(&mut deps);

        let user = deps.api.addr_make("user");
        let info = message_info(&main_factory, &[]);
        on_request_pool_creation(
            deps.as_mut(),
            mock_env(),
            info.clone(),
            "tx_dup".to_string(),
            user.clone(),
            sample_pair(),
            euclid::msgs::vlp::base::PoolConfig::ConstantProduct {},
            "LP".to_string(),
            "LP".to_string(),
            6,
            None,
            50,
            euclid::msgs::cross_chain_config::CrossChainConfig::new(None, None, None),
        )
        .unwrap();
        let res = on_request_pool_creation(
            deps.as_mut(),
            mock_env(),
            info,
            "tx_dup".to_string(),
            user,
            sample_pair(),
            euclid::msgs::vlp::base::PoolConfig::ConstantProduct {},
            "LP".to_string(),
            "LP".to_string(),
            6,
            None,
            50,
            euclid::msgs::cross_chain_config::CrossChainConfig::new(None, None, None),
        );
        assert_eq!(res.unwrap_err(), ContractError::TxAlreadyExist {});
    }
}
