#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{DepsMut, Env, Response};
use cw2::set_contract_version;
use euclid::{error::ContractError, msgs::vlp::concentrated::msg::MigrateMsg};

const CONTRACT_NAME: &str = "crates.io:concentrated_vlp";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    euclid::build_info::set_build_info(deps.storage)?;
    Ok(Response::new()
        .add_attribute("action", "migrate_concentrated_vlp")
        .add_attribute("version", CONTRACT_VERSION))
}
