use cosmwasm_std::{entry_point, from_json, DepsMut, Env, Reply, Response, StdError, StdResult};

pub const INSTANTIATE_REPLY_ID: u64 = 1u64;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> StdResult<Response> {
    match msg.id {
        INSTANTIATE_REPLY_ID => handle_instantiate_reply(deps, msg),
        id => Err(StdError::generic_err(format!("Unknown reply id: {}", id))),
    }
}

fn handle_instantiate_reply(deps: DepsMut, msg: Reply) -> StdResult<Response> {
    Ok(Response::new())
} 