use cosmwasm_std::{
    ensure, from_json, to_json_binary, Binary, CosmosMsg, DepsMut, Env, MessageInfo, Response,
    SubMsg, WasmMsg,
};
use euclid::{
    error::ContractError, liquidity::AddLiquidityResponse,
    msgs::factory::ExecuteMsg as FactoryExecuteMsg,
};
use euclid_ibc::{ack::AcknowledgementMsg, router_ibc::RouterCrossChainExecuteMsg};

use crate::state::{
    MAIN_FACTORY_ADDRESS, PAIR_TO_VLP, PENDING_ADD_LIQUIDITY, PENDING_POOL_REQUESTS,
    VLP_TO_LP_TOKEN,
};

/// Dispatcher for ack callbacks forwarded from main factory's IBC ack path.
/// Caller MUST be main factory. The dispatcher does not own any of the
/// per-variant business logic — it only routes to the matching handler.
pub fn on_pool_ack(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    original_msg: Binary,
    ack: Binary,
    is_native: bool,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;
    let main_factory = MAIN_FACTORY_ADDRESS.load(deps.storage)?;
    ensure!(info.sender == main_factory, ContractError::Unauthorized {});

    let msg: RouterCrossChainExecuteMsg = from_json(&original_msg)?;
    match msg {
        RouterCrossChainExecuteMsg::RequestPoolCreation { tx_id, sender, .. } => {
            ack_pool_creation(deps, sender.address, tx_id, ack, is_native)
        }
        RouterCrossChainExecuteMsg::AddLiquidity { tx_id, sender, .. } => {
            ack_add_liquidity(deps, sender.address, tx_id, ack, is_native)
        }
        // Future slices wire the other pool variants here.
        other => Err(ContractError::new(&format!(
            "OnPoolAck: variant not yet handled by pool_factory: {}",
            std::any::type_name_of_val(&other)
        ))),
    }
}

/// Slice 1 ack path for `RequestPoolCreation`. Currently records the pool
/// registration in `PAIR_TO_VLP` and removes the pending entry. Follow-up
/// slices add escrow/LP instantiation through main factory's proxy entries.
fn ack_pool_creation(
    deps: DepsMut,
    sender: String,
    tx_id: String,
    ack: Binary,
    is_native: bool,
) -> Result<Response, ContractError> {
    let sender = deps.api.addr_validate(&sender)?;
    let req_key = (sender.clone(), tx_id.clone());
    let existing = PENDING_POOL_REQUESTS
        .may_load(deps.storage, req_key.clone())?
        .ok_or(ContractError::PoolRequestDoesNotExists { req: tx_id.clone() })?;
    PENDING_POOL_REQUESTS.remove(deps.storage, req_key);

    let res: AcknowledgementMsg<AddLiquidityResponse> = from_json(&ack)?;
    match res {
        AcknowledgementMsg::Ok(data) => {
            PAIR_TO_VLP.save(
                deps.storage,
                existing.pair_info.get_pair()?.get_tupple(),
                &data.vlp_address.clone(),
            )?;
            Ok(Response::new()
                .add_attribute("method", "pool_factory_ack_pool_creation")
                .add_attribute("tx_id", tx_id)
                .add_attribute("vlp", data.vlp_address))
        }
        AcknowledgementMsg::Error(err) => {
            if is_native {
                return Err(ContractError::new(&err));
            }
            Ok(Response::new()
                .add_attribute("method", "pool_factory_reject_pool_request")
                .add_attribute("tx_id", tx_id)
                .add_attribute("error", err))
        }
    }
}

