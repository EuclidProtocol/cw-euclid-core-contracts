use cosmwasm_std::{
    ensure, Decimal, DepsMut, Env, Int256, MessageInfo, Response, SubMsg, Uint128, Uint256,
};
use cw20::Logo;
use euclid::{
    cross_chain_user::CrossChainUser,
    error::ContractError,
    events::{tx_event, TxType},
    fee::{PartnerFee, BPS_100_PERCENT, MAX_PARTNER_FEE_BPS},
    liquidity::{
        AddLiquidityRequest, RemoveLiquidityRequest, SingleSidedLiquidityRequest, MAX_TICK,
        MIN_TICK,
    },
    msgs::{
        self,
        cross_chain_config::CrossChainConfig,
        escrow::AllowedTokenResponse,
        position_token::{
            OwnerOfResponse, PositionInfoResponse, QueryMsg as PositionTokenQueryMsg,
        },
        vlp::base::{PoolConfig, PoolType},
    },
    swap::NextSwapPair,
    token::{Pair, PairWithDenomAndAmount, TokenType, TokenWithDenom},
    utils::{fund_manager::FundManager, tx::generate_tx},
};
use euclid_ibc::router_ibc::{
    RouterCrossChainConcentratedAddLiquidityExecuteMsg,
    RouterCrossChainConcentratedCollectFeesExecuteMsg,
    RouterCrossChainConcentratedCollectProtocolFeesExecuteMsg,
    RouterCrossChainConcentratedRemoveLiquidityExecuteMsg,
    RouterCrossChainConcentratedRequestPoolCreationExecuteMsg, RouterCrossChainExecuteMsg,
    RouterCrossChainRemoveLiquidityExecuteMsg, RouterCrossChainSingleSidedAddLiquidityMsg,
};

use crate::{
    execute::proxy::pool_factory_is_initialised,
    query::get_chain_type,
    state::{
        ConcentratedAddLiquidityRequest, ConcentratedCollectFeesRequest,
        ConcentratedCollectProtocolFeesRequest, ConcentratedPoolCreateRequest,
        ConcentratedRemoveLiquidityRequest, PoolCreateRequest, ADMIN, PAIR_TO_VLP,
        PENDING_ADD_LIQUIDITY, PENDING_CONCENTRATED_ADD_LIQUIDITY,
        PENDING_CONCENTRATED_COLLECT_FEES, PENDING_CONCENTRATED_COLLECT_PROTOCOL_FEES,
        PENDING_CONCENTRATED_POOL_REQUESTS, PENDING_CONCENTRATED_REMOVE_LIQUIDITY,
        PENDING_POOL_REQUESTS, PENDING_REMOVE_LIQUIDITY, PENDING_SINGLE_SIDED_LIQUIDITY,
        POOL_FACTORY_ADDRESS, POOL_KEY_TO_VLP, POSITION_TOKEN_CONTRACT, STATE, TOKEN_TO_ESCROW,
        VLP_TO_LP_TOKEN,
    },
};

/// Validates tokens for pool creation: checks escrow registration, collects fund
/// transfers, and ensures at least one token is already registered.
fn validate_pool_creation_tokens(
    deps: &DepsMut,
    env: &Env,
    pair_with_denom_and_amount: &PairWithDenomAndAmount,
    sender_address: &str,
    fund_manager: &mut FundManager,
) -> Result<Vec<SubMsg>, ContractError> {
    let mut msgs: Vec<SubMsg> = Vec::new();
    let mut one_token_already_exists = false;

    let tokens = pair_with_denom_and_amount.get_vec_token_info();
    for token in tokens {
        token.token.validate()?;

        if !token.token_type.is_voucher() {
            match token.token_type.clone() {
                TokenType::Native { denom, .. } => {
                    fund_manager.use_fund(token.amount, &denom)?;
                }
                TokenType::Smart { .. } => {
                    let msg = token.token_type.create_transfer_msg(
                        token.amount,
                        env.contract.address.clone().to_string(),
                        Some(sender_address.to_string()),
                        None,
                    )?;
                    msgs.push(SubMsg::new(msg));
                }
                TokenType::Voucher { .. } => return Err(ContractError::UnreachableCode {}),
            }
            let escrow_address = TOKEN_TO_ESCROW.may_load(deps.storage, token.clone().token)?;
            if let Some(escrow_address) = escrow_address {
                let token_allowed_query_msg = euclid::msgs::escrow::QueryMsg::TokenAllowed {
                    denom: token.clone().token_type,
                };
                let token_allowed: AllowedTokenResponse = deps
                    .querier
                    .query_wasm_smart(escrow_address.clone(), &token_allowed_query_msg)?;

                ensure!(
                    token_allowed.allowed,
                    ContractError::UnsupportedDenomination {}
                );
                one_token_already_exists = true;
            }
        } else {
            one_token_already_exists = true;
        }
    }

    ensure!(
        one_token_already_exists,
        ContractError::new(
            "Cannot create pool two new tokens. Atleast one token must already be registered."
        )
    );

    Ok(msgs)
}

fn validate_concentrated_fee_and_spacing(
    fee_tier_bps: u64,
    tick_spacing: u64,
) -> Result<(), ContractError> {
    let expected_tick_spacing = match fee_tier_bps {
        100 => 1,
        500 => 10,
        3_000 => 60,
        10_000 => 200,
        _ => return Err(ContractError::new("Invalid concentrated fee tier")),
    };

    ensure!(
        tick_spacing == expected_tick_spacing,
        ContractError::new(
            format!(
                "Invalid tick spacing {} for fee tier {}. Expected {}",
                tick_spacing, fee_tier_bps, expected_tick_spacing
            )
            .as_str()
        )
    );
    Ok(())
}

// Function to send IBC request to Router in VSL to create a new pool
pub fn execute_request_pool_creation(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    pair_with_denom_and_amount: PairWithDenomAndAmount,
    pool_config: PoolConfig,
    lp_token_name: String,
    lp_token_symbol: String,
    lp_token_decimal: u8,
    lp_token_marketing: Option<cw20_base::msg::InstantiateMarketingInfo>,
    slippage_tolerance_bps: u64,
    cross_chain_config: CrossChainConfig,
) -> Result<Response, ContractError> {
    ensure!(
        slippage_tolerance_bps.le(&BPS_100_PERCENT),
        ContractError::InvalidSlippageTolerance {}
    );

    let pair = pair_with_denom_and_amount.get_pair()?;

    pair.validate()?;

    let state = STATE.load(deps.storage)?;
    let sender = CrossChainUser::new(state.chain_uid.clone(), info.sender.to_string());
    let tx_id = generate_tx(deps, &env, &sender)?;

    let mut fund_manager = FundManager::new(&info.funds);
    let msgs = validate_pool_creation_tokens(
        deps,
        &env,
        &pair_with_denom_and_amount,
        &sender.address,
        &mut fund_manager,
    )?;
    let res = Response::new().add_submessages(msgs);

    let pair = pair_with_denom_and_amount.get_pair()?;
    // Ensure tokens in pair are different
    ensure!(
        pair.token_1 != pair.token_2,
        ContractError::new("Cannot create pool with same token")
    );

    // Slice 1: once pool_factory has been wired and migration accepted,
    // delegate the CP/Stable pool creation flow. Main Factory keeps fund
    // custody (escrow deposits + validation above) and hands the typed
    // request to pool_factory, which builds the outbound packet via its
    // `outbound` module and returns it as `Response::data`. Main factory's
    // `on_pool_factory_delegate_reply` consumes the payload and runs the
    // outbound dispatch.
    if pool_factory_is_initialised(deps)? {
        // We might get errors in ack if marketing is not valid
        if let Some(marketing) = &lp_token_marketing {
            if let Some(logo) = &marketing.logo {
                ensure!(
                    matches!(logo, Logo::Url(_)),
                    ContractError::new("Only URL logos are supported")
                );
            }
            if let Some(marketing_address) = &marketing.marketing {
                deps.api.addr_validate(marketing_address)?;
            }
        }
        let pool_factory = POOL_FACTORY_ADDRESS.load(deps.storage)?;
        let exec = euclid::msgs::pool_factory::ExecuteMsg::OnRequestPoolCreation {
            tx_id: tx_id.clone(),
            sender: info.sender.clone(),
            pair_with_denom_and_amount: pair_with_denom_and_amount.clone(),
            pool_config,
            lp_token_name,
            lp_token_symbol,
            lp_token_decimal,
            lp_token_marketing,
            slippage_tolerance_bps,
            cross_chain_config,
        };
        let delegate_msg = cosmwasm_std::CosmosMsg::Wasm(cosmwasm_std::WasmMsg::Execute {
            contract_addr: pool_factory.into_string(),
            msg: cosmwasm_std::to_json_binary(&exec)?,
            funds: vec![],
        });
        return Ok(res
            .add_event(tx_event(
                &tx_id,
                info.sender.as_str(),
                euclid::events::TxType::PoolCreation,
            ))
            .add_attribute("action", "pool_creation")
            .add_attribute("tx_id", tx_id)
            .add_attribute("method", "request_pool_creation_delegated")
            .add_attribute("token_1", pair.token_1.to_string())
            .add_attribute("token_2", pair.token_2.to_string())
            .add_submessage(SubMsg::reply_on_success(
                delegate_msg,
                crate::reply::POOL_FACTORY_DELEGATE_REPLY_ID,
            )));
    }

    ensure!(
        !PENDING_POOL_REQUESTS.has(deps.storage, (info.sender.clone(), tx_id.clone())),
        ContractError::TxAlreadyExist {}
    );
    ensure!(
        !PAIR_TO_VLP.has(deps.storage, pair.get_tupple()),
        ContractError::PoolAlreadyExists {}
    );

    // We might get errors in ack if marketing is not valid
    if let Some(marketing) = &lp_token_marketing {
        if let Some(logo) = &marketing.logo {
            ensure!(
                matches!(logo, Logo::Url(_)),
                ContractError::new("Only URL logos are supported")
            );
        }

        if let Some(marketing_address) = &marketing.marketing {
            deps.api.addr_validate(marketing_address)?;
        }
    }

    let lp_token_instantiate_msg = cw20_base::msg::InstantiateMsg {
        name: lp_token_name,
        symbol: lp_token_symbol,
        decimals: lp_token_decimal,
        initial_balances: vec![],
        mint: Some(cw20::MinterResponse {
            minter: env.contract.address.clone().into_string(),
            cap: None,
        }),
        marketing: lp_token_marketing,
    };
    lp_token_instantiate_msg.validate()?;

    let req = PoolCreateRequest {
        tx_id: tx_id.clone(),
        sender: info.sender.clone(),
        pair_info: pair_with_denom_and_amount.clone(),
        lp_token_instantiate_msg,
    };

    PENDING_POOL_REQUESTS.save(deps.storage, (info.sender.clone(), tx_id.clone()), &req)?;

    let chain_type = get_chain_type(deps.as_ref(), &env)?;

    let pool_create_msg = RouterCrossChainExecuteMsg::RequestPoolCreation {
        sender,
        tx_id: tx_id.clone(),
        pair: pair_with_denom_and_amount,
        pool_config,
        slippage_tolerance_bps,
    }
    .to_msg(
        deps,
        &env,
        state.router_contract,
        info.sender.clone(),
        state.chain_uid,
        chain_type,
        cross_chain_config.timeout,
        cross_chain_config.ack_response,
    )?;

    Ok(res
        .add_event(tx_event(
            &tx_id,
            info.sender.as_str(),
            euclid::events::TxType::PoolCreation,
        ))
        .add_attribute("action", "pool_creation")
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "request_pool_creation")
        .add_attribute("token_1", pair.token_1.to_string())
        .add_attribute("token_2", pair.token_2.to_string())
        .add_submessage(pool_create_msg))
}

