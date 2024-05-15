use cosmwasm_std::{entry_point, from_json, to_json_binary, DepsMut, Env, Reply, Response, StdError, StdResult, SubMsgResult};
use cw0::parse_reply_instantiate_data; 
use euclid::error::ContractError;
use euclid_ibc::msg::AcknowledgementMsg;
use crate::state::STATE; 
use crate::msg::PoolInstantiateMsg; 

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
pub fn on_pool_instantiate_reply(deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    let data = match msg.result.clone() {
        SubMsgResult::Err(err) => AcknowledgementMsg::Error(err),
        SubMsgResult::Ok(..) => {
            let instantiate_data = parse_reply_instantiate_data(msg).unwrap();
            let pool_msg: PoolInstantiateMsg = from_json(instantiate_data.data.unwrap()).unwrap();

            // Update the router contract address in the state
            STATE.update::<_, ContractError>(deps.storage, |mut state| {
                state.router_contract = pool_msg.vlp_contract.clone(); // Update the router contract address
                Ok(state)
            })?;

            AcknowledgementMsg::Ok("Pool instantiated successfully".to_string())
        }
    };
    
    Ok(Response::default().set_data(to_json_binary(&data)?))
}