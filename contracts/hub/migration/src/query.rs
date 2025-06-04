use cosmwasm_std::{to_json_binary, Binary, Deps};

use euclid::error::ContractError;
use euclid::msgs::migrator::GetStateResponse;

use crate::state::STATE;

pub fn query_state(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&GetStateResponse {
        router: state.router,
        virtual_balance: state.virtual_balance,
        admin: state.admin,
    })?)
}