/// Delegated CP/Stable add-liquidity path used once `pool_factory` is wired.
///
/// Main Factory still owns the escrow custody surface: it validates funds,
/// pulls cw20 transfers, and deposits each non-voucher token to its escrow
/// up-front via `create_escrow_msg`. The pool-state mutation (writing the
/// pending entry, building the outbound packet, ack handling) is delegated
/// to pool_factory's `OnAddLiquidity` handler.
#[allow(clippy::too_many_arguments)]
fn add_liquidity_request_delegated(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    pair_info: PairWithDenomAndAmount,
    slippage_tolerance_bps: u64,
    cross_chain_config: CrossChainConfig,
    tx_id: String,
) -> Result<Response, ContractError> {
    let pair = pair_info.get_pair()?;
    let state = STATE.load(deps.storage)?;
    let sender = CrossChainUser::new(state.chain_uid, info.sender.to_string());

    // Validate funds + collect transfer/deposit messages. Deposits happen
    // up-front so that on failure the refund path can pull from escrow via
    // `ProxyReleaseEscrow` instead of touching factory holdings.
    let mut msgs: Vec<SubMsg> = Vec::new();
    let mut fund_manager = FundManager::new(&info.funds);
    let tokens = pair_info.get_vec_token_info();
    for token in tokens {
        token.token_type.validate(&deps.as_ref())?;
        ensure!(!token.amount.is_zero(), ContractError::ZeroAssetAmount {});

        if token.token_type.is_voucher() {
            continue;
        }

        let escrow_address = TOKEN_TO_ESCROW
            .load(deps.storage, token.token.clone())
            .or(Err(ContractError::EscrowDoesNotExist {}))?;
        let token_allowed_query_msg = euclid::msgs::escrow::QueryMsg::TokenAllowed {
            denom: token.token_type.clone(),
        };
        let token_allowed: AllowedTokenResponse = deps
            .querier
            .query_wasm_smart(escrow_address.clone(), &token_allowed_query_msg)?;
        ensure!(
            token_allowed.allowed,
            ContractError::UnsupportedDenomination {}
        );

        match &token.token_type {
            TokenType::Native { denom, .. } => {
                ensure!(
                    !info.funds.is_empty(),
                    ContractError::InsufficientDeposit {}
                );
                fund_manager.use_fund(token.amount, denom)?;
            }
            TokenType::Smart { .. } => {
                let transfer = token.token_type.create_transfer_msg(
                    token.amount,
                    env.contract.address.clone().to_string(),
                    Some(sender.address.clone()),
                    None,
                )?;
                msgs.push(SubMsg::new(transfer));
            }
            TokenType::Voucher { .. } => return Err(ContractError::UnreachableCode {}),
        }

        // Deposit funds to escrow up-front. This message executes after the
        // optional cw20 TransferFrom above and before the delegate call —
        // the whole tx reverts if any step fails.
        let deposit = token
            .token_type
            .create_escrow_msg(token.amount, escrow_address)?;
        msgs.push(SubMsg::new(deposit));
    }

    ensure!(
        fund_manager.validate_funds_are_empty().is_ok(),
        ContractError::new("Extra funds are not allowed")
    );

    let exec = euclid::msgs::pool_factory::ExecuteMsg::OnAddLiquidity {
        tx_id: tx_id.clone(),
        sender: info.sender.clone(),
        pair_with_denom_and_amount: pair_info,
        slippage_tolerance_bps,
        cross_chain_config,
    };
    let delegate = crate::execute::proxy::pool_factory_execute_msg(deps, &exec)?;

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            info.sender.as_str(),
            euclid::events::TxType::AddLiquidity,
        ))
        .add_attribute("action", "add_liquidity")
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "add_liquidity_request_delegated")
        .add_attribute("token_1", pair.token_1.to_string())
        .add_attribute("token_2", pair.token_2.to_string())
        .add_submessages(msgs)
        .add_submessage(SubMsg::reply_on_success(
            delegate,
            crate::reply::POOL_FACTORY_DELEGATE_REPLY_ID,
        )))
}

// Add liquidity to the pool
pub fn add_liquidity_request(
    deps: &mut DepsMut,
    info: MessageInfo,
    env: Env,
    pair_info: PairWithDenomAndAmount,
    slippage_tolerance_bps: u64,
    cross_chain_config: CrossChainConfig,
) -> Result<Response, ContractError> {
    let pair = pair_info.get_pair()?;

    // Check that slippage tolerance is between 1 and 100
    let state = STATE.load(deps.storage)?;
    let sender = CrossChainUser::new(state.chain_uid.clone(), info.sender.to_string());
    let tx_id = generate_tx(deps, &env, &sender)?;

    ensure!(
        slippage_tolerance_bps >= 1 && slippage_tolerance_bps <= BPS_100_PERCENT,
        ContractError::InvalidSlippageTolerance {}
    );

    // Slice 2: when pool_factory is wired and migration accepted, route
    // add-liquidity end-to-end through pool_factory. Main Factory keeps
    // fund custody up to the escrow deposit, then hands the typed request
    // to pool_factory which returns the outbound packet as `Response::data`;
    // `on_pool_factory_delegate_reply` runs the dispatch and the ack-side
    // proxy calls (`ProxyMintLpToken` on success, `ProxyReleaseEscrow` on
    // failure) follow.
    if pool_factory_is_initialised(deps)? {
        return add_liquidity_request_delegated(
            deps,
            env,
            info,
            pair_info,
            slippage_tolerance_bps,
            cross_chain_config,
            tx_id,
        );
    }

    ensure!(
        !PENDING_ADD_LIQUIDITY.has(deps.storage, (info.sender.clone(), tx_id.clone())),
        ContractError::TxAlreadyExist {}
    );
    ensure!(
        PAIR_TO_VLP.has(deps.storage, pair.get_tupple()),
        ContractError::PoolDoesNotExist {}
    );

    // Prepare msg vector
    let mut msgs: Vec<SubMsg> = Vec::new();

    let mut fund_manager = FundManager::new(&info.funds);
    // Do an early check for tokens escrow so that if it exists, it should allow the denom that we are sending
    let tokens = pair_info.get_vec_token_info();
    for token in tokens {
        // validate token
        token.token_type.validate(&deps.as_ref())?;

        // Ensure liquidity is not zero
        ensure!(!token.amount.is_zero(), ContractError::ZeroAssetAmount {});

        // Vouchers are not escrowed
        if !token.token_type.is_voucher() {
            let escrow_address = TOKEN_TO_ESCROW
                .load(deps.storage, token.token)
                .or(Err(ContractError::EscrowDoesNotExist {}))?;
            let token_allowed_query_msg = euclid::msgs::escrow::QueryMsg::TokenAllowed {
                denom: token.token_type.clone(),
            };
            let token_allowed: AllowedTokenResponse = deps
                .querier
                .query_wasm_smart(escrow_address.clone(), &token_allowed_query_msg)?;

            ensure!(
                token_allowed.allowed,
                ContractError::UnsupportedDenomination {}
            );

            match token.token_type {
                TokenType::Native { denom, .. } => {
                    ensure!(
                        !info.funds.is_empty(),
                        ContractError::InsufficientDeposit {}
                    );
                    // Use funds, if its not present this will throw error.
                    // This will make sure enough funds are provided with the message
                    fund_manager.use_fund(token.amount, &denom)?;
                }
                TokenType::Smart { .. } => {
                    let msg = token.token_type.create_transfer_msg(
                        token.amount,
                        env.contract.address.clone().to_string(),
                        Some(sender.address.clone()),
                        None,
                    )?;
                    msgs.push(SubMsg::new(msg));
                }
                TokenType::Voucher { .. } => return Err(ContractError::UnreachableCode {}),
            }
        }
    }

    ensure!(
        fund_manager.validate_funds_are_empty().is_ok(),
        ContractError::new("Extra funds are not allowed")
    );

    let liquidity_tx_info = AddLiquidityRequest {
        sender: info.sender.to_string(),
        pair_info: pair_info.clone(),
        tx_id: tx_id.clone(),
    };

    PENDING_ADD_LIQUIDITY.save(
        deps.storage,
        (info.sender.clone(), tx_id.clone()),
        &liquidity_tx_info,
    )?;

    let chain_type = get_chain_type(deps.as_ref(), &env)?;

    let add_liq_msg = RouterCrossChainExecuteMsg::AddLiquidity {
        sender,
        slippage_tolerance_bps,
        pair: pair_info,
        tx_id: tx_id.clone(),
    }
    .to_msg(
        deps,
        &env,
        state.router_contract,
        info.sender.clone(),
        state.chain_uid,
        chain_type,
        cross_chain_config.timeout,
        cross_chain_config.ack_response,
    )?;

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            info.sender.as_str(),
            euclid::events::TxType::AddLiquidity,
        ))
        .add_attribute("action", "add_liquidity")
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "add_liquidity_request")
        .add_attribute("token_1", pair.token_1.to_string())
        .add_attribute("token_2", pair.token_2.to_string())
        .add_submessages(msgs)
        .add_submessage(add_liq_msg))
}

// Remove liquidity from the pool
pub fn remove_liquidity_request(
    deps: &mut DepsMut,
    info: MessageInfo,
    env: Env,
    sender: CrossChainUser,
    pair: Pair,
    lp_allocation: Uint256,
    recipient: CrossChainUser,
    cross_chain_config: CrossChainConfig,
) -> Result<Response, ContractError> {
    // Reject mixed-case or empty addresses before mutating state
    sender.validate()?;
    recipient.validate()?;

    let state = STATE.load(deps.storage)?;
    let sender_addr = deps.api.addr_validate(&sender.address)?;

    let tx_id = generate_tx(deps, &env, &sender)?;

    let vlp = PAIR_TO_VLP.load(deps.storage, pair.get_tupple())?;
    let lp_token = VLP_TO_LP_TOKEN.load(deps.storage, vlp)?;

    // Caller (cw20 hook origin) must be the matching LP cw20 contract.
    ensure!(lp_token == info.sender, ContractError::Unauthorized {});

    // Check that the liquidity is greater than 0
    ensure!(!lp_allocation.is_zero(), ContractError::ZeroAssetAmount {});

    // Slice 3: when pool_factory is wired and migration accepted, route
    // remove-liquidity end-to-end through pool_factory. Main Factory still
    // holds the LP cw20 tokens (they arrived via the cw20::Send hook) and
    // drives the burn/refund through `ProxyBurnLpToken` / `ProxyTransferLpToken`
    // after the ack.
    if pool_factory_is_initialised(deps)? {
        let exec = euclid::msgs::pool_factory::ExecuteMsg::OnRemoveLiquidity {
            tx_id: tx_id.clone(),
            sender: sender_addr.clone(),
            pair: pair.clone(),
            lp_allocation,
            lp_token: lp_token.clone(),
            recipient: recipient.clone(),
            cross_chain_config: cross_chain_config.clone(),
        };
        let delegate = crate::execute::proxy::pool_factory_execute_msg(deps, &exec)?;
        return Ok(Response::new()
            .add_event(tx_event(
                &tx_id,
                sender_addr.as_str(),
                euclid::events::TxType::RemoveLiquidity,
            ))
            .add_attribute("action", "remove_liquidity")
            .add_attribute("tx_id", tx_id)
            .add_attribute("method", "remove_liquidity_request_delegated")
            .add_attribute("token_1", pair.token_1.to_string())
            .add_attribute("token_2", pair.token_2.to_string())
            .add_submessage(SubMsg::reply_on_success(
                delegate,
                crate::reply::POOL_FACTORY_DELEGATE_REPLY_ID,
            )));
    }

    ensure!(
        !PENDING_REMOVE_LIQUIDITY.has(deps.storage, (sender_addr.clone(), tx_id.clone())),
        ContractError::TxAlreadyExist {}
    );

    let liquidity_tx_info = RemoveLiquidityRequest {
        sender: sender_addr.to_string(),
        lp_allocation,
        pair: pair.clone(),
        tx_id: tx_id.clone(),
        lp_token,
    };

    PENDING_REMOVE_LIQUIDITY.save(
        deps.storage,
        (sender_addr.clone(), tx_id.clone()),
        &liquidity_tx_info,
    )?;

    let chain_type = get_chain_type(deps.as_ref(), &env)?;
    let token_1 = pair.token_1.to_string();
    let token_2 = pair.token_2.to_string();
    let remove_liq_msg =
        RouterCrossChainExecuteMsg::RemoveLiquidity(RouterCrossChainRemoveLiquidityExecuteMsg {
            sender,
            lp_allocation,
            pair,
            recipient,
            tx_id: tx_id.clone(),
        })
        .to_msg(
            deps,
            &env,
            state.router_contract,
            sender_addr.clone(),
            state.chain_uid,
            chain_type,
            cross_chain_config.timeout,
            cross_chain_config.ack_response,
        )?;

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            sender_addr.as_str(),
            euclid::events::TxType::RemoveLiquidity,
        ))
        .add_attribute("action", "remove_liquidity")
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "remove_liquidity_request")
        .add_attribute("token_1", token_1)
        .add_attribute("token_2", token_2)
        .add_submessage(remove_liq_msg))
}

