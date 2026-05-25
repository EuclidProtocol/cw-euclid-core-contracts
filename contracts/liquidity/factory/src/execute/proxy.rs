use cosmwasm_std::{
    ensure, from_json, to_json_binary, Addr, Binary, CosmosMsg, DepsMut, Env, MessageInfo,
    Response, SubMsg, Uint128, Uint256, WasmMsg,
};
use euclid::{
    chain::ChainType,
    error::ContractError,
    msgs::{
        escrow::ExecuteMsg as EscrowExecuteMsg,
        factory::ExecuteMsg,
        position_token::{self, MintMsg as PositionMintMsg},
    },
    token::{Token, TokenType},
};
use euclid_ibc::router_ibc::RouterCrossChainExecuteMsg;

use crate::{
    query::get_chain_type,
    reply::RELEASE_ESCROW_REPLY_ID,
    state::{
        ADMIN, POOL_FACTORY_ADDRESS, POOL_FACTORY_INITIALISED, POSITION_TOKEN_CONTRACT, STATE,
        TOKEN_TO_ESCROW,
    },
};

/// Bootstrap entry — wires main Factory to a freshly deployed pool_factory.
/// Auth: migration admin. One-shot: rejects once `POOL_FACTORY_INITIALISED`
/// is true. The Sirius drain-and-cut migration writes these same items
/// inside its migrate flow; this entry is the fresh-chain bootstrap path.
pub fn execute_set_pool_factory(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    pool_factory_address: String,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;
    let admins = ADMIN.load(deps.storage)?;
    ensure!(
        info.sender == admins.migration_admin,
        ContractError::Unauthorized {}
    );
    let initialised = POOL_FACTORY_INITIALISED
        .may_load(deps.storage)?
        .unwrap_or(false);
    ensure!(
        !initialised,
        ContractError::new("Pool factory already initialised")
    );

    let pool_factory_addr = deps.api.addr_validate(&pool_factory_address)?;
    POOL_FACTORY_ADDRESS.save(deps.storage, &pool_factory_addr)?;
    POOL_FACTORY_INITIALISED.save(deps.storage, &true)?;

    Ok(Response::new()
        .add_attribute("method", "set_pool_factory")
        .add_attribute("pool_factory_address", pool_factory_addr))
}

/// Authorised proxy entry used by `pool_factory` to dispatch a pool-related
/// `RouterCrossChainExecuteMsg` through main Factory's existing transport.
///
/// The body deserialises the packed binary back into the typed enum and calls
/// the same `to_msg` factory uses today, so both Cosmos (SendPacket → IBC) and
/// Native (direct router callback) branches behave identically to the
/// pre-refactor flow.
pub fn execute_proxy_send_packet(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: Binary,
    timeout: Option<u64>,
    ack_response: Option<Binary>,
    sender: Addr,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;
    let pool_factory = POOL_FACTORY_ADDRESS
        .may_load(deps.storage)?
        .ok_or(ContractError::Unauthorized {})?;
    ensure!(info.sender == pool_factory, ContractError::Unauthorized {});

    let router_msg: RouterCrossChainExecuteMsg = from_json(&msg)?;
    let tx_id = router_msg.get_tx_id();

    let state = STATE.load(deps.storage)?;
    let chain_type: ChainType = get_chain_type(deps.as_ref(), &env)?;

    let submsg = match chain_type {
        ChainType::Native {} | ChainType::Cosmos(_) => router_msg.to_msg(
            &mut deps,
            &env,
            state.router_contract.clone(),
            sender.clone(),
            state.chain_uid.clone(),
            chain_type,
            timeout,
            ack_response,
        )?,
        _ => {
            return Err(ContractError::new(
                "Pool factory proxy only supports cosmos/native chain types",
            ));
        }
    };

    Ok(Response::new()
        .add_attribute("method", "proxy_send_packet")
        .add_attribute("pool_factory", pool_factory)
        .add_attribute("tx_id", tx_id)
        .add_submessage(submsg))
}

