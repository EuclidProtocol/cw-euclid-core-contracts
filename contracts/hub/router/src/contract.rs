#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    ensure, to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, StdError,
    SubMsg, WasmMsg,
};
use cw2::set_contract_version;
use euclid::chain::ChainUid;
use euclid::cross_chain_user::CrossChainUser;
use euclid::error::ContractError;
use euclid_ibc::state::NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_RANGE;

use crate::execute::relay::{
    execute_native_receive_callback, execute_receive_acknowledgement, execute_receive_packet,
    execute_receive_packet_internal_callback, execute_send_packet,
};
use crate::execute::token::{execute_transfer_voucher, execute_withdraw_voucher};
use crate::execute::{execute_manage_router_state, execute_meta_receive, execute_register_factory};

use crate::query::{
    self, query_all_chains, query_all_escrows, query_all_tokens, query_all_vlps, query_chain,
    query_relayer_addresses, query_release_fees, query_state, query_token_denoms, query_token_escrows,
    query_vlp, query_vlp_by_pool_key,
};
use crate::reply::{
    self, ADD_LIQUIDITY_REPLY_ID, CROSS_CHAIN_RECEIVE_REPLY_ID, REMOVE_LIQUIDITY_REPLY_ID,
    SWAP_REPLY_ID, VIRTUAL_BALANCE_INSTANTIATE_REPLY_ID, VLP_INSTANTIATE_REPLY_ID,
    VLP_POOL_REGISTER_REPLY_ID,
};
use crate::state::{FeeState, State, FEE_STATE, LOCKED_CHAINS, RELAYER_CONTRACT, STATE};
use euclid::msgs::router::{ExecuteMsg, InstantiateMsg, QueryMsg};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:router";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let state = State {
        constant_product_vlp_code_id: msg.constant_product_vlp_code_id,
        stable_vlp_code_id: msg.stable_vlp_code_id,
        concentrated_vlp_code_id: msg.concentrated_vlp_code_id,
        admin: info.sender.clone(),
        locked: false,
    };
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    RELAYER_CONTRACT.save(deps.storage, &msg.relayer_contract)?;
    LOCKED_CHAINS.save(deps.storage, &vec![])?;

    STATE.save(deps.storage, &state)?;
    FEE_STATE.save(
        deps.storage,
        &FeeState {
            release_fee_recipient: msg.release_fee_recipient,
            default_fee_recipient: msg.default_fee_recipient,
        },
    )?;

    let virtual_balance_instantiate_msg = euclid::msgs::virtual_balance::msg::InstantiateMsg {
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
        ExecuteMsg::ManageRouterState(msg) => execute_manage_router_state(deps, info, msg),
        _ => {
            // Only allow these messages if the contract is not locked
            ensure!(
                !STATE.load(deps.storage)?.locked,
                ContractError::ContractLocked {}
            );
            match msg {
                ExecuteMsg::ManageRouterState(msg) => execute_manage_router_state(deps, info, msg),
                ExecuteMsg::RegisterFactory {
                    chain_uid,
                    chain_info,
                } => execute_register_factory(&mut deps, env, info, chain_uid, chain_info),
                ExecuteMsg::WithdrawVoucher {
                    token,
                    amount,
                    recipient,
                    cross_chain_config,
                } => {
                    let verified_sender =
                        CrossChainUser::new(ChainUid::vsl_chain_uid()?, info.sender.to_string());
                    execute_withdraw_voucher(
                        &mut deps,
                        env,
                        verified_sender,
                        token,
                        amount,
                        recipient,
                        cross_chain_config,
                    )
                }
                ExecuteMsg::TransferVoucher {
                    token,
                    amount,
                    recipient,
                } => {
                    let verified_sender =
                        CrossChainUser::new(ChainUid::vsl_chain_uid()?, info.sender.to_string());
                    execute_transfer_voucher(
                        &mut deps,
                        env,
                        verified_sender,
                        token,
                        amount,
                        recipient,
                    )
                }
                ExecuteMsg::NativeReceiveCallback { msg, chain_uid } => {
                    execute_native_receive_callback(&mut deps, env, info, chain_uid, msg)
                }
                ExecuteMsg::SendPacket {
                    msg,
                    chain,
                    sender,
                    timeout,
                    ack_response,
                } => {
                    execute_send_packet(deps, info, env, chain, msg, timeout, ack_response, sender)
                }
                ExecuteMsg::ReceivePacket {
                    source_port,
                    destination_port,
                    msg,
                    sequence,
                    timeout,
                } => execute_receive_packet(
                    deps,
                    info,
                    env,
                    msg,
                    sequence,
                    source_port,
                    destination_port,
                    timeout,
                ),
                ExecuteMsg::ReceivePacketInternalCallback {
                    msg,
                    chain_uid,
                    timeout,
                } => execute_receive_packet_internal_callback(
                    &mut deps, env, info, msg, chain_uid, timeout,
                ),
                ExecuteMsg::AcknowledgePacket {
                    source_port,
                    destination_port,
                    msg,
                    sequence,
                    ack,
                } => execute_receive_acknowledgement(
                    deps,
                    info,
                    env,
                    msg,
                    sequence,
                    source_port,
                    destination_port,
                    ack,
                ),

                ExecuteMsg::MetaReceive(msg) => execute_meta_receive(&mut deps, env, info, msg),
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
        QueryMsg::GetVlpByPoolKey { pool_key } => query_vlp_by_pool_key(deps, pool_key),
        QueryMsg::GetAllVlps { pagination } => query_all_vlps(deps, pagination),
        QueryMsg::SimulateSwap(msg) => query::query_simulate_swap(deps, msg),
        QueryMsg::QueryTokenEscrows { token, pagination } => {
            query_token_escrows(deps, token, pagination)
        }
        QueryMsg::QueryAllEscrows { pagination } => query_all_escrows(deps, pagination),
        QueryMsg::QueryAllTokens { pagination } => query_all_tokens(deps, pagination),
        QueryMsg::QueryTokenDenoms { token } => query_token_denoms(deps, token),
        QueryMsg::QueryRelayerAddresses {} => query_relayer_addresses(deps),
        QueryMsg::GetReleaseFees { pagination } => query_release_fees(deps, pagination),
    }
}
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(mut deps: DepsMut, env: Env, msg: Reply) -> Result<Response, ContractError> {
    // If reply id is in HUB_IBC_EXECUTE_MSG_QUEUE_RANGE range of IDS, process it for native ibc wrapper ack call
    // Pros - This way we can reuse existing ack_and _timeout calls instead of managing two flow for native and ibc
    // Cons - Error messages are lost in reply which makes it hard to debug why there was an error. This is fixed from cosmwasm 2.0 probably
    if msg.id.ge(&NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_RANGE.0)
        && msg.id.le(&NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_RANGE.1)
    {
        return reply::on_reply_native_ibc_wrapper_call(deps, env, msg);
    }
    match msg.id {
        VLP_INSTANTIATE_REPLY_ID => reply::on_vlp_instantiate_reply(deps, msg),
        VLP_POOL_REGISTER_REPLY_ID => reply::on_pool_register_reply(deps, msg),
        ADD_LIQUIDITY_REPLY_ID => reply::on_add_liquidity_reply(deps, msg),
        REMOVE_LIQUIDITY_REPLY_ID => reply::on_remove_liquidity_reply(deps, env, msg),
        SWAP_REPLY_ID => reply::on_swap_reply(&mut deps, env, msg),
        VIRTUAL_BALANCE_INSTANTIATE_REPLY_ID => {
            reply::on_virtual_balance_instantiate_reply(deps, msg)
        }
        CROSS_CHAIN_RECEIVE_REPLY_ID => reply::on_cross_chain_receive_reply(deps, msg),

        id => Err(ContractError::Std(StdError::generic_err(format!(
            "Unknown reply id: {}",
            id
        )))),
    }
}
