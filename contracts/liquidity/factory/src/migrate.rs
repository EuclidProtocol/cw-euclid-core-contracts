use cosmwasm_std::{entry_point, DepsMut, Env, Response};
use euclid::{error::ContractError, msgs::factory::MigrateMsg};

use crate::state::MOCK_RELAYER_ADDRESS;

/// This is the migrate entry point for the contract.
/// Currently, it does not perform any migration logic and simply returns an empty response.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, msg: MigrateMsg) -> Result<Response, ContractError> {
    if let Some(mock_relayer_address) = msg.mock_relayer_address {
        MOCK_RELAYER_ADDRESS.save(deps.storage, &mock_relayer_address)?;
    }
    Ok(Response::default())
}
