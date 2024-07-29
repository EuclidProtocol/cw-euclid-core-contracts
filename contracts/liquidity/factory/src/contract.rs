#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, StdError};
use cw2::set_contract_version;
use euclid::chain::CrossChainUser;
use euclid::error::ContractError;

use crate::execute::{
    add_liquidity_request, execute_request_deregister_denom, execute_request_pool_creation,
    execute_request_register_denom, execute_swap_request, execute_update_hub_channel,
    execute_withdraw_vcoin, receive_cw20,
};
use crate::query::{
    get_escrow, get_lp_token_address, get_vlp, pending_liquidity, pending_remove_liquidity,
    pending_swaps, query_all_pools, query_all_tokens, query_state,
};
use crate::reply::{
    CW20_INSTANTIATE_REPLY_ID, ESCROW_INSTANTIATE_REPLY_ID, IBC_ACK_AND_TIMEOUT_REPLY_ID,
    IBC_RECEIVE_REPLY_ID,
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
            token_1_liquidity,
            token_2_liquidity,
            slippage_tolerance,
            timeout,
        } => add_liquidity_request(
            &mut deps,
            info,
            env,
            pair_info,
            token_1_liquidity,
            token_2_liquidity,
            slippage_tolerance,
            timeout,
        ),
        ExecuteMsg::ExecuteSwapRequest {
            asset_in,
            asset_out,
            amount_in,
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
                info,
                env,
                sender,
                asset_in,
                asset_out,
                amount_in,
                min_amount_out,
                swaps,
                timeout,
                cross_chain_addresses,
                partner_fee,
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
        ExecuteMsg::WithdrawVcoin {
            token,
            amount,
            cross_chain_addresses,
            timeout,
        } => execute_withdraw_vcoin(
            deps,
            env,
            info,
            token,
            amount,
            cross_chain_addresses,
            timeout,
        ),

        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::IbcCallbackAckAndTimeout { ack } => {
            ibc::ack_and_timeout::ibc_ack_packet_internal_call(deps, env, ack)
        }
        ExecuteMsg::IbcCallbackReceive { receive_msg } => {
            ibc::receive::ibc_receive_internal_call(deps, env, receive_msg)
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
        QueryMsg::PendingSwapsUser {
            user,
            upper_limit,
            lower_limit,
        } => pending_swaps(deps, user, lower_limit, upper_limit),
        QueryMsg::PendingLiquidity {
            user,
            lower_limit,
            upper_limit,
        } => pending_liquidity(deps, user, lower_limit, upper_limit),
        QueryMsg::PendingRemoveLiquidity {
            user,
            lower_limit,
            upper_limit,
        } => pending_remove_liquidity(deps, user, lower_limit, upper_limit),
        QueryMsg::GetAllTokens {} => query_all_tokens(deps),
    }
}
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        ESCROW_INSTANTIATE_REPLY_ID => reply::on_escrow_instantiate_reply(deps, msg),
        CW20_INSTANTIATE_REPLY_ID => reply::on_cw20_instantiate_reply(deps, msg),
        IBC_ACK_AND_TIMEOUT_REPLY_ID => reply::on_ibc_ack_and_timeout_reply(deps, msg),
        IBC_RECEIVE_REPLY_ID => reply::on_ibc_receive_reply(deps, msg),
        id => Err(ContractError::Std(StdError::generic_err(format!(
            "Unknown reply id: {}",
            id
        )))),
    }
}

#[cfg(test)]
mod tests {}
