use cosmwasm_std::{DepsMut, Env, Response};
use cw2::set_contract_version;
use euclid::{error::ContractError, msgs::pool_factory::MigrateMsg};

use crate::contract::{CONTRACT_NAME, CONTRACT_VERSION};

#[cfg_attr(not(feature = "library"), cosmwasm_std::entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    euclid::build_info::set_build_info(deps.storage)?;
    Ok(Response::new().add_attribute("method", "migrate"))
}