#[allow(clippy::too_many_arguments)]
pub fn execute_request_concentrated_pool_creation(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    pair_with_denom_and_amount: PairWithDenomAndAmount,
    fee_tier_bps: u64,
    tick_spacing: u64,
    slippage_tolerance_bps: u64,
    initial_tick: Option<i64>,
    cross_chain_config: CrossChainConfig,
) -> Result<Response, ContractError> {
    POSITION_TOKEN_CONTRACT
        .load(deps.storage)
        .map_err(|_| ContractError::new("Position token contract not registered"))?;
    ensure!(
        slippage_tolerance_bps.le(&BPS_100_PERCENT),
        ContractError::InvalidSlippageTolerance {}
    );
    validate_concentrated_fee_and_spacing(fee_tier_bps, tick_spacing)?;

    let pair = pair_with_denom_and_amount.get_pair()?;
    pair.validate()?;

    let state = STATE.load(deps.storage)?;
    let sender = CrossChainUser::new(state.chain_uid.clone(), info.sender.to_string());
    let tx_id = generate_tx(deps, &env, &sender)?;

    let mut fund_manager = FundManager::new(&info.funds);
    let msgs = validate_pool_creation_tokens(
        deps,
        &env,
        &pair_with_denom_and_amount,
        &sender.address,
        &mut fund_manager,
    )?;
    let res = Response::new().add_submessages(msgs);

    let pool_key = euclid::msgs::vlp::base::PoolKey {
        pair: pair.clone(),
        pool_type: euclid::msgs::vlp::base::PoolType::Concentrated {
            fee_tier_bps,
            tick_spacing,
        },
    };

    // Slice 4: when pool_factory is wired, delegate the CLP pool-creation flow.
    // Main factory keeps fund custody (validation/transfers above) and hands
    // the typed request to pool_factory, which records the pending entry,
    // builds the outbound packet via `outbound::request_concentrated_pool_creation`,
    // and returns it as `Response::data` for main factory's
    // `on_pool_factory_delegate_reply` to dispatch. The post-ack carry-overs
    // (position-NFT mint and per-token escrow funding) continue to land via
    // the Slice 4 bridge pattern documented in POOL_FACTORY_REFACTOR_ISSUES.md.
    if crate::execute::proxy::pool_factory_is_initialised(deps)? {
        ensure!(
            fund_manager.validate_funds_are_empty().is_ok(),
            ContractError::new("Extra funds are not allowed")
        );
        let pool_factory_addr = POOL_FACTORY_ADDRESS.load(deps.storage)?;
        let exec = euclid::msgs::pool_factory::ExecuteMsg::OnRequestConcentratedPoolCreation {
            tx_id: tx_id.clone(),
            sender: info.sender.clone(),
            pair_with_denom_and_amount: pair_with_denom_and_amount.clone(),
            pool_key,
            slippage_tolerance_bps,
            initial_tick,
            cross_chain_config,
        };
        let delegate_msg = cosmwasm_std::CosmosMsg::Wasm(cosmwasm_std::WasmMsg::Execute {
            contract_addr: pool_factory_addr.into_string(),
            msg: cosmwasm_std::to_json_binary(&exec)?,
            funds: vec![],
        });
        return Ok(res
            .add_event(tx_event(
                &tx_id,
                info.sender.as_str(),
                euclid::events::TxType::PoolCreation,
            ))
            .add_attribute("action", "concentrated_pool_creation")
            .add_attribute("tx_id", tx_id)
            .add_attribute("method", "request_concentrated_pool_creation_delegated")
            .add_attribute("token_1", pair.token_1.to_string())
            .add_attribute("token_2", pair.token_2.to_string())
            .add_submessage(SubMsg::reply_on_success(
                delegate_msg,
                crate::reply::POOL_FACTORY_DELEGATE_REPLY_ID,
            )));
    }

    ensure!(
        !PENDING_CONCENTRATED_POOL_REQUESTS.has(deps.storage, (info.sender.clone(), tx_id.clone())),
        ContractError::TxAlreadyExist {}
    );
    ensure!(
        !POOL_KEY_TO_VLP.has(deps.storage, pool_key.to_map_key()),
        ContractError::PoolAlreadyExists {}
    );

    let req = ConcentratedPoolCreateRequest {
        tx_id: tx_id.clone(),
        sender: info.sender.clone(),
        pair_info: pair_with_denom_and_amount.clone(),
        pool_key: pool_key.clone(),
    };

    PENDING_CONCENTRATED_POOL_REQUESTS.save(
        deps.storage,
        (info.sender.clone(), tx_id.clone()),
        &req,
    )?;

    let chain_type = get_chain_type(deps.as_ref(), &env)?;

    let pool_create_msg = RouterCrossChainExecuteMsg::RequestConcentratedPoolCreation(
        RouterCrossChainConcentratedRequestPoolCreationExecuteMsg {
            sender,
            tx_id: tx_id.clone(),
            pair: pair_with_denom_and_amount,
            pool_key,
            slippage_tolerance_bps,
            initial_tick,
        },
    )
    .to_msg(
        deps,
        &env,
        state.router_contract,
        info.sender.clone(),
        state.chain_uid,
        chain_type,
        cross_chain_config.timeout,
        cross_chain_config.ack_response,
    )?;

    Ok(res
        .add_event(tx_event(
            &tx_id,
            info.sender.as_str(),
            euclid::events::TxType::PoolCreation,
        ))
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "request_concentrated_pool_creation")
        .add_submessage(pool_create_msg))
}

#[allow(clippy::too_many_arguments)]
pub fn add_concentrated_liquidity_request(
    deps: &mut DepsMut,
    info: MessageInfo,
    env: Env,
    pair_info: PairWithDenomAndAmount,
    pool_key: euclid::msgs::vlp::base::PoolKey,
    lower_tick_index: i64,
    upper_tick_index: i64,
    position_id: Option<Uint128>,
    slippage_tolerance_bps: u64,
    cross_chain_config: CrossChainConfig,
) -> Result<Response, ContractError> {
    ensure!(
        lower_tick_index < upper_tick_index,
        ContractError::new("Invalid tick range")
    );
    ensure!(
        lower_tick_index >= MIN_TICK && upper_tick_index <= MAX_TICK,
        ContractError::new("Tick index out of bounds")
    );
    let tick_spacing = match pool_key.pool_type {
        PoolType::Concentrated { tick_spacing, .. } => tick_spacing,
        _ => return Err(ContractError::new("Pool key must be concentrated")),
    };
    ensure!(tick_spacing > 0, ContractError::new("Invalid tick spacing"));
    let tick_spacing_i64 =
        i64::try_from(tick_spacing).map_err(|_| ContractError::new("Invalid tick spacing"))?;
    ensure!(
        lower_tick_index.rem_euclid(tick_spacing_i64) == 0
            && upper_tick_index.rem_euclid(tick_spacing_i64) == 0,
        ContractError::new("Tick indexes must align with pool tick spacing")
    );
    ensure!(
        slippage_tolerance_bps >= 1 && slippage_tolerance_bps <= BPS_100_PERCENT,
        ContractError::InvalidSlippageTolerance {}
    );
    ensure!(
        pair_info.get_pair()?.get_tupple() == pool_key.pair.get_tupple(),
        ContractError::new("Pair does not match pool key")
    );

    let state = STATE.load(deps.storage)?;
    let sender = CrossChainUser::new(state.chain_uid.clone(), info.sender.to_string());
    let tx_id = generate_tx(deps, &env, &sender)?;

    ensure!(
        !PENDING_CONCENTRATED_ADD_LIQUIDITY.has(deps.storage, (info.sender.clone(), tx_id.clone())),
        ContractError::TxAlreadyExist {}
    );

    let vlp_address = POOL_KEY_TO_VLP
        .load(deps.storage, pool_key.to_map_key())
        .map_err(|_| ContractError::PoolDoesNotExist {})?;
    if let Some(position_id) = position_id {
        let position_token_address = POSITION_TOKEN_CONTRACT
            .load(deps.storage)
            .map_err(|_| ContractError::new("Position token contract not registered"))?;
        let owner_resp: OwnerOfResponse = deps.querier.query_wasm_smart(
            position_token_address.clone(),
            &PositionTokenQueryMsg::OwnerOf {
                token_id: position_id.to_string(),
            },
        )?;
        ensure!(
            owner_resp.owner == info.sender.to_string(),
            ContractError::Unauthorized {}
        );
        let position_info: PositionInfoResponse = deps.querier.query_wasm_smart(
            position_token_address,
            &PositionTokenQueryMsg::PositionInfo {
                token_id: position_id.to_string(),
            },
        )?;
        ensure!(
            position_info.vlp_address == vlp_address,
            ContractError::new("Position does not belong to this pool")
        );
    }

    let mut msgs: Vec<SubMsg> = Vec::new();
    let mut fund_manager = FundManager::new(&info.funds);
    let tokens = pair_info.get_vec_token_info();
    for token in tokens {
        token.token_type.validate(&deps.as_ref())?;
        ensure!(!token.amount.is_zero(), ContractError::ZeroAssetAmount {});
        if !token.token_type.is_voucher() {
            let escrow_address = TOKEN_TO_ESCROW
                .load(deps.storage, token.token)
                .or(Err(ContractError::EscrowDoesNotExist {}))?;
            let token_allowed_query_msg = euclid::msgs::escrow::QueryMsg::TokenAllowed {
                denom: token.token_type.clone(),
            };
            let token_allowed: AllowedTokenResponse = deps
                .querier
                .query_wasm_smart(escrow_address.clone(), &token_allowed_query_msg)?;

            ensure!(
                token_allowed.allowed,
                ContractError::UnsupportedDenomination {}
            );

            match token.token_type {
                TokenType::Native { denom, .. } => {
                    fund_manager.use_fund(token.amount, &denom)?;
                }
                TokenType::Smart { .. } => {
                    let msg = token.token_type.create_transfer_msg(
                        token.amount,
                        env.contract.address.clone().to_string(),
                        Some(sender.address.clone()),
                        None,
                    )?;
                    msgs.push(SubMsg::new(msg));
                }
                TokenType::Voucher { .. } => return Err(ContractError::UnreachableCode {}),
            }
        }
    }
    ensure!(
        fund_manager.validate_funds_are_empty().is_ok(),
        ContractError::new("Extra funds are not allowed")
    );

    let liquidity_tx_info = ConcentratedAddLiquidityRequest {
        sender: info.sender.clone(),
        pair_info: pair_info.clone(),
        tx_id: tx_id.clone(),
        pool_key: pool_key.clone(),
        lower_tick_index,
        upper_tick_index,
        position_id: position_id.map(|v| v.u128()),
    };
    PENDING_CONCENTRATED_ADD_LIQUIDITY.save(
        deps.storage,
        (info.sender.clone(), tx_id.clone()),
        &liquidity_tx_info,
    )?;

    let chain_type = get_chain_type(deps.as_ref(), &env)?;
    let add_liq_msg = RouterCrossChainExecuteMsg::AddConcentratedLiquidity(
        RouterCrossChainConcentratedAddLiquidityExecuteMsg {
            sender,
            pair: pair_info,
            pool_key,
            lower_tick_index,
            upper_tick_index,
            position_id,
            slippage_tolerance_bps,
            tx_id: tx_id.clone(),
        },
    )
    .to_msg(
        deps,
        &env,
        state.router_contract,
        info.sender.clone(),
        state.chain_uid,
        chain_type,
        cross_chain_config.timeout,
        cross_chain_config.ack_response,
    )?;

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            info.sender.as_str(),
            euclid::events::TxType::AddLiquidity,
        ))
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "add_concentrated_liquidity_request")
        .add_submessages(msgs)
        .add_submessage(add_liq_msg))
}

