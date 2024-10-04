#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    ensure, to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, StdError,
    SubMsg, WasmMsg,
};
use cw2::set_contract_version;
use euclid::error::ContractError;
use euclid_ibc::msg::HUB_IBC_EXECUTE_MSG_QUEUE_RANGE;

use crate::execute::{
    execute_deregister_chain, execute_native_receive_callback, execute_register_factory,
    execute_release_escrow, execute_reregister_chain, execute_transfer_voucher_internal,
    execute_update_factory_channel, execute_update_lock, execute_update_router_state,
    execute_withdraw_voucher,
};
use crate::ibc::ack_and_timeout::ibc_ack_packet_internal_call;
use crate::ibc::receive::ibc_receive_internal_call;
use crate::query::{
    self, query_all_chains, query_all_escrows, query_all_tokens, query_all_vlps, query_chain,
    query_simulate_escrow_release, query_state, query_token_escrows, query_vlp,
};
use crate::reply::{
    self, ADD_LIQUIDITY_REPLY_ID, IBC_ACK_AND_TIMEOUT_REPLY_ID, IBC_RECEIVE_REPLY_ID,
    REMOVE_LIQUIDITY_REPLY_ID, SWAP_REPLY_ID, VIRTUAL_BALANCE_INSTANTIATE_REPLY_ID,
    VLP_INSTANTIATE_REPLY_ID, VLP_POOL_REGISTER_REPLY_ID,
};
use crate::state::{State, DEREGISTERED_CHAINS, STATE};
use euclid::msgs::router::{ExecuteMsg, InstantiateMsg, QueryMsg};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:factory";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let state = State {
        vlp_code_id: msg.vlp_code_id,
        admin: info.sender.to_string(),
        virtual_balance_address: None,
        locked: false,
    };
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    STATE.save(deps.storage, &state)?;

    let virtual_balance_instantiate_msg = euclid::msgs::virtual_balance::InstantiateMsg {
        router: env.contract.address.clone(),
        admin: Some(info.sender.clone()),
    };
    let virtual_balance_instantiate_msg = WasmMsg::Instantiate {
        admin: Some(info.sender.to_string()),
        code_id: msg.virtual_balance_code_id,
        msg: to_json_binary(&virtual_balance_instantiate_msg)?,
        funds: vec![],
        label: "Instantiate Virtual Balance Contract".to_string(),
    };

    let virtual_balance_instantiate_msg = SubMsg::reply_always(
        virtual_balance_instantiate_msg,
        VIRTUAL_BALANCE_INSTANTIATE_REPLY_ID,
    );

    DEREGISTERED_CHAINS.save(deps.storage, &vec![])?;

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("router_contract", env.contract.address)
        .add_submessage(virtual_balance_instantiate_msg))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    // If the contract is locked and the message isn't UpdateLock, return error

    match msg {
        ExecuteMsg::UpdateLock {} => execute_update_lock(deps, info),
        ExecuteMsg::ReregisterChain { chain } => execute_reregister_chain(deps, info, chain),
        ExecuteMsg::DeregisterChain { chain } => execute_deregister_chain(deps, info, chain),
        _ => {
            ensure!(
                !STATE.load(deps.storage)?.locked,
                ContractError::ContractLocked {}
            );
            match msg {
                ExecuteMsg::UpdateFactoryChannel { channel, chain_uid } => {
                    execute_update_factory_channel(&mut deps, env, info, channel, chain_uid)
                }
                ExecuteMsg::RegisterFactory {
                    chain_uid,
                    chain_info,
                } => execute_register_factory(&mut deps, env, info, chain_uid, chain_info),
                ExecuteMsg::ReleaseEscrowInternal {
                    sender,
                    token,
                    amount,
                    cross_chain_addresses,
                    timeout,
                    tx_id,
                } => execute_release_escrow(
                    &mut deps,
                    env,
                    info,
                    sender,
                    token,
                    amount,
                    cross_chain_addresses,
                    timeout,
                    tx_id,
                ),
                ExecuteMsg::WithdrawVoucher {
                    token,
                    amount,
                    cross_chain_addresses,
                    timeout,
                } => execute_withdraw_voucher(
                    &mut deps,
                    env,
                    info,
                    token,
                    amount,
                    cross_chain_addresses,
                    timeout,
                ),
                ExecuteMsg::TransferVirtualBalance {
                    token,
                    recipient,
                    amount,
                    cross_chain_addresses,
                    timeout,
                } => execute_transfer_voucher_internal(
                    &mut deps,
                    env,
                    info,
                    token,
                    recipient,
                    amount,
                    cross_chain_addresses,
                    timeout,
                ),
                ExecuteMsg::IbcCallbackReceive { receive_msg } => {
                    ibc_receive_internal_call(&mut deps, env, info, receive_msg)
                }
                ExecuteMsg::IbcCallbackAckAndTimeout { ack } => {
                    ibc_ack_packet_internal_call(deps, env, ack)
                }
                ExecuteMsg::UpdateLock {} => execute_update_lock(deps, info),
                ExecuteMsg::NativeReceiveCallback { msg, chain_uid } => {
                    execute_native_receive_callback(&mut deps, env, info, chain_uid, msg)
                }
                ExecuteMsg::UpdateRouterState {
                    admin,
                    vlp_code_id,
                    virtual_balance_address,
                    locked,
                } => execute_update_router_state(
                    deps,
                    info,
                    admin,
                    vlp_code_id,
                    virtual_balance_address,
                    locked,
                ),
                _ => Err(ContractError::UnreachableCode {}),
            }
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::GetState {} => query_state(deps),
        QueryMsg::GetChain { chain_uid } => query_chain(deps, chain_uid),
        QueryMsg::GetAllChains {} => query_all_chains(deps),
        QueryMsg::GetVlp { pair } => query_vlp(deps, pair),
        QueryMsg::GetAllVlps { pagination } => query_all_vlps(deps, pagination),
        QueryMsg::SimulateSwap(msg) => query::query_simulate_swap(deps, msg),
        QueryMsg::SimulateReleaseEscrow {
            token,
            amount,
            cross_chain_addresses,
        } => query_simulate_escrow_release(deps, token, amount, cross_chain_addresses),
        QueryMsg::QueryTokenEscrows { token, pagination } => {
            query_token_escrows(deps, token, pagination)
        }
        QueryMsg::QueryAllEscrows { pagination } => query_all_escrows(deps, pagination),
        QueryMsg::QueryAllTokens { pagination } => query_all_tokens(deps, pagination),
    }
}
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response, ContractError> {
    // If reply id is in HUB_IBC_EXECUTE_MSG_QUEUE_RANGE range of IDS, process it for native ibc wrapper ack call
    // Pros - This way we can reuse existing ack_and _timeout calls instead of managing two flow for native and ibc
    // Cons - Error messages are lost in reply which makes it hard to debug why there was an error. This is fixed from cosmwasm 2.0 probably
    if msg.id.ge(&HUB_IBC_EXECUTE_MSG_QUEUE_RANGE.0)
        && msg.id.le(&HUB_IBC_EXECUTE_MSG_QUEUE_RANGE.1)
    {
        return reply::on_reply_native_ibc_wrapper_call(deps, env, msg);
    }
    match msg.id {
        VLP_INSTANTIATE_REPLY_ID => reply::on_vlp_instantiate_reply(deps, msg),
        VLP_POOL_REGISTER_REPLY_ID => reply::on_pool_register_reply(deps, msg),
        ADD_LIQUIDITY_REPLY_ID => reply::on_add_liquidity_reply(deps, msg),
        REMOVE_LIQUIDITY_REPLY_ID => reply::on_remove_liquidity_reply(deps, env, msg),
        SWAP_REPLY_ID => reply::on_swap_reply(deps, env, msg),

        VIRTUAL_BALANCE_INSTANTIATE_REPLY_ID => {
            reply::on_virtual_balance_instantiate_reply(deps, msg)
        }

        IBC_ACK_AND_TIMEOUT_REPLY_ID => reply::on_ibc_ack_and_timeout_reply(deps, msg),
        IBC_RECEIVE_REPLY_ID => reply::on_ibc_receive_reply(deps, msg),

        id => Err(ContractError::Std(StdError::generic_err(format!(
            "Unknown reply id: {}",
            id
        )))),
    }
}
