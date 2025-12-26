#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{attr, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult};
use cw2::set_contract_version;

use crate::error::ContractError;
use crate::execute;
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::query;
use crate::state::{OrderbookDepositsStatus, RootConfig, State, ROOT_CONFIG, STATE};

const CONTRACT_NAME: &str = "crates.io:orderbook_deposits";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let admin = msg
        .admin
        .map(|addr| deps.api.addr_validate(&addr))
        .transpose()?
        .unwrap_or_else(|| info.sender.clone());
    let virtual_balance = deps.api.addr_validate(&msg.virtual_balance)?;

    let state = State {
        admin: admin.clone(),
        status: OrderbookDepositsStatus::Active,
        virtual_balance: virtual_balance.clone(),
    };
    STATE.save(deps.storage, &state)?;

    let authorized_posters = match msg.authorized_posters {
        Some(posters) => posters
            .into_iter()
            .map(|poster| deps.api.addr_validate(&poster))
            .collect::<Result<Vec<_>, _>>()?,
        None => vec![admin.clone()],
    };

    let root_config = RootConfig {
        permit_signer_pubkey: msg.permit_signer_pubkey,
        permit_signer_address: msg.permit_signer_address,
        root_challenge_period: msg.root_challenge_period.unwrap_or(0),
        authorized_posters,
    };
    ROOT_CONFIG.save(deps.storage, &root_config)?;
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "instantiate"),
        attr("admin", admin.as_str()),
        attr("virtual_balance", virtual_balance.as_str()),
    ]))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    execute::execute(deps, env, info, msg)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(_deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    query::query(_deps, msg)
}