/// Authorised proxy entry used by `pool_factory` to mint LP cw20 tokens to a
/// recipient after a successful add-liquidity ack. Main Factory remains the
/// cw20 minter for every LP token; this entry exposes a narrow auth-gated
/// surface for pool_factory to drive the mint without owning the minter
/// authority itself.
pub fn execute_proxy_mint_lp_token(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    lp_token: Addr,
    recipient: String,
    amount: Uint256,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;
    let pool_factory = POOL_FACTORY_ADDRESS
        .may_load(deps.storage)?
        .ok_or(ContractError::Unauthorized {})?;
    ensure!(info.sender == pool_factory, ContractError::Unauthorized {});

    let recipient_addr = deps.api.addr_validate(&recipient)?;
    let mint_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: lp_token.clone().into_string(),
        msg: to_json_binary(&euclid::msgs::lp_token::msg::ExecuteMsg::Mint {
            recipient: recipient_addr.to_string(),
            amount,
        })?,
        funds: vec![],
    });

    Ok(Response::new()
        .add_attribute("method", "proxy_mint_lp_token")
        .add_attribute("pool_factory", pool_factory)
        .add_attribute("lp_token", lp_token)
        .add_attribute("recipient", recipient_addr)
        .add_attribute("amount", amount.to_string())
        .add_message(mint_msg))
}

/// Authorised proxy entry used by `pool_factory` to burn LP cw20 tokens held
/// by main factory after a successful remove-liquidity ack. The LP tokens
/// arrived on main factory via the `cw20::Send` hook before delegation, so
/// the burn message executes from main factory's address and destroys tokens
/// from main factory's own balance — matching the pre-refactor behaviour.
pub fn execute_proxy_burn_lp_token(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    lp_token: Addr,
    amount: Uint256,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;
    let pool_factory = POOL_FACTORY_ADDRESS
        .may_load(deps.storage)?
        .ok_or(ContractError::Unauthorized {})?;
    ensure!(info.sender == pool_factory, ContractError::Unauthorized {});

    let burn_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: lp_token.clone().into_string(),
        msg: to_json_binary(&euclid::msgs::lp_token::msg::ExecuteMsg::Burn { amount })?,
        funds: vec![],
    });

    Ok(Response::new()
        .add_attribute("method", "proxy_burn_lp_token")
        .add_attribute("pool_factory", pool_factory)
        .add_attribute("lp_token", lp_token)
        .add_attribute("amount", amount.to_string())
        .add_message(burn_msg))
}

/// Authorised proxy entry used by `pool_factory` to return LP cw20 tokens
/// held by main factory back to the original sender after a failed
/// remove-liquidity ack. Issues a cw20 `Transfer` from main factory to the
/// recipient — matching the pre-refactor refund behaviour.
pub fn execute_proxy_transfer_lp_token(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    lp_token: Addr,
    recipient: String,
    amount: Uint256,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;
    let pool_factory = POOL_FACTORY_ADDRESS
        .may_load(deps.storage)?
        .ok_or(ContractError::Unauthorized {})?;
    ensure!(info.sender == pool_factory, ContractError::Unauthorized {});

    let recipient_addr = deps.api.addr_validate(&recipient)?;
    let transfer_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: lp_token.clone().into_string(),
        msg: to_json_binary(&euclid::msgs::lp_token::msg::ExecuteMsg::Transfer {
            recipient: recipient_addr.to_string(),
            amount,
        })?,
        funds: vec![],
    });

    Ok(Response::new()
        .add_attribute("method", "proxy_transfer_lp_token")
        .add_attribute("pool_factory", pool_factory)
        .add_attribute("lp_token", lp_token)
        .add_attribute("recipient", recipient_addr)
        .add_attribute("amount", amount.to_string())
        .add_message(transfer_msg))
}

/// Authorised proxy entry used by `pool_factory` to release escrowed tokens
/// to a recipient. Mirrors the existing `execute_release_escrow` IBC-receive
/// path: looks up the escrow contract for `token`, issues a `Withdraw` with
/// `reply_always(RELEASE_ESCROW_REPLY_ID)` so the existing reply plumbing
/// continues to capture withdraw outcomes.
pub fn execute_proxy_release_escrow(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    token: Token,
    denom: TokenType,
    recipient: String,
    amount: Uint256,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;
    let pool_factory = POOL_FACTORY_ADDRESS
        .may_load(deps.storage)?
        .ok_or(ContractError::Unauthorized {})?;
    ensure!(info.sender == pool_factory, ContractError::Unauthorized {});

    let recipient_addr = deps.api.addr_validate(&recipient)?;
    let escrow_address = TOKEN_TO_ESCROW.load(deps.storage, token.validate()?.to_owned())?;

    let withdraw_msg = EscrowExecuteMsg::Withdraw {
        recipient: recipient_addr.clone(),
        amount,
        denom,
        forwarding_message: None,
    };
    let sub = SubMsg::reply_always(
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: escrow_address.clone().into_string(),
            msg: to_json_binary(&withdraw_msg)?,
            funds: vec![],
        }),
        RELEASE_ESCROW_REPLY_ID,
    );

    Ok(Response::new()
        .add_attribute("method", "proxy_release_escrow")
        .add_attribute("pool_factory", pool_factory)
        .add_attribute("token", token.to_string())
        .add_attribute("recipient", recipient_addr)
        .add_attribute("amount", amount.to_string())
        .add_submessage(sub))
}

