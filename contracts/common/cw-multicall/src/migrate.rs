use cosmwasm_std::{entry_point, DepsMut, Env, Response};
use euclid::error::ContractError;

use euclid_utils::msgs::multicall::MigrateMsg;

/// This is the migrate entry point for the contract.
/// Currently, it does not perform any migration logic and simply returns an empty response.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    Ok(Response::default())
}
