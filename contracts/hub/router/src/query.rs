use cosmwasm_std::{to_json_binary, Binary, Deps};
use euclid::{error::ContractError, msgs::router::StateResponse};

use crate::state::STATE;

pub fn query_state(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&StateResponse {
        admin: state.admin,
        vlp_code_id: state.vlp_code_id,
    })?)
}