/// Authorised proxy entry used by `pool_factory` to mint a CLP position NFT
/// into the singleton position-token contract that main factory administers.
/// Slice 4 stands up the auth boundary so follow-up CLP slices (5+) can drive
/// minting from the pool_factory side without owning the NFT contract.
pub fn execute_proxy_mint_position(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    token_id: Uint128,
    owner: Addr,
    vlp_address: String,
    liquidity: Uint128,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;
    let pool_factory = POOL_FACTORY_ADDRESS
        .may_load(deps.storage)?
        .ok_or(ContractError::Unauthorized {})?;
    ensure!(info.sender == pool_factory, ContractError::Unauthorized {});

    let position_token_contract = POSITION_TOKEN_CONTRACT
        .may_load(deps.storage)?
        .ok_or_else(|| ContractError::new("Position token contract not registered"))?;

    let mint_msg = position_token::ExecuteMsg::Mint(PositionMintMsg {
        token_id,
        token_info: position_token::TokenInfo {
            owner: owner.clone(),
            token_uri: None,
        },
        position_info: position_token::PositionInfo {
            liquidity,
            vlp_address: vlp_address.clone(),
        },
    });
    let mint_call = mint_msg
        .to_msg(position_token_contract.clone())
        .map_err(ContractError::Std)?;

    Ok(Response::new()
        .add_attribute("method", "proxy_mint_position")
        .add_attribute("pool_factory", pool_factory)
        .add_attribute("position_token_contract", position_token_contract)
        .add_attribute("token_id", token_id.to_string())
        .add_attribute("owner", owner)
        .add_attribute("vlp", vlp_address)
        .add_attribute("liquidity", liquidity.to_string())
        .add_message(mint_call))
}

/// Helper used by the executor to build the WasmMsg that hands an ack to
/// pool_factory's `OnPoolAck`. Returned as a `SubMsg` so the caller can
/// attach it with `reply_never` semantics (errors here would already be
/// post-ack and observable on-chain).
pub fn pool_factory_on_pool_ack_submsg(
    deps: &DepsMut,
    original_msg: Binary,
    ack: Binary,
    is_native: bool,
) -> Result<SubMsg, ContractError> {
    let pool_factory = POOL_FACTORY_ADDRESS
        .may_load(deps.storage)?
        .ok_or(ContractError::new("Pool factory not initialised"))?;
    let exec = euclid::msgs::pool_factory::ExecuteMsg::OnPoolAck {
        original_msg,
        ack,
        is_native,
    };
    Ok(SubMsg::new(WasmMsg::Execute {
        contract_addr: pool_factory.into_string(),
        msg: to_json_binary(&exec)?,
        funds: vec![],
    }))
}

/// Returns true when pool_factory has been wired and is owning pool ops.
pub fn pool_factory_is_initialised(deps: &DepsMut) -> Result<bool, ContractError> {
    Ok(POOL_FACTORY_INITIALISED
        .may_load(deps.storage)?
        .unwrap_or(false))
}

/// Build a WasmMsg::Execute hand-off to pool_factory for a delegated handler.
pub fn pool_factory_execute_msg(
    deps: &DepsMut,
    exec: &euclid::msgs::pool_factory::ExecuteMsg,
) -> Result<cosmwasm_std::CosmosMsg, ContractError> {
    let pool_factory = POOL_FACTORY_ADDRESS
        .may_load(deps.storage)?
        .ok_or(ContractError::new("Pool factory not initialised"))?;
    Ok(cosmwasm_std::CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: pool_factory.into_string(),
        msg: to_json_binary(exec)?,
        funds: vec![],
    }))
}

