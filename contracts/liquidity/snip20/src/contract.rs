#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response};
use euclid::msgs::escrow::Snip20InstantiateResponse;
use secret_cw2::set_contract_version;

use crate::execute::execute_update_state;
use crate::state::{State, STATE};
use euclid::error::ContractError;
use euclid::msgs::snip20::{ExecuteMsg, InstantiateMsg, QueryMsg};

use snip20_reference_impl::contract::{
    execute as execute_snip20, instantiate as snip20_instantiate, query as snip20_query,
};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:cw20";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let snip20_resp = snip20_instantiate(deps.branch(), env.clone(), info, msg.clone().into())?;
    let state = State {
        token_pair: msg.token_pair.clone(),
        factory_address: msg.factory,
        vlp: msg.vlp.clone(),
    };
    STATE.save(deps.storage, &state)?;

    let data = Snip20InstantiateResponse {
        pair: msg.token_pair,
        address: env.contract.address.into_string(),
        code_hash: env.contract.code_hash,
        vlp: msg.vlp,
    };

    Ok(snip20_resp.set_data(to_binary(&data)?))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::UpdateState {
            token_pair,
            factory_address,
            vlp,
        } => execute_update_state(deps, env, info, token_pair, factory_address, vlp),
        _ => {
            let msg_to_send:snip20_reference_impl::msg::ExecuteMsg = msg.into();
            Ok(execute_snip20(deps, env, info, msg_to_send)?)
        },
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    Ok(snip20_query(deps, env, msg.into())?)
}
