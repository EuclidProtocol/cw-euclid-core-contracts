use std::collections::HashMap;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_json_binary, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo, ReplyOn, Reply, Response,
    StdError, SubMsg, Uint512, WasmMsg,
};
use cw2::set_contract_version;
use euclid::admin::EuclidAdmin;
use euclid::cross_chain_user::CrossChainUser;
use euclid::error::ContractError;
use euclid::fee::DenomFees;
use euclid::token::TokenType;
use euclid_ibc::state::NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_RANGE;

use crate::execute::pool::{
    add_concentrated_liquidity_request, add_liquidity_request, collect_concentrated_fees_request,
    collect_concentrated_protocol_fees_request, execute_request_concentrated_pool_creation,
    execute_request_pool_creation, remove_concentrated_liquidity_request,
};
use crate::execute::relay::{
    execute_native_receive_callback, execute_receive_acknowledgement, execute_receive_packet,
    execute_receive_packet_internal_callback, execute_send_packet,
};
use crate::execute::swap::execute_swap_request;
use crate::execute::token::{
    execute_deposit_token, execute_request_deregister_denom, execute_request_register_denom,
    execute_transfer_voucher,
};
use crate::execute::{execute_manage_factory_state, receive_cw20, receive_euclid_native};
use crate::query::{
    get_concentrated_vlp, get_escrow, get_lp_token_address, get_partner_fees_collected,
    get_position_token_contract, get_vlp, pending_liquidity, pending_remove_liquidity,
    pending_swaps, query_all_concentrated_pools, query_all_pools, query_all_tokens, query_state,
};
use crate::rate_limit::{RateLimitState, RATE_LIMIT_STATE};
use crate::reply::{
    self, on_lp_instantiate_reply, on_position_token_instantiate_reply,
    CROSS_CHAIN_RECEIVE_REPLY_ID, LP_INSTANTIATE_REPLY_ID, POSITION_TOKEN_INSTANTIATE_REPLY_ID,
};
use crate::reply::{
    on_escrow_instantiate_reply, on_release_escrow_reply, ESCROW_INSTANTIATE_REPLY_ID,
    RELEASE_ESCROW_REPLY_ID,
};
use crate::state::{FeeState, State, ADMIN, FEE_STATE, STATE};
use cosmwasm_std::ensure;
use euclid::msgs::factory::{ExecuteMsg, InstantiateMsg, QueryMsg};