/// Slice 2 ack path for `AddLiquidity`. Funds were deposited to escrow
/// upstream by main factory before delegation, so on success we only need to
/// mint the LP tokens (via main factory's `ProxyMintLpToken`); on failure we
/// release each non-voucher escrow back to the user (via main factory's
/// `ProxyReleaseEscrow`).
fn ack_add_liquidity(
    deps: DepsMut,
    sender: String,
    tx_id: String,
    ack: Binary,
    is_native: bool,
) -> Result<Response, ContractError> {
    let sender = deps.api.addr_validate(&sender)?;
    let req_key = (sender.clone(), tx_id.clone());
    let liquidity_info = PENDING_ADD_LIQUIDITY.load(deps.storage, req_key.clone())?;
    PENDING_ADD_LIQUIDITY.remove(deps.storage, req_key);

    let main_factory = MAIN_FACTORY_ADDRESS.load(deps.storage)?;
    let res: AcknowledgementMsg<AddLiquidityResponse> = from_json(&ack)?;
    match res {
        AcknowledgementMsg::Ok(data) => {
            let lp_token = VLP_TO_LP_TOKEN.load(deps.storage, data.vlp_address.clone())?;
            let proxy_msg = FactoryExecuteMsg::ProxyMintLpToken {
                lp_token,
                recipient: liquidity_info.sender,
                amount: data.mint_lp_tokens,
            };
            let mint_call = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: main_factory.into_string(),
                msg: to_json_binary(&proxy_msg)?,
                funds: vec![],
            });
            Ok(Response::new()
                .add_attribute("method", "pool_factory_ack_add_liquidity")
                .add_attribute("tx_id", tx_id)
                .add_attribute("vlp", data.vlp_address)
                .add_attribute("sender", sender)
                .add_submessage(SubMsg::new(mint_call)))
        }
        AcknowledgementMsg::Error(err) => {
            if is_native {
                return Err(ContractError::new(&err));
            }
            // Refund each non-voucher token deposited upstream by main factory.
            let mut response =
                Response::new().add_attribute("method", "pool_factory_ack_add_liquidity_refund");
            for token_info in liquidity_info.pair_info.get_vec_token_info() {
                if token_info.token_type.is_voucher() {
                    continue;
                }
                let proxy_msg = FactoryExecuteMsg::ProxyReleaseEscrow {
                    token: token_info.token.clone(),
                    denom: token_info.token_type.clone(),
                    recipient: sender.to_string(),
                    amount: token_info.amount,
                };
                let release_call = CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: main_factory.clone().into_string(),
                    msg: to_json_binary(&proxy_msg)?,
                    funds: vec![],
                });
                response = response.add_submessage(SubMsg::new(release_call));
            }
            Ok(response
                .add_attribute("tx_id", tx_id)
                .add_attribute("sender", sender)
                .add_attribute("error", err))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::{
        testing::{message_info, mock_dependencies, mock_env},
        to_json_binary, Addr, Uint256,
    };
    use euclid::{
        cross_chain_user::CrossChainUser,
        liquidity::AddLiquidityRequest,
        token::{PairWithDenomAndAmount, Token, TokenType, TokenWithDenomAndAmount},
    };

    use crate::testing::helpers::init_with_main_factory;

    fn sample_pair_with_denom() -> PairWithDenomAndAmount {
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

    fn add_liquidity_original_msg(sender_addr: &str, tx_id: &str) -> Binary {
        to_json_binary(&RouterCrossChainExecuteMsg::AddLiquidity {
            sender: CrossChainUser {
                chain_uid: euclid::chain::ChainUid::create("testchain".to_string()).unwrap(),
                address: sender_addr.to_string(),
            },
            slippage_tolerance_bps: 50,
            pair: sample_pair_with_denom(),
            tx_id: tx_id.to_string(),
        })
        .unwrap()
    }

    fn seed_pending_add_liquidity(
        deps: &mut crate::testing::helpers::MockDeps,
        sender: &Addr,
        tx_id: &str,
    ) {
        PENDING_ADD_LIQUIDITY
            .save(
                deps.as_mut().storage,
                (sender.clone(), tx_id.to_string()),
                &AddLiquidityRequest {
                    sender: sender.to_string(),
                    tx_id: tx_id.to_string(),
                    pair_info: sample_pair_with_denom(),
                },
            )
            .unwrap();
    }

    #[test]
    fn test_on_pool_ack_unauthorised_caller_rejected() {
        let mut deps = mock_dependencies();
        let main_factory = deps.api.addr_make("main_factory");
        init_with_main_factory(&mut deps, &main_factory);

        let stranger = deps.api.addr_make("stranger");
        let info = message_info(&stranger, &[]);
        let user = deps.api.addr_make("user");
        let original = add_liquidity_original_msg(user.as_str(), "tx_x");
        let ack_ok = to_json_binary(&AcknowledgementMsg::<AddLiquidityResponse>::Ok(
            AddLiquidityResponse {
                mint_lp_tokens: Uint256::from(10u128),
                vlp_address: "vlp1".to_string(),
                tx_id: "tx_x".to_string(),
                sender: CrossChainUser {
                    chain_uid: euclid::chain::ChainUid::create("testchain".to_string()).unwrap(),
                    address: user.to_string(),
                },
            },
        ))
        .unwrap();
        let res = on_pool_ack(deps.as_mut(), mock_env(), info, original, ack_ok, false);
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});
    }

    #[test]
    fn test_ack_add_liquidity_success_emits_proxy_mint_lp_token() {
        let mut deps = mock_dependencies();
        let main_factory = deps.api.addr_make("main_factory");
        init_with_main_factory(&mut deps, &main_factory);

        // Seed VLP_TO_LP_TOKEN so the lookup succeeds.
        let lp_token = deps.api.addr_make("lp_token");
        VLP_TO_LP_TOKEN
            .save(deps.as_mut().storage, "vlp1".to_string(), &lp_token)
            .unwrap();

        let user = deps.api.addr_make("user");
        seed_pending_add_liquidity(&mut deps, &user, "tx_ok");

        let info = message_info(&main_factory, &[]);
        let original = add_liquidity_original_msg(user.as_str(), "tx_ok");
        let ack_ok = to_json_binary(&AcknowledgementMsg::<AddLiquidityResponse>::Ok(
            AddLiquidityResponse {
                mint_lp_tokens: Uint256::from(123u128),
                vlp_address: "vlp1".to_string(),
                tx_id: "tx_ok".to_string(),
                sender: CrossChainUser {
                    chain_uid: euclid::chain::ChainUid::create("testchain".to_string()).unwrap(),
                    address: user.to_string(),
                },
            },
        ))
        .unwrap();
        let res = on_pool_ack(deps.as_mut(), mock_env(), info, original, ack_ok, false).unwrap();
        // One submessage: proxy_mint_lp_token call to main factory.
        assert_eq!(res.messages.len(), 1);
        assert!(res
            .attributes
            .iter()
            .any(|a| a.key == "method" && a.value == "pool_factory_ack_add_liquidity"));
        // Pending entry was removed.
        assert!(!PENDING_ADD_LIQUIDITY.has(deps.as_ref().storage, (user, "tx_ok".to_string())));
    }

    #[test]
    fn test_ack_add_liquidity_failure_emits_proxy_release_escrow_per_token() {
        let mut deps = mock_dependencies();
        let main_factory = deps.api.addr_make("main_factory");
        init_with_main_factory(&mut deps, &main_factory);

        let user = deps.api.addr_make("user");
        seed_pending_add_liquidity(&mut deps, &user, "tx_fail");

        let info = message_info(&main_factory, &[]);
        let original = add_liquidity_original_msg(user.as_str(), "tx_fail");
        let ack_err = to_json_binary(&AcknowledgementMsg::<AddLiquidityResponse>::Error(
            "slippage rejected".to_string(),
        ))
        .unwrap();
        let res = on_pool_ack(deps.as_mut(), mock_env(), info, original, ack_err, false).unwrap();
        // Two submessages, one per non-voucher token in the pair.
        assert_eq!(res.messages.len(), 2);
        assert!(res
            .attributes
            .iter()
            .any(|a| a.key == "method" && a.value == "pool_factory_ack_add_liquidity_refund"));
    }

    #[test]
    fn test_ack_add_liquidity_failure_native_returns_error() {
        let mut deps = mock_dependencies();
        let main_factory = deps.api.addr_make("main_factory");
        init_with_main_factory(&mut deps, &main_factory);

        let user = deps.api.addr_make("user");
        seed_pending_add_liquidity(&mut deps, &user, "tx_native_fail");

        let info = message_info(&main_factory, &[]);
        let original = add_liquidity_original_msg(user.as_str(), "tx_native_fail");
        let ack_err = to_json_binary(&AcknowledgementMsg::<AddLiquidityResponse>::Error(
            "native rejected".to_string(),
        ))
        .unwrap();
        let res = on_pool_ack(deps.as_mut(), mock_env(), info, original, ack_err, true);
        assert!(res.is_err());
    }
}
