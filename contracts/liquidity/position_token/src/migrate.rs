use cosmwasm_std::{entry_point, DepsMut, Env, Response};
use euclid::error::ContractError;

use crate::msg::MigrateMsg;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    Ok(Response::default())
}
