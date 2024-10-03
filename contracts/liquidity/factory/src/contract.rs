use std::collections::HashMap;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, StdError};
use cw2::set_contract_version;
use euclid::chain::CrossChainUser;
use euclid::error::ContractError;
use euclid::fee::DenomFees;
use euclid_ibc::msg::CHAIN_IBC_EXECUTE_MSG_QUEUE_RANGE;

use crate::execute::{
    add_liquidity_request, execute_deposit_token, execute_native_receive_callback,
    execute_request_deregister_denom, execute_request_pool_creation,
    execute_request_register_denom, execute_request_register_escrow, execute_swap_request,
    execute_transfer_virtual_balance, execute_update_hub_channel, execute_update_state,
    execute_withdraw_virtual_balance, receive_cw20,
};
use crate::query::{
    get_escrow, get_lp_token_address, get_partner_fees_collected, get_vlp, pending_liquidity,
    pending_remove_liquidity, pending_swaps, query_all_pools, query_all_tokens, query_state,
};
use crate::reply::{
    on_cw20_instantiate_reply, on_escrow_instantiate_reply, on_ibc_ack_and_timeout_reply,
    on_ibc_receive_reply, CW20_INSTANTIATE_REPLY_ID, ESCROW_INSTANTIATE_REPLY_ID,
    IBC_ACK_AND_TIMEOUT_REPLY_ID, IBC_RECEIVE_REPLY_ID,
};
use crate::state::{State, STATE};
use crate::{ibc, reply};
use euclid::msgs::factory::{ExecuteMsg, InstantiateMsg, QueryMsg};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:factory";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let chain_uid = msg.chain_uid.validate()?.to_owned();
    let state = State {
        router_contract: msg.router_contract.clone(),
        admin: info.sender.clone().to_string(),
        escrow_code_id: msg.escrow_code_id,
        cw20_code_id: msg.cw20_code_id,
        chain_uid,
        is_native: msg.is_native,
        partner_fees_collected: DenomFees {
            totals: HashMap::default(),
        },
    };

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("router_contract", msg.router_contract)
        .add_attribute("escrow_code_id", state.escrow_code_id.to_string())
        .add_attribute("chain_uid", state.chain_uid.to_string()))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::AddLiquidityRequest {
            pair_info,
            slippage_tolerance_bps,
            timeout,
        } => add_liquidity_request(
            &mut deps,
            info,
            env,
            pair_info,
            slippage_tolerance_bps,
            timeout,
        ),
        ExecuteMsg::ExecuteSwapRequest {
            asset_in,
            amount_in,
            asset_out,
            min_amount_out,
            timeout,
            swaps,
            cross_chain_addresses,
            partner_fee,
        } => {
            let state = STATE.load(deps.storage)?;
            let sender = CrossChainUser {
                address: info.sender.to_string(),
                chain_uid: state.chain_uid,
            };
            execute_swap_request(
                &mut deps,
                env,
                info,
                sender,
                asset_in,
                amount_in,
                asset_out,
                min_amount_out,
                swaps,
                timeout,
                cross_chain_addresses,
                partner_fee,
            )
        }
        ExecuteMsg::DepositToken {
            amount_in,
            asset_in,
            recipient,
            timeout,
        } => {
            let state = STATE.load(deps.storage)?;
            let sender = CrossChainUser {
                address: info.sender.to_string(),
                chain_uid: state.chain_uid,
            };

            execute_deposit_token(
                &mut deps, env, info, sender, asset_in, amount_in, timeout, recipient,
            )
        }
        ExecuteMsg::UpdateHubChannel { new_channel } => {
            execute_update_hub_channel(deps, info, new_channel)
        }
        ExecuteMsg::RequestRegisterDenom { token } => {
            execute_request_register_denom(deps, info, token)
        }
        ExecuteMsg::RequestDeregisterDenom { token } => {
            execute_request_deregister_denom(deps, info, token)
        }
        ExecuteMsg::RequestPoolCreation {
            pair,
            lp_token_name,
            lp_token_symbol,
            lp_token_decimal,
            lp_token_marketing,
            timeout,
        } => execute_request_pool_creation(
            &mut deps,
            env,
            info,
            pair,
            lp_token_name,
            lp_token_symbol,
            lp_token_decimal,
            lp_token_marketing,
            timeout,
        ),
        ExecuteMsg::RequestRegisterEscrow { token, timeout } => {
            execute_request_register_escrow(&mut deps, env, info, token, timeout)
        }
        ExecuteMsg::WithdrawVirtualBalance {
            token,
            amount,
            cross_chain_addresses,
            timeout,
        } => execute_withdraw_virtual_balance(
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
            amount,
            recipient_address,
            timeout,
        } => execute_transfer_virtual_balance(
            &mut deps,
            env,
            info,
            token,
            amount,
            recipient_address,
            timeout,
        ),
        ExecuteMsg::UpdateFactoryState {
            router_contract,
            admin,
            escrow_code_id,
            cw20_code_id,
            is_native,
        } => execute_update_state(
            deps,
            info,
            router_contract,
            admin,
            escrow_code_id,
            cw20_code_id,
            is_native,
        ),
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::IbcCallbackAckAndTimeout { ack } => {
            ibc::ack_and_timeout::ibc_ack_packet_internal_call(deps, env, ack)
        }
        ExecuteMsg::IbcCallbackReceive { receive_msg } => {
            ibc::receive::ibc_receive_internal_call(deps, env, receive_msg)
        }
        ExecuteMsg::NativeReceiveCallback { msg } => {
            execute_native_receive_callback(deps, env, info, msg)
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::GetVlp { pair } => get_vlp(deps, pair),
        QueryMsg::GetLPToken { vlp } => get_lp_token_address(deps, vlp),
        QueryMsg::GetEscrow { token_id } => get_escrow(deps, token_id),
        QueryMsg::GetState {} => query_state(deps),
        QueryMsg::GetAllPools {} => query_all_pools(deps),
        // Pool Queries //
        QueryMsg::PendingSwapsUser { user, pagination } => pending_swaps(deps, user, pagination),
        QueryMsg::PendingLiquidity { user, pagination } => {
            pending_liquidity(deps, user, pagination)
        }
        QueryMsg::PendingRemoveLiquidity { user, pagination } => {
            pending_remove_liquidity(deps, user, pagination)
        }
        QueryMsg::GetAllTokens {} => query_all_tokens(deps),
        QueryMsg::GetPartnerFeesCollected {} => get_partner_fees_collected(deps),
    }
}
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response, ContractError> {
    // If reply id is in CHAIN_IBC_EXECUTE_MSG_QUEUE_RANGE range of IDS, process it for native ibc wrapper ack call
    // Pros - This way we can reuse existing ack_and _timeout calls instead of managing two flow for native and ibc
    // Cons - Error messages are lost in reply which makes it hard to debug why there was an error. This is fixed from cosmwasm 2.0 probably
    if msg.id.ge(&CHAIN_IBC_EXECUTE_MSG_QUEUE_RANGE.0)
        && msg.id.le(&CHAIN_IBC_EXECUTE_MSG_QUEUE_RANGE.1)
    {
        return reply::on_reply_native_ibc_wrapper_call(deps, env, msg);
    }
    match msg.id {
        ESCROW_INSTANTIATE_REPLY_ID => on_escrow_instantiate_reply(deps, msg),
        CW20_INSTANTIATE_REPLY_ID => on_cw20_instantiate_reply(deps, msg),
        IBC_ACK_AND_TIMEOUT_REPLY_ID => on_ibc_ack_and_timeout_reply(deps, msg),
        IBC_RECEIVE_REPLY_ID => on_ibc_receive_reply(deps, msg),
        id => Err(ContractError::Std(StdError::generic_err(format!(
            "Unknown reply id: {}",
            id
        )))),
    }
}

#[cfg(test)]
mod tests {}
