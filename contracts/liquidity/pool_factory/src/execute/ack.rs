use cosmwasm_std::{
    ensure, from_json, to_json_binary, Binary, CosmosMsg, DepsMut, Env, Int256, MessageInfo,
    Response, SubMsg, WasmMsg,
};
use euclid::{
    error::ContractError,
    liquidity::{AddLiquidityResponse, ConcentratedAddLiquidityResponse, RemoveLiquidityResponse},
    msgs::factory::ExecuteMsg as FactoryExecuteMsg,
};
use euclid_ibc::{
    ack::AcknowledgementMsg,
    router_ibc::{
        RouterCrossChainConcentratedRequestPoolCreationExecuteMsg, RouterCrossChainExecuteMsg,
    },
};

use crate::state::{
    CONCENTRATED_VLPS, MAIN_FACTORY_ADDRESS, PAIR_TO_VLP, PENDING_ADD_LIQUIDITY,
    PENDING_CONCENTRATED_POOL_REQUESTS, PENDING_POOL_REQUESTS, PENDING_REMOVE_LIQUIDITY,
    VLP_TO_LP_SHARES, VLP_TO_LP_TOKEN,
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
        RouterCrossChainExecuteMsg::RemoveLiquidity(msg) => {
            ack_remove_liquidity(deps, msg.sender.address, msg.tx_id, ack, is_native)
        }
        RouterCrossChainExecuteMsg::RequestConcentratedPoolCreation(msg) => {
            ack_concentrated_pool_creation(deps, msg, ack, is_native)
        }
        // Future slices wire the other pool variants here.
        other => Err(ContractError::new(&format!(
            "OnPoolAck: variant not yet handled by pool_factory: {}",
            std::any::type_name_of_val(&other)
        ))),
    }
}