#[allow(clippy::too_many_arguments)]
pub fn remove_concentrated_liquidity_request(
    deps: &mut DepsMut,
    info: MessageInfo,
    env: Env,
    sender: CrossChainUser,
    pool_key: euclid::msgs::vlp::base::PoolKey,
    position_id: Uint128,
    liquidity_delta: Uint128,
    recipient: CrossChainUser,
    cross_chain_config: CrossChainConfig,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    let sender_addr = deps.api.addr_validate(&sender.address)?;
    let tx_id = generate_tx(deps, &env, &sender)?;

    ensure!(
        !PENDING_CONCENTRATED_REMOVE_LIQUIDITY
            .has(deps.storage, (sender_addr.clone(), tx_id.clone())),
        ContractError::TxAlreadyExist {}
    );
    ensure!(
        !liquidity_delta.is_zero(),
        ContractError::ZeroAssetAmount {}
    );

    let vlp_address = POOL_KEY_TO_VLP
        .load(deps.storage, pool_key.to_map_key())
        .map_err(|_| ContractError::PoolDoesNotExist {})?;

    let position_token_address = POSITION_TOKEN_CONTRACT
        .load(deps.storage)
        .map_err(|_| ContractError::new("Position token contract not registered"))?;
    let owner_resp: OwnerOfResponse = deps.querier.query_wasm_smart(
        position_token_address.clone(),
        &PositionTokenQueryMsg::OwnerOf {
            token_id: position_id.to_string(),
        },
    )?;
    ensure!(
        owner_resp.owner == info.sender.to_string(),
        ContractError::Unauthorized {}
    );

    let position_info: PositionInfoResponse = deps.querier.query_wasm_smart(
        position_token_address.clone(),
        &PositionTokenQueryMsg::PositionInfo {
            token_id: position_id.to_string(),
        },
    )?;
    ensure!(
        position_info.vlp_address == vlp_address,
        ContractError::new("Position does not belong to this pool")
    );
    ensure!(
        position_info.liquidity.ge(&liquidity_delta),
        ContractError::InsufficientFunds {}
    );

    let liquidity_delta_signed = Int256::from(liquidity_delta);

    // Lets update liquidity of the position before removing liquidity so next calls will error if the position is not enough liquidity
    let update_position_msg = msgs::position_token::ExecuteMsg::UpdatePosition {
        token_id: position_id,
        liquidity_change: -liquidity_delta_signed,
    };

    let update_position_msg = SubMsg::new(update_position_msg.to_msg(position_token_address)?);

    let req = ConcentratedRemoveLiquidityRequest {
        tx_id: tx_id.clone(),
        sender: sender_addr.clone(),
        pool_key: pool_key.clone(),
        position_id: position_id.u128(),
        liquidity_delta,
    };

    PENDING_CONCENTRATED_REMOVE_LIQUIDITY.save(
        deps.storage,
        (sender_addr.clone(), tx_id.clone()),
        &req,
    )?;

    let chain_type = get_chain_type(deps.as_ref(), &env)?;
    let remove_msg = RouterCrossChainExecuteMsg::RemoveConcentratedLiquidity(
        RouterCrossChainConcentratedRemoveLiquidityExecuteMsg {
            sender,
            pool_key,
            position_id,
            liquidity_delta,
            recipient,
            tx_id: tx_id.clone(),
        },
    )
    .to_msg(
        deps,
        &env,
        state.router_contract,
        sender_addr.clone(),
        state.chain_uid,
        chain_type,
        cross_chain_config.timeout,
        cross_chain_config.ack_response,
    )?;

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            sender_addr.as_str(),
            euclid::events::TxType::RemoveLiquidity,
        ))
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "remove_concentrated_liquidity_request")
        .add_submessage(update_position_msg)
        .add_submessage(remove_msg))
}

#[allow(clippy::too_many_arguments)]
pub fn collect_concentrated_fees_request(
    deps: &mut DepsMut,
    info: MessageInfo,
    env: Env,
    pool_key: euclid::msgs::vlp::base::PoolKey,
    position_id: Uint128,
    recipient: CrossChainUser,
    cross_chain_config: CrossChainConfig,
) -> Result<Response, ContractError> {
    recipient.validate()?;

    let state = STATE.load(deps.storage)?;
    let sender = CrossChainUser::new(state.chain_uid.clone(), info.sender.to_string());
    let sender_addr = deps.api.addr_validate(&sender.address)?;
    let tx_id = generate_tx(deps, &env, &sender)?;

    ensure!(
        !PENDING_CONCENTRATED_COLLECT_FEES.has(deps.storage, (sender_addr.clone(), tx_id.clone())),
        ContractError::TxAlreadyExist {}
    );

    let vlp_address = POOL_KEY_TO_VLP
        .load(deps.storage, pool_key.to_map_key())
        .map_err(|_| ContractError::PoolDoesNotExist {})?;

    let position_token_address = POSITION_TOKEN_CONTRACT
        .load(deps.storage)
        .map_err(|_| ContractError::new("Position token contract not registered"))?;
    let owner_resp: OwnerOfResponse = deps.querier.query_wasm_smart(
        position_token_address.clone(),
        &PositionTokenQueryMsg::OwnerOf {
            token_id: position_id.to_string(),
        },
    )?;
    ensure!(
        owner_resp.owner == info.sender.to_string(),
        ContractError::Unauthorized {}
    );
    let position_info: PositionInfoResponse = deps.querier.query_wasm_smart(
        position_token_address,
        &PositionTokenQueryMsg::PositionInfo {
            token_id: position_id.to_string(),
        },
    )?;
    ensure!(
        position_info.vlp_address == vlp_address,
        ContractError::new("Position does not belong to this pool")
    );
    let req = ConcentratedCollectFeesRequest {
        tx_id: tx_id.clone(),
        sender: sender_addr.clone(),
        pool_key: pool_key.clone(),
        position_id: position_id.u128(),
        recipient: recipient.clone(),
    };
    PENDING_CONCENTRATED_COLLECT_FEES.save(
        deps.storage,
        (sender_addr.clone(), tx_id.clone()),
        &req,
    )?;

    let chain_type = get_chain_type(deps.as_ref(), &env)?;
    let collect_msg = RouterCrossChainExecuteMsg::CollectConcentratedFees(
        RouterCrossChainConcentratedCollectFeesExecuteMsg {
            sender,
            pool_key,
            position_id,
            recipient,
            tx_id: tx_id.clone(),
        },
    )
    .to_msg(
        deps,
        &env,
        state.router_contract,
        sender_addr.clone(),
        state.chain_uid,
        chain_type,
        cross_chain_config.timeout,
        cross_chain_config.ack_response,
    )?;

    Ok(Response::new()
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "collect_concentrated_fees_request")
        .add_submessage(collect_msg))
}

#[allow(clippy::too_many_arguments)]
pub fn collect_concentrated_protocol_fees_request(
    deps: &mut DepsMut,
    info: MessageInfo,
    env: Env,
    pool_key: euclid::msgs::vlp::base::PoolKey,
    recipient: CrossChainUser,
    amount_0_requested: Uint128,
    amount_1_requested: Uint128,
    cross_chain_config: CrossChainConfig,
) -> Result<Response, ContractError> {
    recipient.validate()?;
    ensure!(
        !(amount_0_requested.is_zero() && amount_1_requested.is_zero()),
        ContractError::ZeroAssetAmount {}
    );

    let state = STATE.load(deps.storage)?;
    let admins = ADMIN.load(deps.storage)?;
    ensure!(
        info.sender == admins.fee_admin,
        ContractError::Unauthorized {}
    );

    let sender = CrossChainUser::new(state.chain_uid.clone(), info.sender.to_string());
    let sender_addr = deps.api.addr_validate(&sender.address)?;
    let tx_id = generate_tx(deps, &env, &sender)?;

    ensure!(
        !PENDING_CONCENTRATED_COLLECT_PROTOCOL_FEES
            .has(deps.storage, (sender_addr.clone(), tx_id.clone())),
        ContractError::TxAlreadyExist {}
    );
    ensure!(
        POOL_KEY_TO_VLP.has(deps.storage, pool_key.to_map_key()),
        ContractError::PoolDoesNotExist {}
    );

    let req = ConcentratedCollectProtocolFeesRequest {
        tx_id: tx_id.clone(),
        sender: sender_addr.clone(),
        pool_key: pool_key.clone(),
        recipient: recipient.clone(),
        amount_0_requested,
        amount_1_requested,
    };
    PENDING_CONCENTRATED_COLLECT_PROTOCOL_FEES.save(
        deps.storage,
        (sender_addr.clone(), tx_id.clone()),
        &req,
    )?;

    let chain_type = get_chain_type(deps.as_ref(), &env)?;
    let collect_msg = RouterCrossChainExecuteMsg::CollectConcentratedProtocolFees(
        RouterCrossChainConcentratedCollectProtocolFeesExecuteMsg {
            sender,
            pool_key,
            recipient,
            amount_0_requested,
            amount_1_requested,
            tx_id: tx_id.clone(),
        },
    )
    .to_msg(
        deps,
        &env,
        state.router_contract,
        sender_addr.clone(),
        state.chain_uid,
        chain_type,
        cross_chain_config.timeout,
        cross_chain_config.ack_response,
    )?;

    Ok(Response::new()
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "collect_concentrated_protocol_fees_request")
        .add_submessage(collect_msg))
}

