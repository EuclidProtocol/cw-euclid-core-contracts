use cosmwasm_std::{ensure, Addr, DepsMut, Env, MessageInfo, Response};
use euclid::{error::ContractError, token::Pair};

use crate::state::{State, STATE};

pub fn execute_update_state(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    token_pair: Pair,
    factory_address: Addr,
    vlp: String,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(
        state.factory_address == info.sender.into_string(),
        ContractError::Unauthorized {}
    );

    STATE.save(
        deps.storage,
        &State {
            token_pair,
            factory_address: factory_address.clone(),
            vlp: vlp.clone(),
        },
    )?;

    Ok(Response::new()
        .add_attribute("method", "update_state")
        .add_attribute("factory_address", factory_address.to_string())
        .add_attribute("vlp", vlp))
}