/// Slice 4 ack path for `RequestConcentratedPoolCreation`. Records the
/// concentrated pool's VLP into `CONCENTRATED_VLPS` on success. Position-NFT
/// mint, per-token escrow funding, and the failure-side refund proxy calls are
/// the documented Slice 4 carry-overs: main factory's pre-Slice-4 ack handler
/// retains authority over those side-effects, and the Slice 4 integration test
/// uses the slice-2-style migrate-and-bridge helper to pre-populate state for
/// follow-on flows.
fn ack_concentrated_pool_creation(
    deps: DepsMut,
    msg: RouterCrossChainConcentratedRequestPoolCreationExecuteMsg,
    ack: Binary,
    is_native: bool,
) -> Result<Response, ContractError> {
    let sender = deps.api.addr_validate(&msg.sender.address)?;
    let req_key = (sender.clone(), msg.tx_id.clone());
    let _existing = PENDING_CONCENTRATED_POOL_REQUESTS
        .may_load(deps.storage, req_key.clone())?
        .ok_or(ContractError::NotFound {
            msg: "pending concentrated pool request not found".to_string(),
        })?;
    PENDING_CONCENTRATED_POOL_REQUESTS.remove(deps.storage, req_key);

    let res: AcknowledgementMsg<ConcentratedAddLiquidityResponse> = from_json(&ack)?;
    match res {
        AcknowledgementMsg::Ok(data) => {
            CONCENTRATED_VLPS.save(
                deps.storage,
                msg.pool_key.to_map_key(),
                &data.vlp_address.clone(),
            )?;
            Ok(Response::new()
                .add_attribute("method", "pool_factory_ack_concentrated_pool_creation")
                .add_attribute("tx_id", msg.tx_id)
                .add_attribute("vlp", data.vlp_address)
                .add_attribute("position_id", data.position_id))
        }
        AcknowledgementMsg::Error(err) => {
            if is_native {
                return Err(ContractError::new(&err));
            }
            Ok(Response::new()
                .add_attribute("method", "pool_factory_reject_concentrated_pool_request")
                .add_attribute("tx_id", msg.tx_id)
                .add_attribute("error", err))
        }
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

/// Slice 3 ack path for `RemoveLiquidity`. Main factory holds the LP cw20
/// tokens (they arrived via the `cw20::Send` hook before delegation). On
/// success we burn them through main factory's `ProxyBurnLpToken`; on failure
/// we return them to the user through `ProxyTransferLpToken`. Escrow payouts
/// for the underlying are not driven here — the hub returns vouchers and the
/// user settles them via the existing `release_voucher` surface.
fn ack_remove_liquidity(
    deps: DepsMut,
    sender: String,
    tx_id: String,
    ack: Binary,
    is_native: bool,
) -> Result<Response, ContractError> {
    let sender = deps.api.addr_validate(&sender)?;
    let req_key = (sender.clone(), tx_id.clone());
    let liquidity_info = PENDING_REMOVE_LIQUIDITY
        .may_load(deps.storage, req_key.clone())?
        .ok_or(ContractError::NotFound {
            msg: "pending remove liquidity request not found".to_string(),
        })?;
    PENDING_REMOVE_LIQUIDITY.remove(deps.storage, req_key);

    let main_factory = MAIN_FACTORY_ADDRESS.load(deps.storage)?;
    let res: AcknowledgementMsg<RemoveLiquidityResponse> = from_json(&ack)?;
    match res {
        AcknowledgementMsg::Ok(data) => {
            let shares = VLP_TO_LP_SHARES
                .may_load(deps.storage, data.vlp_address.clone())?
                .unwrap_or(Int256::zero());
            let shares = shares.checked_sub(Int256::from(
                cosmwasm_std::Uint128::try_from(data.burn_lp_tokens)
                    .map_err(|e| ContractError::Std(e.into()))?,
            ))?;
            VLP_TO_LP_SHARES.save(deps.storage, data.vlp_address.clone(), &shares)?;

            let proxy_msg = FactoryExecuteMsg::ProxyBurnLpToken {
                lp_token: liquidity_info.lp_token,
                amount: liquidity_info.lp_allocation,
            };
            let burn_call = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: main_factory.into_string(),
                msg: to_json_binary(&proxy_msg)?,
                funds: vec![],
            });
            Ok(Response::new()
                .add_attribute("method", "pool_factory_ack_remove_liquidity")
                .add_attribute("tx_id", tx_id)
                .add_attribute("vlp", data.vlp_address)
                .add_attribute("sender", sender)
                .add_submessage(SubMsg::new(burn_call)))
        }
        AcknowledgementMsg::Error(err) => {
            if is_native {
                return Err(ContractError::new(&err));
            }
            let proxy_msg = FactoryExecuteMsg::ProxyTransferLpToken {
                lp_token: liquidity_info.lp_token,
                recipient: sender.to_string(),
                amount: liquidity_info.lp_allocation,
            };
            let return_call = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: main_factory.into_string(),
                msg: to_json_binary(&proxy_msg)?,
                funds: vec![],
            });
            Ok(Response::new()
                .add_attribute("method", "pool_factory_ack_remove_liquidity_refund")
                .add_attribute("tx_id", tx_id)
                .add_attribute("sender", sender)
                .add_attribute("error", err)
                .add_submessage(SubMsg::new(return_call)))
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
        liquidity::{AddLiquidityRequest, RemoveLiquidityRequest},
        token::{
            Pair, PairWithAmount, PairWithDenomAndAmount, Token, TokenType, TokenWithAmount,
            TokenWithDenomAndAmount,
        },
    };
    use euclid_ibc::router_ibc::RouterCrossChainRemoveLiquidityExecuteMsg;

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

    // -----------------------------------------------------------------------
    // ack_remove_liquidity
    // -----------------------------------------------------------------------

    fn sample_pair() -> Pair {
        sample_pair_with_denom().get_pair().unwrap()
    }

    fn remove_liquidity_original_msg(sender_addr: &str, tx_id: &str) -> Binary {
        to_json_binary(&RouterCrossChainExecuteMsg::RemoveLiquidity(
            RouterCrossChainRemoveLiquidityExecuteMsg {
                sender: CrossChainUser {
                    chain_uid: euclid::chain::ChainUid::create("testchain".to_string()).unwrap(),
                    address: sender_addr.to_string(),
                },
                lp_allocation: Uint256::from(50u128),
                pair: sample_pair(),
                recipient: CrossChainUser {
                    chain_uid: euclid::chain::ChainUid::create("testchain".to_string()).unwrap(),
                    address: sender_addr.to_string(),
                },
                tx_id: tx_id.to_string(),
            },
        ))
        .unwrap()
    }

    fn seed_pending_remove_liquidity(
        deps: &mut crate::testing::helpers::MockDeps,
        sender: &Addr,
        tx_id: &str,
        lp_token: &Addr,
    ) {
        PENDING_REMOVE_LIQUIDITY
            .save(
                deps.as_mut().storage,
                (sender.clone(), tx_id.to_string()),
                &RemoveLiquidityRequest {
                    sender: sender.to_string(),
                    tx_id: tx_id.to_string(),
                    lp_allocation: Uint256::from(50u128),
                    pair: sample_pair(),
                    lp_token: lp_token.clone(),
                },
            )
            .unwrap();
    }

    fn remove_liquidity_response_ok(vlp: &str, burn: u128) -> RemoveLiquidityResponse {
        RemoveLiquidityResponse {
            liquidity_removed: PairWithAmount {
                token_1: TokenWithAmount {
                    token: Token::create("aaa".to_string()).unwrap(),
                    amount: Uint256::from(10u128),
                },
                token_2: TokenWithAmount {
                    token: Token::create("bbb".to_string()).unwrap(),
                    amount: Uint256::from(10u128),
                },
            },
            burn_lp_tokens: Uint256::from(burn),
            vlp_address: vlp.to_string(),
        }
    }

    #[test]
    fn test_ack_remove_liquidity_success_emits_proxy_burn_lp_token() {
        let mut deps = mock_dependencies();
        let main_factory = deps.api.addr_make("main_factory");
        init_with_main_factory(&mut deps, &main_factory);

        let user = deps.api.addr_make("user");
        let lp_token = deps.api.addr_make("lp_token");
        seed_pending_remove_liquidity(&mut deps, &user, "tx_rm_ok", &lp_token);

        let info = message_info(&main_factory, &[]);
        let original = remove_liquidity_original_msg(user.as_str(), "tx_rm_ok");
        let ack_ok = to_json_binary(&AcknowledgementMsg::<RemoveLiquidityResponse>::Ok(
            remove_liquidity_response_ok("vlp_rm", 50),
        ))
        .unwrap();
        let res = on_pool_ack(deps.as_mut(), mock_env(), info, original, ack_ok, false).unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(res
            .attributes
            .iter()
            .any(|a| a.key == "method" && a.value == "pool_factory_ack_remove_liquidity"));
        // Pending entry was removed.
        assert!(
            !PENDING_REMOVE_LIQUIDITY.has(deps.as_ref().storage, (user, "tx_rm_ok".to_string()))
        );
        // VLP_TO_LP_SHARES decremented by burn_lp_tokens.
        let shares = VLP_TO_LP_SHARES
            .load(deps.as_ref().storage, "vlp_rm".to_string())
            .unwrap();
        assert_eq!(shares, Int256::from(-50i64));
    }

    #[test]
    fn test_ack_remove_liquidity_failure_emits_proxy_transfer_lp_token() {
        let mut deps = mock_dependencies();
        let main_factory = deps.api.addr_make("main_factory");
        init_with_main_factory(&mut deps, &main_factory);

        let user = deps.api.addr_make("user");
        let lp_token = deps.api.addr_make("lp_token");
        seed_pending_remove_liquidity(&mut deps, &user, "tx_rm_fail", &lp_token);

        let info = message_info(&main_factory, &[]);
        let original = remove_liquidity_original_msg(user.as_str(), "tx_rm_fail");
        let ack_err = to_json_binary(&AcknowledgementMsg::<RemoveLiquidityResponse>::Error(
            "rejected".to_string(),
        ))
        .unwrap();
        let res = on_pool_ack(deps.as_mut(), mock_env(), info, original, ack_err, false).unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(res
            .attributes
            .iter()
            .any(|a| a.key == "method" && a.value == "pool_factory_ack_remove_liquidity_refund"));
    }

    #[test]
    fn test_ack_remove_liquidity_failure_native_returns_error() {
        let mut deps = mock_dependencies();
        let main_factory = deps.api.addr_make("main_factory");
        init_with_main_factory(&mut deps, &main_factory);

        let user = deps.api.addr_make("user");
        let lp_token = deps.api.addr_make("lp_token");
        seed_pending_remove_liquidity(&mut deps, &user, "tx_rm_native_fail", &lp_token);

        let info = message_info(&main_factory, &[]);
        let original = remove_liquidity_original_msg(user.as_str(), "tx_rm_native_fail");
        let ack_err = to_json_binary(&AcknowledgementMsg::<RemoveLiquidityResponse>::Error(
            "native rejected".to_string(),
        ))
        .unwrap();
        let res = on_pool_ack(deps.as_mut(), mock_env(), info, original, ack_err, true);
        assert!(res.is_err());
    }

    // -----------------------------------------------------------------------
    // ack_concentrated_pool_creation
    // -----------------------------------------------------------------------

    fn sample_concentrated_pool_key() -> euclid::msgs::vlp::base::PoolKey {
        euclid::msgs::vlp::base::PoolKey {
            pair: sample_pair(),
            pool_type: euclid::msgs::vlp::base::PoolType::Concentrated {
                fee_tier_bps: 500,
                tick_spacing: 10,
            },
        }
    }

    fn concentrated_pool_creation_original_msg(sender_addr: &str, tx_id: &str) -> Binary {
        to_json_binary(
            &RouterCrossChainExecuteMsg::RequestConcentratedPoolCreation(
                RouterCrossChainConcentratedRequestPoolCreationExecuteMsg {
                    sender: CrossChainUser {
                        chain_uid: euclid::chain::ChainUid::create("testchain".to_string())
                            .unwrap(),
                        address: sender_addr.to_string(),
                    },
                    tx_id: tx_id.to_string(),
                    pair: sample_pair_with_denom(),
                    pool_key: sample_concentrated_pool_key(),
                    slippage_tolerance_bps: 50,
                    initial_tick: None,
                },
            ),
        )
        .unwrap()
    }

    fn seed_pending_concentrated_pool_request(
        deps: &mut crate::testing::helpers::MockDeps,
        sender: &Addr,
        tx_id: &str,
    ) {
        use crate::state::{ConcentratedPoolCreateRequest, PENDING_CONCENTRATED_POOL_REQUESTS};
        PENDING_CONCENTRATED_POOL_REQUESTS
            .save(
                deps.as_mut().storage,
                (sender.clone(), tx_id.to_string()),
                &ConcentratedPoolCreateRequest {
                    tx_id: tx_id.to_string(),
                    sender: sender.clone(),
                    pair_info: sample_pair_with_denom(),
                    pool_key: sample_concentrated_pool_key(),
                },
            )
            .unwrap();
    }

    fn concentrated_pool_creation_response_ok(
        vlp: &str,
        position_id: u128,
    ) -> euclid::liquidity::ConcentratedAddLiquidityResponse {
        euclid::liquidity::ConcentratedAddLiquidityResponse {
            vlp_address: vlp.to_string(),
            tx_id: "tx_concentrated".to_string(),
            sender: CrossChainUser {
                chain_uid: euclid::chain::ChainUid::create("testchain".to_string()).unwrap(),
                address: "user1".to_string(),
            },
            position_id: cosmwasm_std::Uint128::from(position_id),
            liquidity_delta: cosmwasm_std::Uint128::from(1_000u128),
        }
    }

    #[test]
    fn test_ack_concentrated_pool_creation_success_writes_concentrated_vlps() {
        use crate::state::CONCENTRATED_VLPS;
        let mut deps = mock_dependencies();
        let main_factory = deps.api.addr_make("main_factory");
        init_with_main_factory(&mut deps, &main_factory);

        let user = deps.api.addr_make("user_clp");
        seed_pending_concentrated_pool_request(&mut deps, &user, "tx_clp_ok");

        let info = message_info(&main_factory, &[]);
        let original = concentrated_pool_creation_original_msg(user.as_str(), "tx_clp_ok");
        let ack_ok = to_json_binary(&AcknowledgementMsg::Ok(
            concentrated_pool_creation_response_ok("vlp_clp", 1),
        ))
        .unwrap();
        let res = on_pool_ack(deps.as_mut(), mock_env(), info, original, ack_ok, false).unwrap();
        // No outbound submessages in Slice 4 — position mint + escrow funding
        // remain main-factory-side carry-overs.
        assert_eq!(res.messages.len(), 0);
        let stored = CONCENTRATED_VLPS
            .load(
                deps.as_ref().storage,
                sample_concentrated_pool_key().to_map_key(),
            )
            .unwrap();
        assert_eq!(stored, "vlp_clp");
        // Pending entry was removed.
        use crate::state::PENDING_CONCENTRATED_POOL_REQUESTS;
        assert!(!PENDING_CONCENTRATED_POOL_REQUESTS
            .has(deps.as_ref().storage, (user, "tx_clp_ok".to_string())));
    }

    #[test]
    fn test_ack_concentrated_pool_creation_failure_non_native_logs_only() {
        let mut deps = mock_dependencies();
        let main_factory = deps.api.addr_make("main_factory");
        init_with_main_factory(&mut deps, &main_factory);

        let user = deps.api.addr_make("user_clp");
        seed_pending_concentrated_pool_request(&mut deps, &user, "tx_clp_fail");

        let info = message_info(&main_factory, &[]);
        let original = concentrated_pool_creation_original_msg(user.as_str(), "tx_clp_fail");
        let ack_err = to_json_binary(&AcknowledgementMsg::<
            euclid::liquidity::ConcentratedAddLiquidityResponse,
        >::Error("hub rejected".to_string()))
        .unwrap();
        let res = on_pool_ack(deps.as_mut(), mock_env(), info, original, ack_err, false).unwrap();
        assert_eq!(res.messages.len(), 0);
        assert!(res.attributes.iter().any(
            |a| a.key == "method" && a.value == "pool_factory_reject_concentrated_pool_request"
        ));
    }

    #[test]
    fn test_ack_concentrated_pool_creation_failure_native_returns_error() {
        let mut deps = mock_dependencies();
        let main_factory = deps.api.addr_make("main_factory");
        init_with_main_factory(&mut deps, &main_factory);

        let user = deps.api.addr_make("user_clp");
        seed_pending_concentrated_pool_request(&mut deps, &user, "tx_clp_native_fail");

        let info = message_info(&main_factory, &[]);
        let original = concentrated_pool_creation_original_msg(user.as_str(), "tx_clp_native_fail");
        let ack_err = to_json_binary(&AcknowledgementMsg::<
            euclid::liquidity::ConcentratedAddLiquidityResponse,
        >::Error("native rejected".to_string()))
        .unwrap();
        let res = on_pool_ack(deps.as_mut(), mock_env(), info, original, ack_err, true);
        assert!(res.is_err());
    }

    #[test]
    fn test_ack_concentrated_pool_creation_missing_pending_returns_error() {
        let mut deps = mock_dependencies();
        let main_factory = deps.api.addr_make("main_factory");
        init_with_main_factory(&mut deps, &main_factory);

        let user = deps.api.addr_make("user_clp");
        let info = message_info(&main_factory, &[]);
        let original = concentrated_pool_creation_original_msg(user.as_str(), "tx_missing");
        let ack_ok = to_json_binary(&AcknowledgementMsg::Ok(
            concentrated_pool_creation_response_ok("vlp_x", 1),
        ))
        .unwrap();
        let res = on_pool_ack(deps.as_mut(), mock_env(), info, original, ack_ok, false);
        assert!(res.is_err());
    }

    #[test]
    fn test_ack_remove_liquidity_missing_pending_returns_error() {
        let mut deps = mock_dependencies();
        let main_factory = deps.api.addr_make("main_factory");
        init_with_main_factory(&mut deps, &main_factory);

        let user = deps.api.addr_make("user");
        let info = message_info(&main_factory, &[]);
        let original = remove_liquidity_original_msg(user.as_str(), "tx_missing");
        let ack_ok = to_json_binary(&AcknowledgementMsg::<RemoveLiquidityResponse>::Ok(
            remove_liquidity_response_ok("vlp_rm", 10),
        ))
        .unwrap();
        let res = on_pool_ack(deps.as_mut(), mock_env(), info, original, ack_ok, false);
        assert!(res.is_err());
    }
}