// Single-sided add liquidity: user deposits one token, the hub atomically swaps
// a backend-computed portion through the target VLP and adds liquidity on the
// same VLP, all in one IBC roundtrip.
#[allow(clippy::too_many_arguments)]
pub fn execute_single_sided_add_liquidity_request(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    asset_in: TokenWithDenom,
    amount_in: Uint256,
    pair: Pair,
    swap_amount: Uint256,
    swap_route: Vec<NextSwapPair>,
    min_lp_out: Uint256,
    partner_fee: Option<PartnerFee>,
    cross_chain_config: CrossChainConfig,
) -> Result<Response, ContractError> {
    asset_in.token.validate()?;
    asset_in.token_type.validate(&deps.as_ref())?;
    pair.validate()?;

    // asset_in must be one side of the target pair; the other side is the
    // swap output and the matching liquidity leg.
    ensure!(
        asset_in.token == pair.token_1 || asset_in.token == pair.token_2,
        ContractError::new("asset_in must be one of the pair tokens")
    );
    let asset_out = pair.get_other_token(asset_in.token.clone());

    // Partner fee: same model as execute_swap_request.
    // The full `amount_in` from the user is split into:
    //   - partner_fee_amount: retained at the factory until ack resolves
    //   - amount_in (rebound below): the portion that crosses IBC
    let partner_fee_bps = partner_fee
        .as_ref()
        .map(|fee| fee.partner_fee_bps)
        .unwrap_or(0);
    ensure!(
        partner_fee_bps <= MAX_PARTNER_FEE_BPS,
        ContractError::InvalidPartnerFee {}
    );
    let partner_fee_amount = amount_in.checked_mul_ceil(Decimal::bps(partner_fee_bps))?;
    let amount_in = amount_in.checked_sub(partner_fee_amount)?;

    ensure!(!amount_in.is_zero(), ContractError::ZeroAssetAmount {});
    ensure!(!swap_amount.is_zero(), ContractError::ZeroAssetAmount {});
    ensure!(
        swap_amount < amount_in,
        ContractError::new("swap_amount must be < amount_in")
    );
    ensure!(!min_lp_out.is_zero(), ContractError::ZeroAssetAmount {});

    // Single-hop in v1 — kept Vec for forward-compat.
    ensure!(
        swap_route.len() == 1,
        ContractError::new("swap_route must contain exactly one hop in v1")
    );
    let hop = &swap_route[0];
    ensure!(
        hop.token_in == asset_in.token,
        ContractError::new("swap_route first hop token_in must match asset_in")
    );
    ensure!(
        hop.token_out == asset_out,
        ContractError::new("swap_route last hop token_out must match the other pair token")
    );

    let state = STATE.load(deps.storage)?;
    let sender = CrossChainUser::new(state.chain_uid.clone(), info.sender.to_string());
    let tx_id = generate_tx(deps, &env, &sender)?;

    ensure!(
        !PENDING_SINGLE_SIDED_LIQUIDITY.has(deps.storage, (info.sender.clone(), tx_id.clone())),
        ContractError::TxAlreadyExist {}
    );

    // Target VLP must already exist — fail fast before paying IBC roundtrip.
    ensure!(
        PAIR_TO_VLP.has(deps.storage, pair.get_tupple()),
        ContractError::PoolDoesNotExist {}
    );

    // Escrow must exist and allow this denom.
    let escrow_address = TOKEN_TO_ESCROW
        .load(deps.storage, asset_in.token.clone())
        .or(Err(ContractError::EscrowDoesNotExist {}))?;
    let token_allowed: AllowedTokenResponse = deps.querier.query_wasm_smart(
        escrow_address,
        &euclid::msgs::escrow::QueryMsg::TokenAllowed {
            denom: asset_in.token_type.clone(),
        },
    )?;
    ensure!(
        token_allowed.allowed,
        ContractError::UnsupportedDenomination {}
    );

    // Handle funds. Native: bank funds; Smart: TransferFrom into the factory;
    // Voucher: structurally impossible from a remote-chain factory.
    // Funds must cover the FULL deposit (amount_in + partner_fee_amount).
    let full_amount = amount_in.checked_add(partner_fee_amount)?;
    let mut fund_manager = FundManager::new(&info.funds);
    let mut pre_submsgs: Vec<SubMsg> = Vec::new();
    match &asset_in.token_type {
        TokenType::Native { denom, .. } => {
            fund_manager.use_fund(full_amount, denom)?;
        }
        TokenType::Smart { .. } => {
            let transfer_msg = asset_in.token_type.create_transfer_msg(
                full_amount,
                env.contract.address.to_string(),
                Some(sender.address.clone()),
                None,
            )?;
            pre_submsgs.push(SubMsg::new(transfer_msg));
        }
        TokenType::Voucher { .. } => return Err(ContractError::UnreachableCode {}),
    }
    ensure!(
        fund_manager.validate_funds_are_empty().is_ok(),
        ContractError::new("Extra funds are not allowed")
    );

    // Resolve partner-fee recipient: default to sender if not specified.
    let partner_fee_recipient = partner_fee
        .as_ref()
        .map(|fee| deps.api.addr_validate(&fee.recipient))
        .transpose()?
        .unwrap_or(info.sender.clone());

    let pending = SingleSidedLiquidityRequest {
        sender: info.sender.to_string(),
        tx_id: tx_id.clone(),
        asset_in: asset_in.clone(),
        amount_in,
        partner_fee_amount,
        partner_fee_recipient: partner_fee_recipient.clone(),
    };
    PENDING_SINGLE_SIDED_LIQUIDITY.save(
        deps.storage,
        (info.sender.clone(), tx_id.clone()),
        &pending,
    )?;

    let chain_type = get_chain_type(deps.as_ref(), &env)?;

    let ibc_msg = RouterCrossChainExecuteMsg::SingleSidedAddLiquidity(
        RouterCrossChainSingleSidedAddLiquidityMsg {
            sender,
            asset_in: asset_in.clone(),
            amount_in,
            swap_amount,
            pair: pair.clone(),
            swaps: swap_route,
            min_lp_out,
            partner_fee_amount,
            partner_fee_recipient: CrossChainUser::new(
                state.chain_uid.clone(),
                partner_fee_recipient.to_string(),
            ),
            tx_id: tx_id.clone(),
        },
    )
    .to_msg(
        deps,
        &env,
        state.router_contract,
        info.sender.clone(),
        state.chain_uid,
        chain_type,
        cross_chain_config.timeout,
        cross_chain_config.ack_response,
    )?;

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            info.sender.as_str(),
            TxType::SingleSidedAddLiquidity,
        ))
        .add_attribute("action", "single_sided_add_liquidity")
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "execute_single_sided_add_liquidity_request")
        .add_attribute("asset_in", asset_in.token.to_string())
        .add_attribute("asset_out", asset_out.to_string())
        .add_attribute("amount_in", amount_in)
        .add_attribute("swap_amount", swap_amount)
        .add_submessages(pre_submsgs)
        .add_submessage(ibc_msg))
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{
        testing::{message_info, mock_dependencies, mock_env},
        Uint128, Uint256,
    };
    use euclid::{
        error::ContractError,
        msgs::factory::ExecuteMsg,
        token::{Token, TokenType},
    };

    use crate::{
        contract::execute,
        testing::helpers::{
            default_cross_chain_config, init, seed_escrow, seed_vlp, set_escrow_token_allowed,
        },
    };

    // -----------------------------------------------------------------------
    // Execute: AddLiquidity – PoolDoesNotExist
    // -----------------------------------------------------------------------

    #[test]
    fn test_add_liquidity_pool_does_not_exist() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        deps.querier
            .bank
            .update_balance("any", vec![cosmwasm_std::coin(1_000_000, "uusdc")]);
        deps.querier
            .bank
            .update_balance("any2", vec![cosmwasm_std::coin(1_000_000, "ueth")]);

        let user = deps.api.addr_make("user");
        let info = message_info(
            &user,
            &[
                cosmwasm_std::coin(100, "uusdc"),
                cosmwasm_std::coin(100, "ueth"),
            ],
        );

        let pair_info = euclid::token::PairWithDenomAndAmount {
            token_1: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("eth".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "ueth".to_string(),
                    decimals: None,
                },
                amount: Uint256::from(100u128),
            },
            token_2: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("usdc".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uusdc".to_string(),
                    decimals: None,
                },
                amount: Uint256::from(100u128),
            },
        };
        let msg = ExecuteMsg::AddLiquidity {
            pair_with_denom_and_amount: pair_info,
            slippage_tolerance_bps: 50,
            cross_chain_config: default_cross_chain_config(),
        };
        let res = execute(deps.as_mut(), mock_env(), info, msg);
        assert_eq!(res.unwrap_err(), ContractError::PoolDoesNotExist {});
    }

    // -----------------------------------------------------------------------
    // Execute: AddLiquidity – invalid slippage (zero)
    // -----------------------------------------------------------------------

    #[test]
    fn test_add_liquidity_zero_slippage_fails() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");

        deps.querier
            .bank
            .update_balance("any", vec![cosmwasm_std::coin(1_000_000, "uusdc")]);
        deps.querier
            .bank
            .update_balance("any2", vec![cosmwasm_std::coin(1_000_000, "ueth")]);

        set_escrow_token_allowed(&mut deps, true);
        seed_escrow(&mut deps, "eth", "escrow_eth");
        seed_escrow(&mut deps, "usdc", "escrow_usdc");

        let user = deps.api.addr_make("user");
        let info = message_info(
            &user,
            &[
                cosmwasm_std::coin(100, "uusdc"),
                cosmwasm_std::coin(100, "ueth"),
            ],
        );
        let pair_info = euclid::token::PairWithDenomAndAmount {
            token_1: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("eth".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "ueth".to_string(),
                    decimals: None,
                },
                amount: Uint256::from(100u128),
            },
            token_2: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("usdc".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uusdc".to_string(),
                    decimals: None,
                },
                amount: Uint256::from(100u128),
            },
        };
        let msg = ExecuteMsg::AddLiquidity {
            pair_with_denom_and_amount: pair_info,
            slippage_tolerance_bps: 0,
            cross_chain_config: default_cross_chain_config(),
        };
        let res = execute(deps.as_mut(), mock_env(), info, msg);
        assert_eq!(res.unwrap_err(), ContractError::InvalidSlippageTolerance {});
    }

    // -----------------------------------------------------------------------
    // Execute: AddLiquidity – zero token amount
    // -----------------------------------------------------------------------

    #[test]
    fn test_add_liquidity_zero_amount_fails() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");
        seed_escrow(&mut deps, "eth", "escrow_eth");
        seed_escrow(&mut deps, "usdc", "escrow_usdc");

        deps.querier
            .bank
            .update_balance("any", vec![cosmwasm_std::coin(1_000_000, "uusdc")]);
        deps.querier
            .bank
            .update_balance("any2", vec![cosmwasm_std::coin(1_000_000, "ueth")]);

        set_escrow_token_allowed(&mut deps, true);

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[]);

        let pair_info = euclid::token::PairWithDenomAndAmount {
            token_1: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("eth".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "ueth".to_string(),
                    decimals: None,
                },
                amount: Uint256::zero(),
            },
            token_2: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("usdc".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uusdc".to_string(),
                    decimals: None,
                },
                amount: Uint256::from(100u128),
            },
        };
        let msg = ExecuteMsg::AddLiquidity {
            pair_with_denom_and_amount: pair_info,
            slippage_tolerance_bps: 50,
            cross_chain_config: default_cross_chain_config(),
        };
        let res = execute(deps.as_mut(), mock_env(), info, msg);
        assert_eq!(res.unwrap_err(), ContractError::ZeroAssetAmount {});
    }

    // -----------------------------------------------------------------------
    // Execute: RequestPoolCreation – slippage tolerance exceeds 100%
    // -----------------------------------------------------------------------

    #[test]
    fn test_request_pool_creation_slippage_too_high_fails() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        deps.querier
            .bank
            .update_balance("any", vec![cosmwasm_std::coin(1_000_000, "uusdc")]);
        deps.querier
            .bank
            .update_balance("any2", vec![cosmwasm_std::coin(1_000_000, "ueth")]);

        let user = deps.api.addr_make("user");
        let info = message_info(
            &user,
            &[
                cosmwasm_std::coin(100, "uusdc"),
                cosmwasm_std::coin(100, "ueth"),
            ],
        );

        let pair_info = euclid::token::PairWithDenomAndAmount {
            token_1: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("eth".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "ueth".to_string(),
                    decimals: None,
                },
                amount: Uint256::from(100u128),
            },
            token_2: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("usdc".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uusdc".to_string(),
                    decimals: None,
                },
                amount: Uint256::from(100u128),
            },
        };

        let msg = ExecuteMsg::RequestPoolCreation {
            pair_with_denom_and_amount: pair_info,
            pool_config: euclid::msgs::vlp::base::PoolConfig::ConstantProduct {},
            lp_token_name: "LP Token".to_string(),
            lp_token_symbol: "LPT".to_string(),
            lp_token_decimal: 6,
            slippage_tolerance_bps: 10_001,
            lp_token_marketing: None,
            cross_chain_config: default_cross_chain_config(),
        };
        let res = execute(deps.as_mut(), mock_env(), info, msg);
        assert_eq!(res.unwrap_err(), ContractError::InvalidSlippageTolerance {});
    }

    // -----------------------------------------------------------------------
    // Execute: RequestPoolCreation – pool already exists
    // -----------------------------------------------------------------------

    #[test]
    fn test_request_pool_creation_pool_already_exists_fails() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_vlp(&mut deps, "eth", "usdc", "existing_vlp");
        seed_escrow(&mut deps, "eth", "escrow_eth");
        seed_escrow(&mut deps, "usdc", "escrow_usdc");

        deps.querier
            .bank
            .update_balance("any", vec![cosmwasm_std::coin(1_000_000, "uusdc")]);
        deps.querier
            .bank
            .update_balance("any2", vec![cosmwasm_std::coin(1_000_000, "ueth")]);

        set_escrow_token_allowed(&mut deps, true);

        let user = deps.api.addr_make("user");
        let info = message_info(
            &user,
            &[
                cosmwasm_std::coin(100, "uusdc"),
                cosmwasm_std::coin(100, "ueth"),
            ],
        );

        let pair_info = euclid::token::PairWithDenomAndAmount {
            token_1: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("eth".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "ueth".to_string(),
                    decimals: None,
                },
                amount: Uint256::from(100u128),
            },
            token_2: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("usdc".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uusdc".to_string(),
                    decimals: None,
                },
                amount: Uint256::from(100u128),
            },
        };

        let msg = ExecuteMsg::RequestPoolCreation {
            pair_with_denom_and_amount: pair_info,
            pool_config: euclid::msgs::vlp::base::PoolConfig::ConstantProduct {},
            lp_token_name: "LP Token".to_string(),
            lp_token_symbol: "LPT".to_string(),
            lp_token_decimal: 6,
            slippage_tolerance_bps: 50,
            lp_token_marketing: None,
            cross_chain_config: default_cross_chain_config(),
        };
        let res = execute(deps.as_mut(), mock_env(), info, msg);
        assert_eq!(res.unwrap_err(), ContractError::PoolAlreadyExists {});
    }

    // -----------------------------------------------------------------------
    // Execute: RequestPoolCreation – same token on both sides
    // -----------------------------------------------------------------------

    #[test]
    fn test_request_pool_creation_same_token_fails() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        deps.querier
            .bank
            .update_balance("any", vec![cosmwasm_std::coin(1_000_000, "uusdc")]);

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[cosmwasm_std::coin(100, "uusdc")]);

        let pair_info = euclid::token::PairWithDenomAndAmount {
            token_1: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("usdc".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uusdc".to_string(),
                    decimals: None,
                },
                amount: Uint256::from(100u128),
            },
            token_2: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("usdc".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uusdc".to_string(),
                    decimals: None,
                },
                amount: Uint256::from(100u128),
            },
        };

        let msg = ExecuteMsg::RequestPoolCreation {
            pair_with_denom_and_amount: pair_info,
            pool_config: euclid::msgs::vlp::base::PoolConfig::ConstantProduct {},
            lp_token_name: "LP Token".to_string(),
            lp_token_symbol: "LPT".to_string(),
            lp_token_decimal: 6,
            slippage_tolerance_bps: 50,
            lp_token_marketing: None,
            cross_chain_config: default_cross_chain_config(),
        };
        let res = execute(deps.as_mut(), mock_env(), info, msg);
        assert!(res.is_err());
    }

    // -----------------------------------------------------------------------
    // Execute: RequestPoolCreation – both tokens new (no pre-existing escrow)
    // -----------------------------------------------------------------------

    // Reorg-replay safety: regenerating the same tx_id on RequestPoolCreation
    // must hit the PENDING_POOL_REQUESTS TxAlreadyExist guard before the
    // PoolAlreadyExists check.
    #[test]
    fn test_request_pool_creation_duplicate_tx_id_rejected() {
        use euclid::utils::tx::TX_NONCES;

        let mut deps = mock_dependencies();
        init(&mut deps);
        // One token must already be registered (escrow exists) so the second
        // token is treated as the new one; the pair itself is NOT seeded into
        // PAIR_TO_VLP, so the first call succeeds.
        seed_escrow(&mut deps, "usdc", "escrow_usdc");
        set_escrow_token_allowed(&mut deps, true);

        deps.querier
            .bank
            .update_balance("any", vec![cosmwasm_std::coin(1_000_000, "uusdc")]);
        deps.querier
            .bank
            .update_balance("any2", vec![cosmwasm_std::coin(1_000_000, "ueth")]);

        let user = deps.api.addr_make("user");
        let make_msg = || ExecuteMsg::RequestPoolCreation {
            pair_with_denom_and_amount: euclid::token::PairWithDenomAndAmount {
                token_1: euclid::token::TokenWithDenomAndAmount {
                    token: Token::create("eth".to_string()).unwrap(),
                    token_type: TokenType::Native {
                        denom: "ueth".to_string(),
                        decimals: Some(6),
                    },
                    amount: Uint256::from(100u128),
                },
                token_2: euclid::token::TokenWithDenomAndAmount {
                    token: Token::create("usdc".to_string()).unwrap(),
                    token_type: TokenType::Native {
                        denom: "uusdc".to_string(),
                        decimals: Some(6),
                    },
                    amount: Uint256::from(100u128),
                },
            },
            pool_config: euclid::msgs::vlp::base::PoolConfig::ConstantProduct {},
            lp_token_name: "LP Token".to_string(),
            lp_token_symbol: "LPT".to_string(),
            lp_token_decimal: 6,
            slippage_tolerance_bps: 50,
            lp_token_marketing: None,
            cross_chain_config: default_cross_chain_config(),
        };
        let funds = [
            cosmwasm_std::coin(100, "uusdc"),
            cosmwasm_std::coin(100, "ueth"),
        ];

        execute(
            deps.as_mut(),
            mock_env(),
            message_info(&user, &funds),
            make_msg(),
        )
        .unwrap();

        TX_NONCES
            .save(deps.as_mut().storage, format!("testchain:{user}"), &0u128)
            .unwrap();

        let err = execute(
            deps.as_mut(),
            mock_env(),
            message_info(&user, &funds),
            make_msg(),
        )
        .unwrap_err();
        assert_eq!(err, ContractError::TxAlreadyExist {});
    }

    // Reorg-replay safety for AddLiquidity: the PENDING_ADD_LIQUIDITY guard
    // must reject a regenerated tx_id even though the pool exists.
    #[test]
    fn test_add_liquidity_duplicate_tx_id_rejected() {
        use euclid::utils::tx::TX_NONCES;

        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");
        seed_escrow(&mut deps, "eth", "escrow_eth");
        seed_escrow(&mut deps, "usdc", "escrow_usdc");
        set_escrow_token_allowed(&mut deps, true);

        deps.querier
            .bank
            .update_balance("any", vec![cosmwasm_std::coin(1_000_000, "uusdc")]);
        deps.querier
            .bank
            .update_balance("any2", vec![cosmwasm_std::coin(1_000_000, "ueth")]);

        let user = deps.api.addr_make("user");
        let make_msg = || ExecuteMsg::AddLiquidity {
            pair_with_denom_and_amount: euclid::token::PairWithDenomAndAmount {
                token_1: euclid::token::TokenWithDenomAndAmount {
                    token: Token::create("eth".to_string()).unwrap(),
                    token_type: TokenType::Native {
                        denom: "ueth".to_string(),
                        decimals: None,
                    },
                    amount: Uint256::from(100u128),
                },
                token_2: euclid::token::TokenWithDenomAndAmount {
                    token: Token::create("usdc".to_string()).unwrap(),
                    token_type: TokenType::Native {
                        denom: "uusdc".to_string(),
                        decimals: None,
                    },
                    amount: Uint256::from(100u128),
                },
            },
            slippage_tolerance_bps: 50,
            cross_chain_config: default_cross_chain_config(),
        };
        let funds = [
            cosmwasm_std::coin(100, "uusdc"),
            cosmwasm_std::coin(100, "ueth"),
        ];

        execute(
            deps.as_mut(),
            mock_env(),
            message_info(&user, &funds),
            make_msg(),
        )
        .unwrap();

        TX_NONCES
            .save(deps.as_mut().storage, format!("testchain:{user}"), &0u128)
            .unwrap();

        let err = execute(
            deps.as_mut(),
            mock_env(),
            message_info(&user, &funds),
            make_msg(),
        )
        .unwrap_err();
        assert_eq!(err, ContractError::TxAlreadyExist {});
    }

    // Reorg-replay safety for RemoveLiquidity: the PENDING_REMOVE_LIQUIDITY
    // guard must reject a regenerated tx_id. RemoveLiquidity is reached via
    // a CW20 hook, so this test calls `remove_liquidity_request` directly
    // (the same path the CW20 receive handler invokes).
    #[test]
    fn test_remove_liquidity_duplicate_tx_id_rejected() {
        use crate::execute::pool::remove_liquidity_request;
        use crate::state::VLP_TO_LP_TOKEN;
        use euclid::utils::tx::TX_NONCES;

        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");
        seed_escrow(&mut deps, "eth", "escrow_eth");
        seed_escrow(&mut deps, "usdc", "escrow_usdc");
        let lp_token = cosmwasm_std::Addr::unchecked("lp_addr");
        VLP_TO_LP_TOKEN
            .save(deps.as_mut().storage, "vlp_addr".to_string(), &lp_token)
            .unwrap();

        let user_addr = deps.api.addr_make("user");
        let sender = euclid::cross_chain_user::CrossChainUser::new(
            euclid::chain::ChainUid::create(crate::testing::helpers::TEST_CHAIN_UID.to_string())
                .unwrap(),
            user_addr.to_string(),
        );
        let pair = euclid::token::Pair::new(
            Token::create("eth".to_string()).unwrap(),
            Token::create("usdc".to_string()).unwrap(),
        )
        .unwrap();
        let lp_info = message_info(&lp_token, &[]);

        remove_liquidity_request(
            &mut deps.as_mut(),
            lp_info.clone(),
            mock_env(),
            sender.clone(),
            pair.clone(),
            Uint256::from(10u128),
            sender.clone(),
            default_cross_chain_config(),
        )
        .unwrap();

        TX_NONCES
            .save(
                deps.as_mut().storage,
                format!("testchain:{user_addr}"),
                &0u128,
            )
            .unwrap();

        let err = remove_liquidity_request(
            &mut deps.as_mut(),
            lp_info,
            mock_env(),
            sender.clone(),
            pair,
            Uint256::from(10u128),
            sender,
            default_cross_chain_config(),
        )
        .unwrap_err();
        assert_eq!(err, ContractError::TxAlreadyExist {});
    }

    #[test]
    fn test_request_pool_creation_both_tokens_new_fails() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        deps.querier
            .bank
            .update_balance("any", vec![cosmwasm_std::coin(1_000_000, "uaaa")]);
        deps.querier
            .bank
            .update_balance("any2", vec![cosmwasm_std::coin(1_000_000, "ubbb")]);

        let user = deps.api.addr_make("user");
        let info = message_info(
            &user,
            &[
                cosmwasm_std::coin(100, "uaaa"),
                cosmwasm_std::coin(100, "ubbb"),
            ],
        );

        let pair_info = euclid::token::PairWithDenomAndAmount {
            token_1: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("aaa".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uaaa".to_string(),
                    decimals: None,
                },
                amount: Uint256::from(100u128),
            },
            token_2: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("bbb".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "ubbb".to_string(),
                    decimals: None,
                },
                amount: Uint256::from(100u128),
            },
        };

        let msg = ExecuteMsg::RequestPoolCreation {
            pair_with_denom_and_amount: pair_info,
            pool_config: euclid::msgs::vlp::base::PoolConfig::ConstantProduct {},
            lp_token_name: "LP Token".to_string(),
            lp_token_symbol: "LPT".to_string(),
            lp_token_decimal: 6,
            slippage_tolerance_bps: 50,
            lp_token_marketing: None,
            cross_chain_config: default_cross_chain_config(),
        };
        let res = execute(deps.as_mut(), mock_env(), info, msg);
        assert!(matches!(res.unwrap_err(), ContractError::Generic { .. }));
    }

    // -----------------------------------------------------------------------
    // Execute: AddSingleSidedLiquidity tests
    // -----------------------------------------------------------------------

    use cosmwasm_std::{to_json_binary, ContractResult, SystemResult, WasmQuery};
    use euclid::{
        msgs::escrow::AllowedTokenResponse,
        swap::NextSwapPair,
        token::{Pair, TokenWithDenom},
    };

    use crate::state::PENDING_SINGLE_SIDED_LIQUIDITY;
    use crate::testing::helpers::get_attribute;

    fn native_token_with_denom(token_id: &str, denom: &str) -> TokenWithDenom {
        TokenWithDenom {
            token: Token::create(token_id.to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: denom.to_string(),
                decimals: None,
            },
        }
    }

    fn single_hop(token_in: &str, token_out: &str) -> Vec<NextSwapPair> {
        vec![NextSwapPair {
            token_in: Token::create(token_in.to_string()).unwrap(),
            token_out: Token::create(token_out.to_string()).unwrap(),
            pool_key: None,
            test_fail: None,
        }]
    }

    fn ss_default_msg() -> ExecuteMsg {
        ExecuteMsg::AddSingleSidedLiquidity {
            asset_in: native_token_with_denom("eth", "ueth"),
            amount_in: Uint256::from(1000u128),
            pair: Pair::new(
                Token::create("eth".to_string()).unwrap(),
                Token::create("usdc".to_string()).unwrap(),
            )
            .unwrap(),
            swap_amount: Uint256::from(500u128),
            swap_route: single_hop("eth", "usdc"),
            min_lp_out: Uint256::from(1u128),
            partner_fee: None,
            cross_chain_config: default_cross_chain_config(),
        }
    }

    /// Seed mock querier for `ueth` supply so `TokenType::Native::validate` succeeds.
    fn set_native_supply(deps: &mut crate::testing::helpers::MockDeps) {
        deps.querier
            .bank
            .update_balance("anywhere", vec![cosmwasm_std::coin(1_000_000, "ueth")]);
        deps.querier
            .bank
            .update_balance("anywhere2", vec![cosmwasm_std::coin(1_000_000, "uusdc")]);
    }

    /// Happy path: native USDC-style deposit creates pending entry and emits IBC submsg.
    #[test]
    fn test_single_sided_happy_path_native_saves_pending_and_emits_ibc() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);
        seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");
        seed_escrow(&mut deps, "eth", "escrow_eth");
        set_escrow_token_allowed(&mut deps, true);

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[cosmwasm_std::coin(1000, "ueth")]);

        let msg = ss_default_msg();
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        // One submsg: the IBC packet
        assert_eq!(res.messages.len(), 1);

        // PENDING_SINGLE_SIDED_LIQUIDITY should now contain exactly one entry for this sender.
        let entries: Vec<_> = PENDING_SINGLE_SIDED_LIQUIDITY
            .range(&deps.storage, None, None, cosmwasm_std::Order::Ascending)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(entries.len(), 1);
        let ((sender_addr, _tx_id), pending) = &entries[0];
        assert_eq!(sender_addr, &user);
        assert_eq!(pending.sender, user.to_string());
        assert_eq!(pending.amount_in, Uint256::from(1000u128));
        assert_eq!(pending.asset_in.token.to_string(), "eth");

        // Attributes
        let attrs = &res.attributes;
        assert!(attrs
            .iter()
            .any(|a| a.key == "method" && a.value == "execute_single_sided_add_liquidity_request"));
        assert!(attrs
            .iter()
            .any(|a| a.key == "asset_in" && a.value == "eth"));
        assert!(attrs
            .iter()
            .any(|a| a.key == "asset_out" && a.value == "usdc"));
    }

    /// PAIR_TO_VLP missing for (asset_in, asset_out) → PoolDoesNotExist.
    #[test]
    fn test_single_sided_pool_does_not_exist() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);
        // intentionally do NOT seed_vlp

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[cosmwasm_std::coin(1000, "ueth")]);

        let msg = ss_default_msg();
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::PoolDoesNotExist {});
    }

    /// asset_in.token not in pair → "asset_in must be one of the pair tokens".
    #[test]
    fn test_single_sided_asset_in_not_in_pair() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[cosmwasm_std::coin(1000, "ueth")]);

        let mut msg = ss_default_msg();
        if let ExecuteMsg::AddSingleSidedLiquidity { ref mut pair, .. } = msg {
            *pair = Pair::new(
                Token::create("usdc".to_string()).unwrap(),
                Token::create("dai".to_string()).unwrap(),
            )
            .unwrap();
        }
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(
            err,
            ContractError::new("asset_in must be one of the pair tokens")
        );
    }

    /// amount_in == 0 → ZeroAssetAmount.
    #[test]
    fn test_single_sided_zero_amount_in() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[]);

        let mut msg = ss_default_msg();
        if let ExecuteMsg::AddSingleSidedLiquidity {
            ref mut amount_in, ..
        } = msg
        {
            *amount_in = Uint256::zero();
        }
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::ZeroAssetAmount {});
    }

    /// swap_amount == 0 → ZeroAssetAmount.
    #[test]
    fn test_single_sided_zero_swap_amount() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[cosmwasm_std::coin(1000, "ueth")]);

        let mut msg = ss_default_msg();
        if let ExecuteMsg::AddSingleSidedLiquidity {
            ref mut swap_amount,
            ..
        } = msg
        {
            *swap_amount = Uint256::zero();
        }
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::ZeroAssetAmount {});
    }

    /// swap_amount > amount_in → "swap_amount must be < amount_in".
    #[test]
    fn test_single_sided_swap_amount_greater_than_amount_in() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[cosmwasm_std::coin(2000, "ueth")]);

        let mut msg = ss_default_msg();
        if let ExecuteMsg::AddSingleSidedLiquidity {
            ref mut swap_amount,
            ref mut amount_in,
            ..
        } = msg
        {
            *amount_in = Uint256::from(1000u128);
            *swap_amount = Uint256::from(2000u128);
        }
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::new("swap_amount must be < amount_in"));
    }

    /// swap_amount == amount_in (boundary) → "swap_amount must be < amount_in".
    #[test]
    fn test_single_sided_swap_amount_equals_amount_in_rejected() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[cosmwasm_std::coin(1000, "ueth")]);

        let mut msg = ss_default_msg();
        if let ExecuteMsg::AddSingleSidedLiquidity {
            ref mut swap_amount,
            ref mut amount_in,
            ..
        } = msg
        {
            *amount_in = Uint256::from(1000u128);
            *swap_amount = Uint256::from(1000u128);
        }
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::new("swap_amount must be < amount_in"));
    }

    /// min_lp_out == 0 → ZeroAssetAmount.
    #[test]
    fn test_single_sided_zero_min_lp_out() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[cosmwasm_std::coin(1000, "ueth")]);

        let mut msg = ss_default_msg();
        if let ExecuteMsg::AddSingleSidedLiquidity {
            ref mut min_lp_out, ..
        } = msg
        {
            *min_lp_out = Uint256::zero();
        }
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::ZeroAssetAmount {});
    }

    /// swap_route.len() != 1 (e.g. empty) → "exactly one hop in v1".
    #[test]
    fn test_single_sided_empty_swap_route() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[cosmwasm_std::coin(1000, "ueth")]);

        let mut msg = ss_default_msg();
        if let ExecuteMsg::AddSingleSidedLiquidity {
            ref mut swap_route, ..
        } = msg
        {
            *swap_route = vec![];
        }
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(
            err,
            ContractError::new("swap_route must contain exactly one hop in v1")
        );
    }

    /// swap_route.len() > 1 → "exactly one hop in v1".
    #[test]
    fn test_single_sided_multi_hop_swap_route_rejected() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[cosmwasm_std::coin(1000, "ueth")]);

        let mut msg = ss_default_msg();
        if let ExecuteMsg::AddSingleSidedLiquidity {
            ref mut swap_route, ..
        } = msg
        {
            let mut routes = single_hop("eth", "usdc");
            routes.push(NextSwapPair {
                token_in: Token::create("usdc".to_string()).unwrap(),
                token_out: Token::create("dai".to_string()).unwrap(),
                pool_key: None,
                test_fail: None,
            });
            *swap_route = routes;
        }
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(
            err,
            ContractError::new("swap_route must contain exactly one hop in v1")
        );
    }

    /// swap_route[0].token_in != asset_in.token → hop_in mismatch error.
    #[test]
    fn test_single_sided_route_token_in_mismatch() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[cosmwasm_std::coin(1000, "ueth")]);

        let mut msg = ss_default_msg();
        if let ExecuteMsg::AddSingleSidedLiquidity {
            ref mut swap_route, ..
        } = msg
        {
            *swap_route = single_hop("dai", "usdc"); // asset_in is eth, not dai
        }
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(
            err,
            ContractError::new("swap_route first hop token_in must match asset_in")
        );
    }

    /// swap_route[0].token_out != other pair token → hop_out mismatch error.
    #[test]
    fn test_single_sided_route_token_out_mismatch() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[cosmwasm_std::coin(1000, "ueth")]);

        let mut msg = ss_default_msg();
        if let ExecuteMsg::AddSingleSidedLiquidity {
            ref mut swap_route, ..
        } = msg
        {
            *swap_route = single_hop("eth", "dai"); // other pair token is usdc, not dai
        }
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(
            err,
            ContractError::new("swap_route last hop token_out must match the other pair token")
        );
    }

    /// Escrow does not exist for asset_in.token → EscrowDoesNotExist.
    #[test]
    fn test_single_sided_escrow_does_not_exist() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);
        seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");
        // No seed_escrow for "eth"

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[cosmwasm_std::coin(1000, "ueth")]);

        let msg = ss_default_msg();
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::EscrowDoesNotExist {});
    }

    /// Escrow exists but TokenAllowed returns false → UnsupportedDenomination.
    #[test]
    fn test_single_sided_token_not_allowed_by_escrow() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);
        seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");
        seed_escrow(&mut deps, "eth", "escrow_eth");
        set_escrow_token_allowed(&mut deps, false);

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[cosmwasm_std::coin(1000, "ueth")]);

        let msg = ss_default_msg();
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::UnsupportedDenomination {});
    }

    /// Native funds insufficient (info.funds doesn't contain enough of the denom)
    /// → fund_manager InsufficientFunds.
    #[test]
    fn test_single_sided_native_funds_mismatch() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);
        seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");
        seed_escrow(&mut deps, "eth", "escrow_eth");
        set_escrow_token_allowed(&mut deps, true);

        let user = deps.api.addr_make("user");
        // Send only 100 of ueth even though amount_in = 1000.
        let info = message_info(&user, &[cosmwasm_std::coin(100, "ueth")]);

        let msg = ss_default_msg();
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::InsufficientFunds {});
    }

    /// Extra funds beyond what amount_in requires → "Extra funds are not allowed".
    #[test]
    fn test_single_sided_extra_funds_rejected() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);
        seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");
        seed_escrow(&mut deps, "eth", "escrow_eth");
        set_escrow_token_allowed(&mut deps, true);

        let user = deps.api.addr_make("user");
        // Send the right amount of ueth, plus an extra unrelated denom.
        let info = message_info(
            &user,
            &[
                cosmwasm_std::coin(1000, "ueth"),
                cosmwasm_std::coin(50, "uextra"),
            ],
        );

        let msg = ss_default_msg();
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::new("Extra funds are not allowed"));
    }

    /// Smart (CW20) asset_in happy path:
    /// - No native funds attached (CW20 TransferFrom model).
    /// - Response carries a TransferFrom submessage pulling `amount_in` (full
    ///   deposit, BEFORE partner-fee deduction) from the user into the factory.
    /// - Pending entry is saved with post-fee `amount_in` and the partner-fee
    ///   fields.
    ///
    /// Mocks both ContractInfo (for token_type.validate) and Smart queries.
    #[test]
    fn test_single_sided_smart_asset_in_happy_path() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");
        seed_escrow(&mut deps, "eth", "escrow_eth");

        let cw20_addr = deps.api.addr_make("cw20").to_string();
        let cw20_for_querier = cw20_addr.clone();

        // Mock all wasm queries: ContractInfo always succeeds; TokenAllowed always true.
        deps.querier.update_wasm(move |q| match q {
            WasmQuery::ContractInfo { .. } => SystemResult::Ok(ContractResult::Ok(
                to_json_binary(&cosmwasm_std::ContractInfoResponse::new(
                    1,
                    cosmwasm_std::Addr::unchecked("creator"),
                    Some(cosmwasm_std::Addr::unchecked("admin")),
                    false,
                    None,
                ))
                .unwrap(),
            )),
            WasmQuery::Smart { msg, .. } => {
                let query: serde_json::Value = serde_json::from_slice(msg.as_slice()).unwrap();
                if query.get("token_allowed").is_some() {
                    SystemResult::Ok(ContractResult::Ok(
                        to_json_binary(&AllowedTokenResponse { allowed: true }).unwrap(),
                    ))
                } else {
                    panic!("unexpected smart query to {cw20_for_querier}")
                }
            }
            _ => panic!("unexpected query"),
        });

        let user = deps.api.addr_make("user");
        // No native funds attached — Smart tokens flow via TransferFrom.
        let info = message_info(&user, &[]);

        let asset_in = TokenWithDenom {
            token: Token::create("eth".to_string()).unwrap(),
            token_type: TokenType::Smart {
                contract_address: cw20_addr.clone(),
                decimals: Some(6),
            },
        };
        let msg = ExecuteMsg::AddSingleSidedLiquidity {
            asset_in,
            amount_in: Uint256::from(1000u128),
            pair: Pair::new(
                Token::create("eth".to_string()).unwrap(),
                Token::create("usdc".to_string()).unwrap(),
            )
            .unwrap(),
            swap_amount: Uint256::from(500u128),
            swap_route: single_hop("eth", "usdc"),
            min_lp_out: Uint256::from(1u128),
            partner_fee: None,
            cross_chain_config: default_cross_chain_config(),
        };
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        // Expect at least one TransferFrom submessage targeting the CW20
        // contract, owned by the user, recipient = the factory contract.
        let env = mock_env();
        let factory_addr = env.contract.address.to_string();
        let user_str = user.to_string();
        let mut found_transfer_from = false;
        for sm in &res.messages {
            if let cosmwasm_std::CosmosMsg::Wasm(cosmwasm_std::WasmMsg::Execute {
                contract_addr,
                msg,
                funds,
            }) = &sm.msg
            {
                if contract_addr != &cw20_addr {
                    continue;
                }
                assert!(funds.is_empty(), "CW20 TransferFrom must carry no funds");
                let parsed: cw20_base::msg::ExecuteMsg =
                    cosmwasm_std::from_json(msg.as_slice()).unwrap();
                if let cw20_base::msg::ExecuteMsg::TransferFrom {
                    owner,
                    recipient,
                    amount,
                } = parsed
                {
                    assert_eq!(owner, user_str);
                    assert_eq!(recipient, factory_addr);
                    // No partner fee → full deposit equals amount_in (1000).
                    assert_eq!(amount, cosmwasm_std::Uint128::from(1000u128));
                    found_transfer_from = true;
                    break;
                }
            }
        }
        assert!(
            found_transfer_from,
            "expected a CW20 TransferFrom submessage targeting the factory"
        );

        // Pending entry persists post-fee amount_in (= 1000 with no fee) and
        // zero partner_fee_amount.
        let tx_id = get_attribute(&res, "tx_id").to_string();
        let pending = PENDING_SINGLE_SIDED_LIQUIDITY
            .load(deps.as_ref().storage, (user.clone(), tx_id))
            .unwrap();
        assert_eq!(pending.amount_in, Uint256::from(1000u128));
        assert_eq!(pending.partner_fee_amount, Uint256::zero());
    }

    /// Smart (CW20) asset_in with a partner fee: the TransferFrom must pull the
    /// FULL deposit (amount_in + partner_fee_amount) — pre-deduction — from the
    /// user, mirroring the native invariant that funds must cover the full
    /// deposit.
    #[test]
    fn test_single_sided_smart_asset_in_partner_fee_transfers_full_deposit() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");
        seed_escrow(&mut deps, "eth", "escrow_eth");

        let cw20_addr = deps.api.addr_make("cw20").to_string();
        let cw20_for_querier = cw20_addr.clone();

        deps.querier.update_wasm(move |q| match q {
            WasmQuery::ContractInfo { .. } => SystemResult::Ok(ContractResult::Ok(
                to_json_binary(&cosmwasm_std::ContractInfoResponse::new(
                    1,
                    cosmwasm_std::Addr::unchecked("creator"),
                    Some(cosmwasm_std::Addr::unchecked("admin")),
                    false,
                    None,
                ))
                .unwrap(),
            )),
            WasmQuery::Smart { msg, .. } => {
                let query: serde_json::Value = serde_json::from_slice(msg.as_slice()).unwrap();
                if query.get("token_allowed").is_some() {
                    SystemResult::Ok(ContractResult::Ok(
                        to_json_binary(&AllowedTokenResponse { allowed: true }).unwrap(),
                    ))
                } else {
                    panic!("unexpected smart query to {cw20_for_querier}")
                }
            }
            _ => panic!("unexpected query"),
        });

        let user = deps.api.addr_make("user");
        let partner = deps.api.addr_make("partner");
        let info = message_info(&user, &[]);

        // amount_in=1000, bps=30 → partner_fee_amount = ceil(1000 * 30/10000) = 3.
        // Full deposit pulled via TransferFrom must equal 1000.
        // After deduction, pending.amount_in = 997.
        let asset_in = TokenWithDenom {
            token: Token::create("eth".to_string()).unwrap(),
            token_type: TokenType::Smart {
                contract_address: cw20_addr.clone(),
                decimals: Some(6),
            },
        };
        let msg = ExecuteMsg::AddSingleSidedLiquidity {
            asset_in,
            amount_in: Uint256::from(1000u128),
            pair: Pair::new(
                Token::create("eth".to_string()).unwrap(),
                Token::create("usdc".to_string()).unwrap(),
            )
            .unwrap(),
            swap_amount: Uint256::from(500u128),
            swap_route: single_hop("eth", "usdc"),
            min_lp_out: Uint256::from(1u128),
            partner_fee: Some(PartnerFee {
                partner_fee_bps: 30,
                recipient: partner.to_string(),
            }),
            cross_chain_config: default_cross_chain_config(),
        };
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        // Find the TransferFrom submessage and assert it pulls the full 1000.
        let mut found_amount: Option<cosmwasm_std::Uint128> = None;
        for sm in &res.messages {
            if let cosmwasm_std::CosmosMsg::Wasm(cosmwasm_std::WasmMsg::Execute {
                contract_addr,
                msg,
                ..
            }) = &sm.msg
            {
                if contract_addr != &cw20_addr {
                    continue;
                }
                if let cw20_base::msg::ExecuteMsg::TransferFrom { amount, .. } =
                    cosmwasm_std::from_json(msg.as_slice()).unwrap()
                {
                    found_amount = Some(amount);
                    break;
                }
            }
        }
        assert_eq!(found_amount, Some(cosmwasm_std::Uint128::from(1000u128)));

        // Pending state holds the post-fee amount_in and the partner fee.
        let tx_id = get_attribute(&res, "tx_id").to_string();
        let pending = PENDING_SINGLE_SIDED_LIQUIDITY
            .load(deps.as_ref().storage, (user.clone(), tx_id))
            .unwrap();
        assert_eq!(pending.amount_in, Uint256::from(997u128));
        assert_eq!(pending.partner_fee_amount, Uint256::from(3u128));
        assert_eq!(pending.partner_fee_recipient, partner);
    }

    /// Voucher asset_in → UnreachableCode (Voucher is structurally impossible from a remote factory).
    #[test]
    fn test_single_sided_voucher_asset_in_rejected() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");
        seed_escrow(&mut deps, "eth", "escrow_eth");
        set_escrow_token_allowed(&mut deps, true);

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[]);

        let voucher = TokenWithDenom {
            token: Token::create("eth".to_string()).unwrap(),
            token_type: TokenType::Voucher {},
        };
        let msg = ExecuteMsg::AddSingleSidedLiquidity {
            asset_in: voucher,
            amount_in: Uint256::from(1000u128),
            pair: Pair::new(
                Token::create("eth".to_string()).unwrap(),
                Token::create("usdc".to_string()).unwrap(),
            )
            .unwrap(),
            swap_amount: Uint256::from(500u128),
            swap_route: single_hop("eth", "usdc"),
            min_lp_out: Uint256::from(1u128),
            partner_fee: None,
            cross_chain_config: default_cross_chain_config(),
        };
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::UnreachableCode {});
    }

    /// Sanity: silence "unused" warnings for items only consumed by certain test cases.
    #[allow(dead_code)]
    fn _silence_unused() {
        let _ = (Pair::new, Uint128::zero);
    }

    // -----------------------------------------------------------------------
    // Execute: AddSingleSidedLiquidity – partner fee tests
    // -----------------------------------------------------------------------

    use euclid::fee::PartnerFee;

    /// partner_fee_bps > MAX_PARTNER_FEE_BPS (30) → InvalidPartnerFee.
    #[test]
    fn test_single_sided_partner_fee_exceeds_cap() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);
        seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");
        seed_escrow(&mut deps, "eth", "escrow_eth");
        set_escrow_token_allowed(&mut deps, true);

        let user = deps.api.addr_make("user");
        let recipient = deps.api.addr_make("partner");
        // amount_in (1000) + ceil(1000 * 31/10000) (4) = 1004 funds attached;
        // the partner-fee check happens before fund checks but we attach valid
        // funds to ensure the cap check is what actually trips.
        let info = message_info(&user, &[cosmwasm_std::coin(1004, "ueth")]);

        let mut msg = ss_default_msg();
        if let ExecuteMsg::AddSingleSidedLiquidity {
            ref mut partner_fee,
            ..
        } = msg
        {
            *partner_fee = Some(PartnerFee {
                partner_fee_bps: 31,
                recipient: recipient.to_string(),
            });
        }
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::InvalidPartnerFee {});
    }

    /// partner_fee_bps = 0 → behaves as no fee: partner_fee_amount=0, recipient=info.sender.
    #[test]
    fn test_single_sided_partner_fee_zero_bps_acts_as_no_fee() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);
        seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");
        seed_escrow(&mut deps, "eth", "escrow_eth");
        set_escrow_token_allowed(&mut deps, true);

        let user = deps.api.addr_make("user");
        let recipient = deps.api.addr_make("partner");
        // 0 bps means full amount_in (1000) crosses, no extra needed for the fee.
        let info = message_info(&user, &[cosmwasm_std::coin(1000, "ueth")]);

        let mut msg = ss_default_msg();
        if let ExecuteMsg::AddSingleSidedLiquidity {
            ref mut partner_fee,
            ..
        } = msg
        {
            *partner_fee = Some(PartnerFee {
                partner_fee_bps: 0,
                recipient: recipient.to_string(),
            });
        }
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(res.messages.len(), 1);

        let entries: Vec<_> = PENDING_SINGLE_SIDED_LIQUIDITY
            .range(&deps.storage, None, None, cosmwasm_std::Order::Ascending)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(entries.len(), 1);
        let ((sender_addr, _tx_id), pending) = &entries[0];
        assert_eq!(sender_addr, &user);
        assert_eq!(pending.amount_in, Uint256::from(1000u128));
        assert_eq!(pending.partner_fee_amount, Uint256::zero());
        // 0-bps with Some(partner_fee) still validates the recipient.
        // The recipient is honored (does NOT default to info.sender).
        assert_eq!(pending.partner_fee_recipient, recipient);
    }

    /// MAX bps fee deposit: native funds must cover post-fee amount_in + partner_fee_amount,
    /// which equals the originally supplied amount_in.
    /// User supplies amount_in=1000, bps=30 → partner_fee_amount = ceil(1000 * 30/10000) = 3,
    /// post-fee amount_in = 997, full_deposit = 997 + 3 = 1000.
    /// Funds=999 fails (InsufficientFunds); funds=1000 succeeds.
    #[test]
    fn test_single_sided_partner_fee_native_funds_must_cover_full_deposit() {
        let recipient_str = {
            let deps = mock_dependencies();
            deps.api.addr_make("partner").to_string()
        };

        // Case A: funds short by 1 → InsufficientFunds.
        {
            let mut deps = mock_dependencies();
            init(&mut deps);
            set_native_supply(&mut deps);
            seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");
            seed_escrow(&mut deps, "eth", "escrow_eth");
            set_escrow_token_allowed(&mut deps, true);

            let user = deps.api.addr_make("user");
            let info = message_info(&user, &[cosmwasm_std::coin(999, "ueth")]);

            let mut msg = ss_default_msg();
            if let ExecuteMsg::AddSingleSidedLiquidity {
                ref mut partner_fee,
                ..
            } = msg
            {
                *partner_fee = Some(PartnerFee {
                    partner_fee_bps: 30,
                    recipient: recipient_str.clone(),
                });
            }
            let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
            assert_eq!(err, ContractError::InsufficientFunds {});
        }

        // Case B: exact full deposit (1000) attached → success.
        {
            let mut deps = mock_dependencies();
            init(&mut deps);
            set_native_supply(&mut deps);
            seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");
            seed_escrow(&mut deps, "eth", "escrow_eth");
            set_escrow_token_allowed(&mut deps, true);

            let user = deps.api.addr_make("user");
            let info = message_info(&user, &[cosmwasm_std::coin(1000, "ueth")]);

            let mut msg = ss_default_msg();
            if let ExecuteMsg::AddSingleSidedLiquidity {
                ref mut partner_fee,
                ..
            } = msg
            {
                *partner_fee = Some(PartnerFee {
                    partner_fee_bps: 30,
                    recipient: recipient_str.clone(),
                });
            }
            let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
            assert_eq!(res.messages.len(), 1);
        }
    }

    /// Pending state reflects post-fee deduction:
    /// User supplies amount_in=1000, bps=30 → stored amount_in=997, partner_fee_amount=3,
    /// partner_fee_recipient = validated recipient. Native funds attached = 1000 (the
    /// original amount_in, which equals post-fee amount_in + partner_fee_amount).
    #[test]
    fn test_single_sided_partner_fee_pending_state_reflects_deduction() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);
        seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");
        seed_escrow(&mut deps, "eth", "escrow_eth");
        set_escrow_token_allowed(&mut deps, true);

        let user = deps.api.addr_make("user");
        let recipient = deps.api.addr_make("partner");
        let info = message_info(&user, &[cosmwasm_std::coin(1000, "ueth")]);

        let mut msg = ss_default_msg();
        // swap_amount default in ss_default_msg is 500; new amount_in becomes 997
        // so 500 < 997 still holds.
        if let ExecuteMsg::AddSingleSidedLiquidity {
            ref mut partner_fee,
            ..
        } = msg
        {
            *partner_fee = Some(PartnerFee {
                partner_fee_bps: 30,
                recipient: recipient.to_string(),
            });
        }
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(res.messages.len(), 1);

        let entries: Vec<_> = PENDING_SINGLE_SIDED_LIQUIDITY
            .range(&deps.storage, None, None, cosmwasm_std::Order::Ascending)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(entries.len(), 1);
        let ((sender_addr, _tx_id), pending) = &entries[0];
        assert_eq!(sender_addr, &user);
        assert_eq!(pending.amount_in, Uint256::from(997u128));
        assert_eq!(pending.partner_fee_amount, Uint256::from(3u128));
        assert_eq!(pending.partner_fee_recipient, recipient);

        // amount_in attribute reflects post-fee amount.
        let attrs = &res.attributes;
        assert!(attrs
            .iter()
            .any(|a| a.key == "amount_in" && a.value == "997"));
    }

    /// partner_fee = None → partner_fee_amount=0 and recipient defaults to info.sender.
    #[test]
    fn test_single_sided_partner_fee_default_recipient_is_sender() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);
        seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");
        seed_escrow(&mut deps, "eth", "escrow_eth");
        set_escrow_token_allowed(&mut deps, true);

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[cosmwasm_std::coin(1000, "ueth")]);

        // ss_default_msg() uses partner_fee: None
        let msg = ss_default_msg();
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(res.messages.len(), 1);

        let entries: Vec<_> = PENDING_SINGLE_SIDED_LIQUIDITY
            .range(&deps.storage, None, None, cosmwasm_std::Order::Ascending)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(entries.len(), 1);
        let ((sender_addr, _tx_id), pending) = &entries[0];
        assert_eq!(sender_addr, &user);
        assert_eq!(pending.partner_fee_amount, Uint256::zero());
        assert_eq!(pending.partner_fee_recipient, user);
    }
}
