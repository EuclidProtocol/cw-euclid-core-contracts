use cosmwasm_std::{ensure, DepsMut, Env, MessageInfo, Response};
use euclid::{
    cross_chain_user::CrossChainUser, error::ContractError,
    msgs::cross_chain_config::CrossChainConfig, utils::tx::generate_tx,
};
use euclid_ibc::router_ibc::RouterCrossChainExecuteMsg;

use crate::{
    query::get_chain_type,
    state::{PingInfo, LATEST_PING, STATE},
};

pub fn execute_ping_router(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    cross_chain_config: CrossChainConfig,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(
        state.admin == info.sender.to_string(),
        ContractError::Unauthorized {}
    );

    let sender = CrossChainUser::new(state.chain_uid.clone(), info.sender.to_string());
    let tx_id = generate_tx(deps, &env, &sender)?;
    let chain_type = get_chain_type(deps.as_ref(), &env)?;
    LATEST_PING.save(
        deps.storage,
        &PingInfo {
            tx_id: tx_id.clone(),
            block_height: env.block.height,
            timestamp: env.block.time.seconds(),
        },
    )?;

    let ping_msg = RouterCrossChainExecuteMsg::Ping {
        tx_id: tx_id.clone(),
        block_height: env.block.height,
        timestamp: env.block.time.seconds(),
    }
    .to_msg(
        deps,
        &env,
        state.router_contract,
        info.sender,
        state.chain_uid,
        chain_type,
        cross_chain_config.timeout,
        cross_chain_config.ack_response,
    )?;

    Ok(Response::new()
        .add_attribute("method", "ping_router")
        .add_attribute("tx_id", tx_id)
        .add_attribute("factory_block_height", env.block.height.to_string())
        .add_attribute("factory_timestamp", env.block.time.seconds().to_string())
        .add_submessage(ping_msg))
}
