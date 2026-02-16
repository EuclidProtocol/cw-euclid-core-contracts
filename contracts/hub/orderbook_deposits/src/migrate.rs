use cosmwasm_std::{entry_point, DepsMut, Env, Response};
use cw2::set_contract_version;

use crate::error::ContractError;
use euclid::msgs::orderbook_deposits::MigrateMsg;

const CONTRACT_NAME: &str = "crates.io:orderbook_deposits";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Migrate entry point. This can only be called by the chain contract admin.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::default())
}
