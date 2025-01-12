use cosmwasm_std::{ensure, Addr, DepsMut, Env, MessageInfo, Response};
use euclid::{error::ContractError, token::Pair};

use crate::state::{State, STATE};

pub fn execute_update_state(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    token_pair: Option<Pair>,
    factory_address: Option<Addr>,
    vlp: Option<String>,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(
        state.factory_address == info.sender.into_string(),
        ContractError::Unauthorized {}
    );

    STATE.save(
        deps.storage,
        &State {
            token_pair: token_pair.unwrap_or(state.token_pair),
            factory_address: factory_address.clone().unwrap_or(state.factory_address),
            vlp: vlp.clone().unwrap_or(state.vlp),
        },
    )?;

    Ok(Response::new()
        .add_attribute("method", "update_state")
        .add_attribute(
            "factory_address",
            factory_address.map_or("unchanged".to_string(), |x| x.to_string()),
        )
        .add_attribute(
            "vlp",
            vlp.map_or("unchanged".to_string(), |x| x.to_string()),
        ))
}