/// Internal helper to expose the `SetPoolFactory` flow via the unified
/// ExecuteMsg surface — see `contract::execute` for dispatch.
pub fn handle_set_pool_factory(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    pool_factory_address: String,
) -> Result<Response, ContractError> {
    execute_set_pool_factory(deps, env, info, pool_factory_address)
}

/// Wrapper kept symmetric with the other `execute_*` exports.
pub fn handle_proxy_send_packet(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::ProxySendPacket {
            msg,
            timeout,
            ack_response,
            sender,
        } => execute_proxy_send_packet(deps, env, info, msg, timeout, ack_response, sender),
        _ => Err(ContractError::new("invalid proxy variant")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::{
        testing::{message_info, mock_dependencies, mock_env},
        to_json_binary, Addr,
    };
    use euclid::{
        cross_chain_user::CrossChainUser,
        msgs::vlp::base::PoolConfig,
        token::{Pair, PairWithDenomAndAmount, Token, TokenType, TokenWithDenomAndAmount},
    };
    use euclid_ibc::router_ibc::RouterCrossChainExecuteMsg;

    use crate::testing::helpers::{init, TEST_CHAIN_UID};

    fn cross_chain_user(addr: &str) -> CrossChainUser {
        CrossChainUser {
            chain_uid: euclid::chain::ChainUid::create(TEST_CHAIN_UID.to_string()).unwrap(),
            address: addr.to_string(),
        }
    }

    fn make_pool_creation_packet(sender_addr: &str) -> cosmwasm_std::Binary {
        let pair = PairWithDenomAndAmount {
            token_1: TokenWithDenomAndAmount {
                token: Token::create("aaa".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uaaa".to_string(),
                    decimals: None,
                },
                amount: cosmwasm_std::Uint256::from(100u128),
            },
            token_2: TokenWithDenomAndAmount {
                token: Token::create("bbb".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "ubbb".to_string(),
                    decimals: None,
                },
                amount: cosmwasm_std::Uint256::from(100u128),
            },
        };
        to_json_binary(&RouterCrossChainExecuteMsg::RequestPoolCreation {
            sender: cross_chain_user(sender_addr),
            tx_id: "tx_test".to_string(),
            pair,
            pool_config: PoolConfig::ConstantProduct {},
            slippage_tolerance_bps: 50,
        })
        .unwrap()
    }

    #[test]
    fn test_proxy_send_packet_unauthorised_caller_rejected() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        // Wire pool factory so the auth check has a configured address to
        // compare against, but call from a different sender.
        let pool_factory = deps.api.addr_make("pool_factory");
        POOL_FACTORY_ADDRESS
            .save(deps.as_mut().storage, &pool_factory)
            .unwrap();
        POOL_FACTORY_INITIALISED
            .save(deps.as_mut().storage, &true)
            .unwrap();

        let stranger = deps.api.addr_make("stranger");
        let user = deps.api.addr_make("user");
        let info = message_info(&stranger, &[]);
        let res = execute_proxy_send_packet(
            deps.as_mut(),
            mock_env(),
            info,
            make_pool_creation_packet(user.as_str()),
            None,
            None,
            user,
        );
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});
    }

    #[test]
    fn test_proxy_send_packet_with_no_pool_factory_set_unauthorised() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let caller = deps.api.addr_make("any_caller");
        let user = deps.api.addr_make("user");
        let info = message_info(&caller, &[]);
        let res = execute_proxy_send_packet(
            deps.as_mut(),
            mock_env(),
            info,
            make_pool_creation_packet(user.as_str()),
            None,
            None,
            user,
        );
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});
    }

    #[test]
    fn test_proxy_send_packet_authorised_caller_emits_submsg() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let pool_factory = deps.api.addr_make("pool_factory");
        POOL_FACTORY_ADDRESS
            .save(deps.as_mut().storage, &pool_factory)
            .unwrap();
        POOL_FACTORY_INITIALISED
            .save(deps.as_mut().storage, &true)
            .unwrap();

        let user = deps.api.addr_make("user");
        let info = message_info(&pool_factory, &[]);
        let res = execute_proxy_send_packet(
            deps.as_mut(),
            mock_env(),
            info,
            make_pool_creation_packet(user.as_str()),
            None,
            None,
            user,
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(res
            .attributes
            .iter()
            .any(|a| a.key == "method" && a.value == "proxy_send_packet"));
    }

    #[test]
    fn test_set_pool_factory_one_shot() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let admin = deps.api.addr_make("sender");
        let pool_factory = deps.api.addr_make("pool_factory");
        let info = message_info(&admin, &[]);
        execute_set_pool_factory(
            deps.as_mut(),
            mock_env(),
            info.clone(),
            pool_factory.to_string(),
        )
        .unwrap();
        // Second invocation must fail.
        let other = deps.api.addr_make("other_pool_factory");
        let err = execute_set_pool_factory(deps.as_mut(), mock_env(), info, other.to_string())
            .unwrap_err();
        assert!(matches!(err, ContractError::Generic { .. }));
        // Stored address matches the first call.
        let stored = POOL_FACTORY_ADDRESS.load(&deps.storage).unwrap();
        assert_eq!(stored, pool_factory);
    }

    #[test]
    fn test_set_pool_factory_non_migration_admin_rejected() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let non_admin = deps.api.addr_make("stranger");
        let pool_factory = deps.api.addr_make("pool_factory");
        let info = message_info(&non_admin, &[]);
        let res =
            execute_set_pool_factory(deps.as_mut(), mock_env(), info, pool_factory.to_string());
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});
    }

    fn _silence(_: Pair) {}

    // -----------------------------------------------------------------------
    // ProxyMintLpToken
    // -----------------------------------------------------------------------

    #[test]
    fn test_proxy_mint_lp_token_unauthorised_caller_rejected() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let pool_factory = deps.api.addr_make("pool_factory");
        POOL_FACTORY_ADDRESS
            .save(deps.as_mut().storage, &pool_factory)
            .unwrap();
        POOL_FACTORY_INITIALISED
            .save(deps.as_mut().storage, &true)
            .unwrap();

        let stranger = deps.api.addr_make("stranger");
        let lp_token = deps.api.addr_make("lp_token");
        let recipient = deps.api.addr_make("recipient");
        let info = message_info(&stranger, &[]);
        let res = execute_proxy_mint_lp_token(
            deps.as_mut(),
            mock_env(),
            info,
            lp_token,
            recipient.to_string(),
            cosmwasm_std::Uint256::from(100u128),
        );
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});
    }

    #[test]
    fn test_proxy_mint_lp_token_without_pool_factory_set_unauthorised() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let caller = deps.api.addr_make("any");
        let lp_token = deps.api.addr_make("lp_token");
        let recipient = deps.api.addr_make("recipient");
        let info = message_info(&caller, &[]);
        let res = execute_proxy_mint_lp_token(
            deps.as_mut(),
            mock_env(),
            info,
            lp_token,
            recipient.to_string(),
            cosmwasm_std::Uint256::from(100u128),
        );
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});
    }

    #[test]
    fn test_proxy_mint_lp_token_authorised_caller_emits_mint() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let pool_factory = deps.api.addr_make("pool_factory");
        POOL_FACTORY_ADDRESS
            .save(deps.as_mut().storage, &pool_factory)
            .unwrap();
        POOL_FACTORY_INITIALISED
            .save(deps.as_mut().storage, &true)
            .unwrap();

        let lp_token = deps.api.addr_make("lp_token");
        let recipient = deps.api.addr_make("recipient");
        let info = message_info(&pool_factory, &[]);
        let res = execute_proxy_mint_lp_token(
            deps.as_mut(),
            mock_env(),
            info,
            lp_token,
            recipient.to_string(),
            cosmwasm_std::Uint256::from(123u128),
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(res
            .attributes
            .iter()
            .any(|a| a.key == "method" && a.value == "proxy_mint_lp_token"));
    }

    // -----------------------------------------------------------------------
    // ProxyReleaseEscrow
    // -----------------------------------------------------------------------

    #[test]
    fn test_proxy_release_escrow_unauthorised_caller_rejected() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let pool_factory = deps.api.addr_make("pool_factory");
        POOL_FACTORY_ADDRESS
            .save(deps.as_mut().storage, &pool_factory)
            .unwrap();
        POOL_FACTORY_INITIALISED
            .save(deps.as_mut().storage, &true)
            .unwrap();

        let stranger = deps.api.addr_make("stranger");
        let recipient = deps.api.addr_make("recipient");
        let info = message_info(&stranger, &[]);
        let res = execute_proxy_release_escrow(
            deps.as_mut(),
            mock_env(),
            info,
            Token::create("eth".to_string()).unwrap(),
            TokenType::Native {
                denom: "ueth".to_string(),
                decimals: None,
            },
            recipient.to_string(),
            cosmwasm_std::Uint256::from(100u128),
        );
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});
    }

    // -----------------------------------------------------------------------
    // ProxyBurnLpToken
    // -----------------------------------------------------------------------

    #[test]
    fn test_proxy_burn_lp_token_unauthorised_caller_rejected() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let pool_factory = deps.api.addr_make("pool_factory");
        POOL_FACTORY_ADDRESS
            .save(deps.as_mut().storage, &pool_factory)
            .unwrap();
        POOL_FACTORY_INITIALISED
            .save(deps.as_mut().storage, &true)
            .unwrap();

        let stranger = deps.api.addr_make("stranger");
        let lp_token = deps.api.addr_make("lp_token");
        let info = message_info(&stranger, &[]);
        let res = execute_proxy_burn_lp_token(
            deps.as_mut(),
            mock_env(),
            info,
            lp_token,
            cosmwasm_std::Uint256::from(100u128),
        );
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});
    }

    #[test]
    fn test_proxy_burn_lp_token_without_pool_factory_set_unauthorised() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let caller = deps.api.addr_make("any");
        let lp_token = deps.api.addr_make("lp_token");
        let info = message_info(&caller, &[]);
        let res = execute_proxy_burn_lp_token(
            deps.as_mut(),
            mock_env(),
            info,
            lp_token,
            cosmwasm_std::Uint256::from(100u128),
        );
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});
    }

    #[test]
    fn test_proxy_burn_lp_token_authorised_caller_emits_burn() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let pool_factory = deps.api.addr_make("pool_factory");
        POOL_FACTORY_ADDRESS
            .save(deps.as_mut().storage, &pool_factory)
            .unwrap();
        POOL_FACTORY_INITIALISED
            .save(deps.as_mut().storage, &true)
            .unwrap();

        let lp_token = deps.api.addr_make("lp_token");
        let info = message_info(&pool_factory, &[]);
        let res = execute_proxy_burn_lp_token(
            deps.as_mut(),
            mock_env(),
            info,
            lp_token,
            cosmwasm_std::Uint256::from(77u128),
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(res
            .attributes
            .iter()
            .any(|a| a.key == "method" && a.value == "proxy_burn_lp_token"));
    }

    // -----------------------------------------------------------------------
    // ProxyTransferLpToken
    // -----------------------------------------------------------------------

    #[test]
    fn test_proxy_transfer_lp_token_unauthorised_caller_rejected() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let pool_factory = deps.api.addr_make("pool_factory");
        POOL_FACTORY_ADDRESS
            .save(deps.as_mut().storage, &pool_factory)
            .unwrap();
        POOL_FACTORY_INITIALISED
            .save(deps.as_mut().storage, &true)
            .unwrap();

        let stranger = deps.api.addr_make("stranger");
        let lp_token = deps.api.addr_make("lp_token");
        let recipient = deps.api.addr_make("recipient");
        let info = message_info(&stranger, &[]);
        let res = execute_proxy_transfer_lp_token(
            deps.as_mut(),
            mock_env(),
            info,
            lp_token,
            recipient.to_string(),
            cosmwasm_std::Uint256::from(100u128),
        );
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});
    }

    #[test]
    fn test_proxy_transfer_lp_token_authorised_caller_emits_transfer() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let pool_factory = deps.api.addr_make("pool_factory");
        POOL_FACTORY_ADDRESS
            .save(deps.as_mut().storage, &pool_factory)
            .unwrap();
        POOL_FACTORY_INITIALISED
            .save(deps.as_mut().storage, &true)
            .unwrap();

        let lp_token = deps.api.addr_make("lp_token");
        let recipient = deps.api.addr_make("recipient");
        let info = message_info(&pool_factory, &[]);
        let res = execute_proxy_transfer_lp_token(
            deps.as_mut(),
            mock_env(),
            info,
            lp_token,
            recipient.to_string(),
            cosmwasm_std::Uint256::from(77u128),
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(res
            .attributes
            .iter()
            .any(|a| a.key == "method" && a.value == "proxy_transfer_lp_token"));
    }

    // -----------------------------------------------------------------------
    // ProxyMintPosition
    // -----------------------------------------------------------------------

    fn seed_position_token_contract(deps: &mut crate::testing::helpers::MockDeps, addr: &Addr) {
        POSITION_TOKEN_CONTRACT
            .save(deps.as_mut().storage, addr)
            .unwrap();
    }

    #[test]
    fn test_proxy_mint_position_unauthorised_caller_rejected() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let pool_factory = deps.api.addr_make("pool_factory");
        POOL_FACTORY_ADDRESS
            .save(deps.as_mut().storage, &pool_factory)
            .unwrap();
        POOL_FACTORY_INITIALISED
            .save(deps.as_mut().storage, &true)
            .unwrap();
        let position_token = deps.api.addr_make("position_token");
        seed_position_token_contract(&mut deps, &position_token);

        let stranger = deps.api.addr_make("stranger");
        let owner = deps.api.addr_make("owner");
        let info = message_info(&stranger, &[]);
        let res = execute_proxy_mint_position(
            deps.as_mut(),
            mock_env(),
            info,
            cosmwasm_std::Uint128::new(1),
            owner,
            "vlp_clp".to_string(),
            cosmwasm_std::Uint128::new(100),
        );
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});
    }

    #[test]
    fn test_proxy_mint_position_without_pool_factory_set_unauthorised() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let caller = deps.api.addr_make("any");
        let owner = deps.api.addr_make("owner");
        let info = message_info(&caller, &[]);
        let res = execute_proxy_mint_position(
            deps.as_mut(),
            mock_env(),
            info,
            cosmwasm_std::Uint128::new(1),
            owner,
            "vlp_clp".to_string(),
            cosmwasm_std::Uint128::new(100),
        );
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});
    }

    #[test]
    fn test_proxy_mint_position_without_position_token_set_errors() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let pool_factory = deps.api.addr_make("pool_factory");
        POOL_FACTORY_ADDRESS
            .save(deps.as_mut().storage, &pool_factory)
            .unwrap();
        POOL_FACTORY_INITIALISED
            .save(deps.as_mut().storage, &true)
            .unwrap();

        let owner = deps.api.addr_make("owner");
        let info = message_info(&pool_factory, &[]);
        let res = execute_proxy_mint_position(
            deps.as_mut(),
            mock_env(),
            info,
            cosmwasm_std::Uint128::new(1),
            owner,
            "vlp_clp".to_string(),
            cosmwasm_std::Uint128::new(100),
        );
        assert!(res.is_err());
    }

    #[test]
    fn test_proxy_mint_position_authorised_caller_emits_mint() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let pool_factory = deps.api.addr_make("pool_factory");
        POOL_FACTORY_ADDRESS
            .save(deps.as_mut().storage, &pool_factory)
            .unwrap();
        POOL_FACTORY_INITIALISED
            .save(deps.as_mut().storage, &true)
            .unwrap();
        let position_token = deps.api.addr_make("position_token");
        seed_position_token_contract(&mut deps, &position_token);

        let owner = deps.api.addr_make("owner");
        let info = message_info(&pool_factory, &[]);
        let res = execute_proxy_mint_position(
            deps.as_mut(),
            mock_env(),
            info,
            cosmwasm_std::Uint128::new(7),
            owner,
            "vlp_clp".to_string(),
            cosmwasm_std::Uint128::new(1_000),
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(res
            .attributes
            .iter()
            .any(|a| a.key == "method" && a.value == "proxy_mint_position"));
    }

    #[test]
    fn test_proxy_release_escrow_authorised_caller_emits_withdraw_submsg() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let pool_factory = deps.api.addr_make("pool_factory");
        POOL_FACTORY_ADDRESS
            .save(deps.as_mut().storage, &pool_factory)
            .unwrap();
        POOL_FACTORY_INITIALISED
            .save(deps.as_mut().storage, &true)
            .unwrap();

        // Seed an escrow for the token so the load succeeds.
        crate::testing::helpers::seed_escrow(&mut deps, "eth", "escrow_eth");

        let recipient = deps.api.addr_make("recipient");
        let info = message_info(&pool_factory, &[]);
        let res = execute_proxy_release_escrow(
            deps.as_mut(),
            mock_env(),
            info,
            Token::create("eth".to_string()).unwrap(),
            TokenType::Native {
                denom: "ueth".to_string(),
                decimals: None,
            },
            recipient.to_string(),
            cosmwasm_std::Uint256::from(100u128),
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_eq!(res.messages[0].id, crate::reply::RELEASE_ESCROW_REPLY_ID);
        assert!(res
            .attributes
            .iter()
            .any(|a| a.key == "method" && a.value == "proxy_release_escrow"));
    }
}
