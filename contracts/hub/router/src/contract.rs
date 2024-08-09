#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, StdError, SubMsg,
    WasmMsg,
};
use cw2::set_contract_version;
use euclid::chain::ChainUid;
use euclid::error::ContractError;

use crate::execute::{
    execute_deregister_chain, execute_register_factory, execute_release_escrow,
    execute_reregister_chain, execute_update_lock, execute_update_vlp_code_id,
};
use crate::ibc::ack_and_timeout::ibc_ack_packet_internal_call;
use crate::ibc::receive::ibc_receive_internal_call;
use crate::query::{
    self, query_all_chains, query_all_tokens, query_all_vlps, query_chain,
    query_simulate_escrow_release, query_state, query_token_escrows, query_vlp,
};
use crate::reply::{
    self, ADD_LIQUIDITY_REPLY_ID, IBC_ACK_AND_TIMEOUT_REPLY_ID, IBC_RECEIVE_REPLY_ID,
    REMOVE_LIQUIDITY_REPLY_ID, SWAP_REPLY_ID, VCOIN_BURN_REPLY_ID, VCOIN_INSTANTIATE_REPLY_ID,
    VCOIN_MINT_REPLY_ID, VCOIN_TRANSFER_REPLY_ID, VLP_INSTANTIATE_REPLY_ID,
    VLP_POOL_REGISTER_REPLY_ID,
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
        vcoin_address: None,
        locked: false,
    };
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    STATE.save(deps.storage, &state)?;

    let vcoin_instantiate_msg = euclid::msgs::vcoin::InstantiateMsg {
        router: env.contract.address.clone(),
        admin: Some(info.sender.clone()),
    };
    let vcoin_instantiate_msg = WasmMsg::Instantiate {
        admin: Some(info.sender.to_string()),
        code_id: msg.vcoin_code_id,
        msg: to_json_binary(&vcoin_instantiate_msg)?,
        funds: vec![],
        label: "Instantiate VCoin Contract".to_string(),
    };

    let vcoin_instantiate_msg =
        SubMsg::reply_always(vcoin_instantiate_msg, VCOIN_INSTANTIATE_REPLY_ID);

    let empty_chains: Vec<ChainUid> = vec![];
    DEREGISTERED_CHAINS.save(deps.storage, &empty_chains)?;

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("router_contract", env.contract.address)
        .add_submessage(vcoin_instantiate_msg))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    // If the contract is locked and the message isn't UpdateLock, return error
    let locked = STATE.load(deps.storage)?.locked;

    if locked {
        if let ExecuteMsg::UpdateLock {} = msg {
            execute_update_lock(deps, info)
        } else if let ExecuteMsg::ReregisterChain { chain } = msg {
            execute_reregister_chain(deps, info, chain)
        } else if let ExecuteMsg::DeregisterChain { chain } = msg {
            execute_deregister_chain(deps, info, chain)
        } else {
            Err(ContractError::ContractLocked {})
        }
    } else {
        match msg {
            ExecuteMsg::ReregisterChain { chain } => execute_reregister_chain(deps, info, chain),
            ExecuteMsg::DeregisterChain { chain } => execute_deregister_chain(deps, info, chain),
            ExecuteMsg::UpdateVLPCodeId { new_vlp_code_id } => {
                execute_update_vlp_code_id(deps, info, new_vlp_code_id)
            }
            ExecuteMsg::RegisterFactory {
                channel,
                timeout,
                chain_uid,
            } => execute_register_factory(&mut deps, env, info, chain_uid, channel, timeout),
            ExecuteMsg::ReleaseEscrowInternal {
                sender,
                token,
                amount,
                cross_chain_addresses,
                timeout,
                tx_id,
            } => execute_release_escrow(
                deps,
                env,
                info,
                sender,
                token,
                amount,
                cross_chain_addresses,
                timeout,
                tx_id,
            ),
            ExecuteMsg::IbcCallbackReceive { receive_msg } => {
                ibc_receive_internal_call(deps, env, info, receive_msg)
            }
            ExecuteMsg::IbcCallbackAckAndTimeout { ack } => {
                ibc_ack_packet_internal_call(deps, env, ack)
            }
            ExecuteMsg::UpdateLock {} => execute_update_lock(deps, info),
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
        QueryMsg::GetAllVlps {
            start,
            end,
            skip,
            limit,
        } => query_all_vlps(deps, start, end, skip, limit),
        QueryMsg::SimulateSwap(msg) => query::query_simulate_swap(deps, msg),
        QueryMsg::SimulateReleaseEscrow {
            token,
            amount,
            cross_chain_addresses,
        } => query_simulate_escrow_release(deps, token, amount, cross_chain_addresses),
        QueryMsg::QueryTokenEscrows {
            token,
            start,
            end,
            skip,
            limit,
        } => query_token_escrows(deps, token, start, end, skip, limit),
        QueryMsg::QueryAllTokens {
            start,
            end,
            skip,
            limit,
        } => query_all_tokens(deps, start, end, skip, limit),
    }
}
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        VLP_INSTANTIATE_REPLY_ID => reply::on_vlp_instantiate_reply(deps, msg),
        VLP_POOL_REGISTER_REPLY_ID => reply::on_pool_register_reply(deps, msg),
        ADD_LIQUIDITY_REPLY_ID => reply::on_add_liquidity_reply(deps, msg),
        REMOVE_LIQUIDITY_REPLY_ID => reply::on_remove_liquidity_reply(deps, env, msg),
        SWAP_REPLY_ID => reply::on_swap_reply(deps, env, msg),

        VCOIN_INSTANTIATE_REPLY_ID => reply::on_vcoin_instantiate_reply(deps, msg),

        VCOIN_MINT_REPLY_ID => reply::on_vcoin_mint_reply(deps, msg),
        VCOIN_BURN_REPLY_ID => reply::on_vcoin_burn_reply(deps, msg),
        VCOIN_TRANSFER_REPLY_ID => reply::on_vcoin_transfer_reply(deps, msg),

        IBC_ACK_AND_TIMEOUT_REPLY_ID => reply::on_ibc_ack_and_timeout_reply(deps, msg),
        IBC_RECEIVE_REPLY_ID => reply::on_ibc_receive_reply(deps, msg),

        id => Err(ContractError::Std(StdError::generic_err(format!(
            "Unknown reply id: {}",
            id
        )))),
    }
}
