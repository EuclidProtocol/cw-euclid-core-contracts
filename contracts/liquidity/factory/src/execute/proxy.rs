use cosmwasm_std::{
    ensure, from_json, to_json_binary, Addr, Binary, DepsMut, Env, MessageInfo, Response, SubMsg,
    WasmMsg,
};
use euclid::{chain::ChainType, error::ContractError, msgs::factory::ExecuteMsg};
use euclid_ibc::router_ibc::RouterCrossChainExecuteMsg;

use crate::{
    query::get_chain_type,
    state::{ADMIN, POOL_FACTORY_ADDRESS, POOL_FACTORY_INITIALISED, STATE},
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
}
