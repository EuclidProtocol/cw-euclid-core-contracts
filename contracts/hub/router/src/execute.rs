use cosmwasm_std::{
    ensure, to_json_binary, CosmosMsg, DepsMut, Env, IbcMsg, IbcTimeout, MessageInfo, Response,
};
use euclid::{error::ContractError, timeout::get_timeout};
use euclid_ibc::msg::{ChainIbcExecuteMsg, HubIbcExecuteMsg};

use crate::state::STATE;

// Function to update the pool code ID
pub fn execute_update_vlp_code_id(
    deps: DepsMut,
    info: MessageInfo,
    new_vlp_code_id: u64,
) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;

    ensure!(info.sender == state.admin, ContractError::Unauthorized {});

    state.vlp_code_id = new_vlp_code_id;

    STATE.save(deps.storage, &state)?;

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

    let timeout = get_timeout(timeout)?;

    let packet = IbcMsg::SendPacket {
        channel_id: channel.clone(),
        data: to_json_binary(&msg)?,
        timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(timeout)),
    };

    Ok(Response::new()
        .add_attribute("method", "register_factory")
        .add_attribute("channel", channel)
        .add_attribute("timeout", timeout.to_string())
        .add_message(CosmosMsg::Ibc(packet)))
}

pub fn execute_internal_msg(
    deps: DepsMut,
    env: Env,
    msg: ChainIbcExecuteMsg,
) -> Result<Response, ContractError> {
    Ok(Response::new())
}
