use cosmwasm_std::{entry_point, DepsMut, Env, Response};
use cw2::set_contract_version;
use euclid::{error::ContractError, msgs::escrow::MigrateMsg};

use crate::contract::{CONTRACT_NAME, CONTRACT_VERSION};

// No data migration needed for voucher normalization:
// - State.total_amount: Uint128→Uint256 backward compatible (same JSON string format)
// - DENOM_TO_AMOUNT: Uint128→Uint256 backward compatible
// - ALLOWED_DENOMS Vec<TokenType>: new decimals: Option<u32> field defaults to None
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::default())
}
