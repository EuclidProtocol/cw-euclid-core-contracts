use std::collections::HashMap;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, StdError, Uint512};
use cw2::set_contract_version;
use euclid::admin::EuclidAdmin;
use euclid::cross_chain_user::CrossChainUser;
use euclid::error::ContractError;
use euclid::fee::DenomFees;
use euclid::token::TokenType;
use euclid_ibc::state::NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_RANGE;

use crate::execute::pool::{add_liquidity_request, execute_request_pool_creation};
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
    get_escrow, get_lp_token_address, get_partner_fees_collected, get_vlp, pending_liquidity,
    pending_remove_liquidity, pending_swaps, query_all_pools, query_all_tokens, query_state,
};
use crate::rate_limit::{RateLimitState, RATE_LIMIT_STATE};
use crate::reply::{
    self, on_lp_instantiate_reply, CROSS_CHAIN_RECEIVE_REPLY_ID, LP_INSTANTIATE_REPLY_ID,
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
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let chain_uid = msg.chain_uid.validate()?.to_owned();
    let state = State {
        router_contract: msg.router_contract.clone(),
        relayer_contract: msg.relayer_contract.clone(),
        escrow_code_id: msg.escrow_code_id,
        lp_code_id: msg.lp_code_id,
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

        id => Err(ContractError::Std(StdError::generic_err(format!(
            "Unknown reply id: {}",
            id
        )))),
    }
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{
        testing::{message_info, mock_dependencies, mock_env},
        to_json_binary, Addr, Uint128,
    };
    use euclid::{
        chain::ChainUid,
        msgs::factory::{ExecuteMsg, InstantiateMsg},
    };

    use crate::{
        rate_limit::RATE_LIMIT_STATE,
        state::ADMIN,
        testing::helpers::{
            init, load_fee_state, load_state, TEST_CHAIN_UID, TEST_RATE_LIMIT_FEE_RECIPIENT,
            TEST_RELAYER, TEST_ROUTER,
        },
    };

    use super::{execute, instantiate};

    // -----------------------------------------------------------------------
    // Instantiate
    // -----------------------------------------------------------------------

    #[test]
    fn test_instantiate_stores_correct_state() {
        let mut deps = mock_dependencies();
        let res = init(&mut deps);

        assert!(res
            .attributes
            .iter()
            .any(|a| a.key == "method" && a.value == "instantiate"));
        assert!(res
            .attributes
            .iter()
            .any(|a| a.key == "router_contract" && a.value == TEST_ROUTER));
        assert!(res
            .attributes
            .iter()
            .any(|a| a.key == "chain_uid" && a.value == TEST_CHAIN_UID));

        let state = load_state(&deps);
        assert_eq!(state.router_contract, TEST_ROUTER);
        assert_eq!(state.relayer_contract, Addr::unchecked(TEST_RELAYER));
        assert_eq!(state.escrow_code_id, 10);
        assert_eq!(state.lp_code_id, 11);
        assert!(!state.is_native);
        assert_eq!(
            state.chain_uid,
            ChainUid::create(TEST_CHAIN_UID.to_string()).unwrap()
        );

        let admin = ADMIN.load(&deps.storage).unwrap();
        let sender = deps.api.addr_make("sender");
        assert_eq!(admin.general_admin, sender);
        assert_eq!(admin.fee_admin, sender);
        assert_eq!(admin.migration_admin, sender);
    }

    #[test]
    fn test_instantiate_stores_fee_state() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let fee_state = load_fee_state(&deps);
        assert_eq!(
            fee_state.rate_limit_fee_recipient,
            Addr::unchecked(TEST_RATE_LIMIT_FEE_RECIPIENT)
        );
        assert_eq!(fee_state.rate_limit_fee_denom, "uusd");
        assert_eq!(
            fee_state.rate_limit_fee_collected,
            cosmwasm_std::Uint512::zero()
        );
        assert!(fee_state.partner_fees_collected.totals.is_empty());
    }

    #[test]
    fn test_instantiate_stores_rate_limit_state() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let rl = RATE_LIMIT_STATE.load(&deps.storage).unwrap();
        assert_eq!(rl.free_limit, 100);
        assert!(rl.fee_brackets.is_empty());
    }

    #[test]
    fn test_instantiate_accepts_valid_chain_uid() {
        let mut deps = mock_dependencies();
        let sender = deps.api.addr_make("sender");
        let info = message_info(&sender, &[]);
        let msg = InstantiateMsg {
            router_contract: TEST_ROUTER.to_string(),
            chain_uid: ChainUid::create("validuid123".to_string()).unwrap(),
            escrow_code_id: 10,
            lp_code_id: 11,
            is_native: false,
            relayer_contract: Addr::unchecked(TEST_RELAYER),
            rate_limit_fee_recipient: Addr::unchecked(TEST_RATE_LIMIT_FEE_RECIPIENT),
            rate_limit_fee_denom: "uusd".to_string(),
            rate_limit_free_limit: Uint128::new(100),
        };
        assert!(instantiate(deps.as_mut(), mock_env(), info, msg).is_ok());
    }

    // -----------------------------------------------------------------------
    // Execute: SendPacket – only callable by the contract itself
    // -----------------------------------------------------------------------

    #[test]
    fn test_send_packet_unauthorized_external_caller() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let external = deps.api.addr_make("external_caller");
        let info = message_info(&external, &[]);
        let msg = ExecuteMsg::SendPacket {
            msg: to_json_binary(&"test").unwrap(),
            timeout: None,
            ack_response: None,
            sender: external.clone(),
        };
        let res = execute(deps.as_mut(), mock_env(), info, msg);
        assert_eq!(
            res.unwrap_err(),
            euclid::error::ContractError::Unauthorized {}
        );
    }
}
