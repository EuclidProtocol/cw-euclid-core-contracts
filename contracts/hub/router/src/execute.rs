use cosmwasm_std::{
    ensure, to_json_binary, CosmosMsg, DepsMut, Env, IbcMsg, IbcTimeout, MessageInfo, Response,
};
use euclid::error::ContractError;
use euclid_ibc::msg::HubIbcExecuteMsg;

use crate::state::STATE;

// Function to update the pool code ID
pub fn execute_update_vlp_code_id(
    deps: DepsMut,
    info: MessageInfo,
    new_vlp_code_id: u64,
) -> Result<Response, ContractError> {
    // Load the state
    STATE.update(deps.storage, |mut state| -> Result<_, ContractError> {
        // Ensure that only the admin can update the pool code ID
        if info.sender != state.admin {
            return Err(ContractError::Unauthorized {});
        }

        // Update the pool code ID
        state.vlp_code_id = new_vlp_code_id;
        Ok(state)
    })?;

    Ok(Response::new()
        .add_attribute("method", "update_pool_code_id")
        .add_attribute("new_vlp_code_id", new_vlp_code_id.to_string()))
}

// Function to update the pool code ID
pub fn execute_register_factory(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    channel: String,
    timeout: Option<u64>,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.admin, ContractError::Unauthorized {});

    let msg = HubIbcExecuteMsg::RegisterFactory {
        router: env.contract.address.to_string(),
    };

    let packet = IbcMsg::SendPacket {
        channel_id: channel.clone(),
        data: to_json_binary(&msg)?,
        // TODO: Add Joe min max timestamp logic here!
        timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(timeout.unwrap_or(60))),
    };

    Ok(Response::new()
        .add_attribute("method", "register_factory")
        .add_attribute("channel", channel)
        .add_message(CosmosMsg::Ibc(packet)))
}
