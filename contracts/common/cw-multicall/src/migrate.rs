use cosmwasm_std::{entry_point, DepsMut, Env, Response};
use cw2::set_contract_version;
use euclid::error::ContractError;
use euclid_utils::msgs::multicall::MigrateMsg;

use crate::contract::{CONTRACT_NAME, CONTRACT_VERSION};

/// This is the migrate entry point for the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::default())
}
