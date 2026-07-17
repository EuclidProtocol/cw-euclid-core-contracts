#![cfg(not(target_arch = "wasm32"))]

//! Test-only stub that mimics the pool factory's `OnRequestPoolCreation`
//! ExecuteMsg surface but always returns `Response::data` carrying a
//! `PoolFactoryReply::SendPacket` whose inner binary is a non-pool
//! `RouterReceiveMsg` variant. Used to exercise the defence-in-depth
//! pool-variant check on main factory's `on_pool_factory_delegate_reply`.

use cosmwasm_std::{
    to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult,
};
use cw_orch::{interface, prelude::*};
use euclid::{
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    error::ContractError,
    msgs::pool_factory::{ExecuteMsg, InstantiateMsg, MigrateMsg, PoolFactoryReply, QueryMsg},
    token::{Token, TokenType, TokenWithDenom},
};
use euclid_ibc::wire::envelope::router::RouterReceiveMsg;
use euclid_ibc::wire::msgs::RegisterDenomSendMsg;

pub fn instantiate(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    Ok(Response::default())
}

pub fn execute(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    let ExecuteMsg::OnRequestPoolCreation { tx_id, sender, .. } = msg else {
        return Ok(Response::default());
    };

    // Pack a `RouterReceiveMsg::RegisterDenom` (non-pool variant)
    // into a `PoolFactoryReply::SendPacket`. Main factory's reply handler MUST
    // reject this packet before dispatching anything.
    let evil_packet = RouterReceiveMsg::RegisterDenom(RegisterDenomSendMsg {
        sender: CrossChainUser::new(
            ChainUid::create("evil".to_string()).unwrap(),
            sender.to_string(),
        ),
        tx_id: tx_id.clone(),
        token: TokenWithDenom {
            token: Token::create("evil".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "uevil".to_string(),
                decimals: None,
            },
        },
    });
    let payload = PoolFactoryReply::SendPacket {
        msg: to_json_binary(&evil_packet)?,
        timeout: None,
        ack_response: None,
        sender,
    };

    Ok(Response::new()
        .add_attribute("method", "malicious_on_request_pool_creation")
        .add_attribute("tx_id", tx_id)
        .set_data(to_json_binary(&payload)?))
}

pub fn query(_deps: Deps, _env: Env, _msg: QueryMsg) -> StdResult<Binary> {
    Err(StdError::generic_err("malicious stub has no queries"))
}

pub const CONTRACT_ID: &str = "malicious_pool_factory_stub";

#[interface(InstantiateMsg, ExecuteMsg, QueryMsg, MigrateMsg, id = CONTRACT_ID)]
pub struct MaliciousPoolFactoryContract<Chain: CwEnv>;

impl<Chain> Uploadable for MaliciousPoolFactoryContract<Chain> {
    fn wrapper() -> Box<dyn MockContract<Empty>> {
        Box::new(ContractWrapper::new_with_empty(execute, instantiate, query))
    }
}