// version info for migration info
pub(crate) const CONTRACT_NAME: &str = "crates.io:factory";
pub(crate) const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let chain_uid = msg.chain_uid.validate()?.to_owned();
    let state = State {
        router_contract: msg.router_contract.clone(),
        relayer_contract: msg.relayer_contract.clone(),
        escrow_code_id: msg.escrow_code_id,
        lp_code_id: msg.lp_code_id,
        position_token_code_id: msg.position_token_code_id,
        chain_uid,
        is_native: msg.is_native,
    };

    let fee_state = FeeState {
        rate_limit_fee_recipient: msg.rate_limit_fee_recipient.clone(),
        rate_limit_fee_denom: msg.rate_limit_fee_denom.clone(),
        rate_limit_fee_collected: Uint512::zero(),
        partner_fees_collected: DenomFees {
            totals: HashMap::default(),
        },
    };
    FEE_STATE.save(deps.storage, &fee_state)?;
    RATE_LIMIT_STATE.save(
        deps.storage,
        &RateLimitState {
            free_limit: msg.rate_limit_free_limit.u128(),
            fee_brackets: vec![],
        },
    )?;

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    STATE.save(deps.storage, &state)?;
    ADMIN.save(deps.storage, &EuclidAdmin::default(info.sender.clone()))?;

    let init_position_token_msg = CosmosMsg::Wasm(WasmMsg::Instantiate {
        admin: Some(info.sender.into_string()),
        code_id: msg.position_token_code_id,
        msg: to_json_binary(&euclid::msgs::position_token::InstantiateMsg {
            name: "Euclid Concentrated Positions".to_string(),
            symbol: "EUPOS".to_string(),
            minter: env.contract.address.clone(),
            admin: env.contract.address,
        })?,
        funds: vec![],
        label: "position_token".to_string(),
    });

    Ok(Response::new()
        .add_submessage(SubMsg {
            id: POSITION_TOKEN_INSTANTIATE_REPLY_ID,
            msg: init_position_token_msg,
            gas_limit: None,
            reply_on: ReplyOn::Always,
            payload: Binary::default(),
        })
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
        ExecuteMsg::RegisterDenom {
            token_with_denom,
            cross_chain_config,
        } => execute_request_register_denom(
            &mut deps,
            env,
            info,
            token_with_denom,
            cross_chain_config,
        ),
        ExecuteMsg::DeregisterDenom {
            token_with_denom,
            cross_chain_config,
        } => execute_request_deregister_denom(
            &mut deps,
            env,
            info,
            token_with_denom,
            cross_chain_config,
        ),
        ExecuteMsg::DepositToken {
            asset_in,
            amount_in,
            recipients,
            cross_chain_config,
        } => {
            let state = STATE.load(deps.storage)?;
            let sender = CrossChainUser::new(state.chain_uid, info.sender.to_string());

            execute_deposit_token(
                &mut deps,
                env,
                info,
                sender,
                asset_in,
                amount_in,
                recipients,
                cross_chain_config,
            )
        }
        ExecuteMsg::TransferVoucher {
            token_id,
            amount,
            from,
            recipients,
            cross_chain_config,
        } => execute_transfer_voucher(
            &mut deps,
            env,
            info,
            token_id,
            amount,
            from,
            recipients,
            cross_chain_config,
        ),

        ExecuteMsg::RequestPoolCreation {
            pair_with_denom_and_amount,
            pool_config,
            slippage_tolerance_bps,
            lp_token_name,
            lp_token_symbol,
            lp_token_decimal,
            lp_token_marketing,
            cross_chain_config,
        } => execute_request_pool_creation(
            &mut deps,
            env,
            info,
            pair_with_denom_and_amount,
            pool_config,
            lp_token_name,
            lp_token_symbol,
            lp_token_decimal,
            lp_token_marketing,
            slippage_tolerance_bps,
            cross_chain_config,
        ),
        ExecuteMsg::RequestConcentratedPoolCreation {
            pair_with_denom_and_amount,
            fee_tier_bps,
            tick_spacing,
            slippage_tolerance_bps,
            cross_chain_config,
            ..
        } => execute_request_concentrated_pool_creation(
            &mut deps,
            env,
            info,
            pair_with_denom_and_amount,
            fee_tier_bps,
            tick_spacing,
            slippage_tolerance_bps,
            cross_chain_config,
        ),
        ExecuteMsg::AddLiquidity {
            pair_with_denom_and_amount,
            slippage_tolerance_bps,
            cross_chain_config,
        } => add_liquidity_request(
            &mut deps,
            info,
            env,
            pair_with_denom_and_amount,
            slippage_tolerance_bps,
            cross_chain_config,
        ),
        ExecuteMsg::AddConcentratedLiquidity {
            pair_with_denom_and_amount,
            pool_key,
            lower_tick_index,
            upper_tick_index,
            position_id,
            slippage_tolerance_bps,
            cross_chain_config,
        } => add_concentrated_liquidity_request(
            &mut deps,
            info,
            env,
            pair_with_denom_and_amount,
            pool_key,
            lower_tick_index,
            upper_tick_index,
            position_id,
            slippage_tolerance_bps,
            cross_chain_config,
        ),
        ExecuteMsg::RemoveConcentratedLiquidity {
            pool_key,
            position_id,
            lp_allocation,
            recipient,
            cross_chain_config,
        } => {
            let state = STATE.load(deps.storage)?;
            let sender = CrossChainUser::new(state.chain_uid, info.sender.to_string());
            remove_concentrated_liquidity_request(
                &mut deps,
                info,
                env,
                sender,
                pool_key,
                position_id,
                lp_allocation,
                recipient,
                cross_chain_config,
            )
        }
        ExecuteMsg::CollectConcentratedFees {
            pool_key,
            position_id,
            recipient,
            cross_chain_config,
        } => collect_concentrated_fees_request(
            &mut deps,
            info,
            env,
            pool_key,
            position_id,
            recipient,
            cross_chain_config,
        ),
        ExecuteMsg::CollectConcentratedProtocolFees {
            pool_key,
            recipient,
            amount_0_requested,
            amount_1_requested,
            cross_chain_config,
        } => collect_concentrated_protocol_fees_request(
            &mut deps,
            info,
            env,
            pool_key,
            recipient,
            amount_0_requested,
            amount_1_requested,
            cross_chain_config,
        ),
        ExecuteMsg::ExecuteSwapRequest(msg) => {
            let state = STATE.load(deps.storage)?;
            let sender = CrossChainUser::new(state.chain_uid, info.sender.to_string());

            let mut amount_in = msg.amount_in;
            // If this asset is native, lets get the actual amount of funds sent because these amount can vary depending on forwarding contract swaps
            if let TokenType::Native { denom } = &msg.asset_in.token_type {
                amount_in = info
                    .funds
                    .iter()
                    .find(|fund| fund.denom == *denom)
                    .ok_or(ContractError::InsufficientFunds {})?
                    .amount;
            }
            ensure!(
                amount_in.ge(&msg.amount_in),
                ContractError::InsufficientFunds {}
            );

            execute_swap_request(
                &mut deps,
                env,
                info,
                sender,
                msg.asset_in,
                amount_in,
                msg.asset_out,
                msg.min_amount_out,
                msg.swaps,
                msg.recipients,
                msg.cross_chain_config,
                msg.partner_fee,
            )
        }
        ExecuteMsg::ManageFactoryState(msg) => execute_manage_factory_state(deps, env, info, msg),
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::EuclidReceive(msg) => receive_euclid_native(deps, env, info, msg),

        ExecuteMsg::NativeReceiveCallback { msg } => {
            execute_native_receive_callback(&mut deps, env, info, msg)
        }
        // COMSOS ENTRY POINTS FOR RELAYER
        ExecuteMsg::SendPacket {
            sender,
            msg,
            timeout,
            ack_response,
        } => execute_send_packet(deps, info, env, msg, timeout, ack_response, sender),
        ExecuteMsg::ReceivePacket {
            msg,
            sequence,
            source_port,
            destination_port,
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

        ExecuteMsg::ReceivePacketInternalCallback { msg, timeout } => {
            execute_receive_packet_internal_callback(&mut deps, env, info, msg, timeout)
        }
        ExecuteMsg::AcknowledgePacket {
            msg,
            sequence,
            source_port,
            destination_port,
            ack,
        } => execute_receive_acknowledgement(
            &mut deps,
            info,
            env,
            msg,
            sequence,
            source_port,
            destination_port,
            ack,
        ),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::GetVlp { pair } => get_vlp(deps, pair),
        QueryMsg::GetConcentratedVlp { pool_key } => get_concentrated_vlp(deps, pool_key),
        QueryMsg::GetLPToken { vlp } => get_lp_token_address(deps, vlp),
        QueryMsg::GetEscrow { token_id } => get_escrow(deps, token_id),
        QueryMsg::GetState {} => query_state(deps),
        QueryMsg::GetAllPools {} => query_all_pools(deps),
        QueryMsg::GetAllConcentratedPools {} => query_all_concentrated_pools(deps),
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
        QueryMsg::GetPositionTokenContract {} => get_position_token_contract(deps),
    }
}
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(mut deps: DepsMut, env: Env, msg: Reply) -> Result<Response, ContractError> {
    // If reply id is in CHAIN_IBC_EXECUTE_MSG_QUEUE_RANGE range of IDS, process it for native ibc wrapper ack call
    // Pros - This way we can reuse existing ack_and _timeout calls instead of managing two flow for native and ibc
    // Cons - Error messages are lost in reply which makes it hard to debug why there was an error. This is fixed from cosmwasm 2.0 probably
    if msg.id.ge(&NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_RANGE.0)
        && msg.id.le(&NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_RANGE.1)
    {
        return reply::on_reply_native_ibc_wrapper_call(&mut deps, env, msg);
    }
    match msg.id {
        ESCROW_INSTANTIATE_REPLY_ID => on_escrow_instantiate_reply(deps.branch(), msg),
        LP_INSTANTIATE_REPLY_ID => on_lp_instantiate_reply(deps.branch(), msg),
        RELEASE_ESCROW_REPLY_ID => on_release_escrow_reply(deps.branch(), msg),
        CROSS_CHAIN_RECEIVE_REPLY_ID => reply::on_cross_chain_receive_reply(deps.branch(), msg),
        POSITION_TOKEN_INSTANTIATE_REPLY_ID => {
            on_position_token_instantiate_reply(deps.branch(), msg)
        }

        id => Err(ContractError::Std(StdError::generic_err(format!(
            "Unknown reply id: {}",
            id
        )))),
    }
}

#[cfg(test)]
mod tests {}
