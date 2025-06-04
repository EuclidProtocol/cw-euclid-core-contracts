#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response};
use cw2::set_contract_version;

use crate::execute::{migrate_vbalance, migrate_vlp, update_state};
use crate::query::query_state;
use crate::reply;
use crate::reply::{NEXT_SWAP_REPLY_ID, VIRTUAL_BALANCE_TRANSFER_REPLY_ID};
use crate::state::{State, STATE};
use euclid::error::ContractError;
use euclid::msgs::migrator::{ExecuteMsg, InstantiateMsg, QueryMsg};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:migration";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let state = State {
        virtual_balance: msg.virtual_balance,
        router: info.sender.to_string(),
        vlp: msg.vlp,
        admin: msg.admin,
    };

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    STATE.save(deps.storage, &state)?;

    Ok(Response::default()
        .add_attribute("method", "instantiate")
        .add_attribute("vlp_address", env.contract.address.to_string())
        .add_attribute("owner", info.sender))
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
            router,
            virtual_balance,
            vlp,
            admin,
        } => update_state(deps, info, router, virtual_balance, vlp, admin),
        ExecuteMsg::MigrateVBalance {
            vbalance_address,
            router_address,
            channel_id,
            timeout,
        } => migrate_vbalance(
            deps,
            env,
            info,
            vbalance_address,
            router_address,
            channel_id,
            timeout,
        ),
        ExecuteMsg::MigrateVLP {
            vlp_address,
            router_address,
            vbalance_address,
            channel_id,
            timeout,
        } => migrate_vlp(
            deps,
            env,
            info,
            vbalance_address,
            router_address,
            vlp_address,
            channel_id,
            timeout,
        ),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::State {} => query_state(deps),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        VIRTUAL_BALANCE_TRANSFER_REPLY_ID => reply::on_virtual_balance_transfer_reply(deps, msg),
        NEXT_SWAP_REPLY_ID => reply::on_next_swap_reply(deps, msg),

        id => Err(ContractError::Generic {
            err: format!("Unknown reply id: {id}"),
        }),
    }
}
