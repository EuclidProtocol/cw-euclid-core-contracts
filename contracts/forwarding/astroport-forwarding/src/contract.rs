use std::borrow::BorrowMut;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{ensure, Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, StdError};

use cw2::set_contract_version;
use forwarding::msgs::common_old::TokenType;
use forwarding::msgs::errors_old::ContractError;

use crate::{
    execute::{execute_cw20_receive, receive_euclid_native, swap},
    reply::{on_astro_swap_reply, ASTRO_SWAP_REPLY_ID},
    state::{State, STATE},
};

use forwarding::msgs::astroport::{ExecuteMsg, InstantiateMsg, QueryMsg};

// version info for migration info
pub(crate) const CONTRACT_NAME: &str = "crates.io:astroport-forwarding";
pub(crate) const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let state = State {
        astro_router_address: msg.astro_router.clone(),
    };
    STATE.save(deps.storage, &state)?;
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("astro_router", msg.astro_router))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::EuclidReceive(msg) => {
            receive_euclid_native(deps.borrow_mut(), &env, &info, msg)
        }
        ExecuteMsg::Receive(msg) => execute_cw20_receive(deps.borrow_mut(), &env, &info, msg),
        ExecuteMsg::Swap(swap_msg) => {
            ensure!(
                info.funds.len() == 1,
                ContractError::new("only one token is supported")
            );
            let from_token = TokenType::Native {
                denom: info.funds[0].denom.to_string(),
            };
            let from_amount = info.funds[0].amount.into();

            swap(deps.borrow_mut(), &env, swap_msg, from_token, from_amount)
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(_deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {}
}
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        ASTRO_SWAP_REPLY_ID => on_astro_swap_reply(deps, env, msg),
        id => Err(ContractError::Std(StdError::generic_err(format!(
            "Unknown reply id: {}",
            id
        )))),
    }
}
