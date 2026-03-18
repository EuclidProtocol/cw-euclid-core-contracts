use cosmwasm_std::{ensure, DepsMut, Env, MessageInfo, Response, SubMsg, Uint128};
use cw20::Logo;
use euclid::{
    cross_chain_user::CrossChainUser,
    error::ContractError,
    events::tx_event,
    fee::BPS_100_PERCENT,
    liquidity::{AddLiquidityRequest, RemoveLiquidityRequest},
    msgs::{
        cross_chain_config::CrossChainConfig,
        escrow::AllowedTokenResponse,
        vlp::base::{PoolConfig, PoolType},
    },
    token::{Pair, PairWithDenomAndAmount, TokenType},
    utils::{fund_manager::FundManager, tx::generate_tx},
};
use euclid_ibc::router_ibc::{
    RouterCrossChainConcentratedAddLiquidityExecuteMsg,
    RouterCrossChainConcentratedCollectFeesExecuteMsg,
    RouterCrossChainConcentratedCollectProtocolFeesExecuteMsg,
    RouterCrossChainConcentratedRemoveLiquidityExecuteMsg,
    RouterCrossChainConcentratedRequestPoolCreationExecuteMsg, RouterCrossChainExecuteMsg,
    RouterCrossChainRemoveLiquidityExecuteMsg,
};

use crate::{
    query::get_chain_type,
    state::{
        pool_key_to_map_key, ConcentratedAddLiquidityRequest, ConcentratedCollectFeesRequest,
        ConcentratedCollectProtocolFeesRequest, ConcentratedPoolCreateRequest,
        ConcentratedRemoveLiquidityRequest, PoolCreateRequest, ADMIN, PAIR_TO_VLP,
        PENDING_ADD_LIQUIDITY, PENDING_CONCENTRATED_ADD_LIQUIDITY,
        PENDING_CONCENTRATED_COLLECT_FEES, PENDING_CONCENTRATED_COLLECT_PROTOCOL_FEES,
        PENDING_CONCENTRATED_POOL_REQUESTS, PENDING_CONCENTRATED_REMOVE_LIQUIDITY,
        PENDING_POOL_REQUESTS, PENDING_REMOVE_LIQUIDITY, POOL_KEY_TO_VLP, POSITION_ID_TO_METADATA,
        STATE, TOKEN_TO_ESCROW, VLP_TO_LP_TOKEN,
    },
};

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

    let mut res = Response::new();

    // Changes factory state without sending liquidity request to router. That will be handled in Pool creation request's reply in router
    // Add liquidity Section //
    // Prepare msg vector
    let mut msgs: Vec<SubMsg> = Vec::new();

    let mut fund_manager = FundManager::new(&info.funds);
    let mut one_token_already_exists = false;
    // Do an early check for tokens escrow so that if it exists, it should allow the denom that we are sending
    let tokens = pair_with_denom_and_amount.get_vec_token_info();

    for token in tokens {
        // Validate token id
        token.token.validate()?;

        // Vouchers are not escrowed
        if !token.token_type.is_voucher() {
            match token.token_type.clone() {
                TokenType::Native { denom } => {
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
            // Ensure valid denom if token already exists
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
            // If its a voucher token, then we can assume that one token already exists
            one_token_already_exists = true;
        }
    }

    ensure!(
        one_token_already_exists,
        ContractError::new(
            "Cannot create pool two new tokens. Atleast one token must already be registered."
        )
    );

    res = res.add_submessages(msgs);

    let pair = pair_with_denom_and_amount.get_pair()?;
    // Ensure tokens in pair are different
    ensure!(
        pair.token_1 != pair.token_2,
        ContractError::new("Cannot create pool with same token")
    );

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
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "request_pool_creation")
        .add_submessage(pool_create_msg))
}

// Add liquidity to the pool
// TODO look into alternatives of using .branch(), maybe unifying the functions would help
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
        (1..=BPS_100_PERCENT).contains(&slippage_tolerance_bps),
        ContractError::InvalidSlippageTolerance {}
    );

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
                TokenType::Native { denom } => {
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
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "add_liquidity_request")
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
    lp_allocation: Uint128,
    recipient: CrossChainUser,
    cross_chain_config: CrossChainConfig,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    let sender_addr = deps.api.addr_validate(&sender.address)?;

    let tx_id = generate_tx(deps, &env, &sender)?;

    ensure!(
        !PENDING_REMOVE_LIQUIDITY.has(deps.storage, (sender_addr.clone(), tx_id.clone())),
        ContractError::TxAlreadyExist {}
    );

    let vlp = PAIR_TO_VLP.load(deps.storage, pair.get_tupple())?;
    let lp_token = VLP_TO_LP_TOKEN.load(deps.storage, vlp)?;

    ensure!(lp_token == info.sender, ContractError::Unauthorized {});

    ensure!(
        PAIR_TO_VLP.has(deps.storage, pair.get_tupple()),
        ContractError::PoolDoesNotExist {}
    );

    // Check that the liquidity is greater than 0
    ensure!(!lp_allocation.is_zero(), ContractError::ZeroAssetAmount {});

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
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "remove_liquidity_request")
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
    cross_chain_config: CrossChainConfig,
) -> Result<Response, ContractError> {
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

    let pool_key = euclid::msgs::vlp::base::PoolKey {
        pair: pair.clone(),
        pool_type: euclid::msgs::vlp::base::PoolType::Concentrated {
            fee_tier_bps,
            tick_spacing,
        },
    };

    ensure!(
        !PENDING_CONCENTRATED_POOL_REQUESTS.has(deps.storage, (info.sender.clone(), tx_id.clone())),
        ContractError::TxAlreadyExist {}
    );
    ensure!(
        !POOL_KEY_TO_VLP.has(deps.storage, pool_key_to_map_key(&pool_key)),
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
        (1..=BPS_100_PERCENT).contains(&slippage_tolerance_bps),
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
    ensure!(
        POOL_KEY_TO_VLP.has(deps.storage, pool_key_to_map_key(&pool_key)),
        ContractError::PoolDoesNotExist {}
    );

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
                TokenType::Native { denom } => {
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
    lp_allocation: Uint128,
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
    ensure!(!lp_allocation.is_zero(), ContractError::ZeroAssetAmount {});

    let position_meta = POSITION_ID_TO_METADATA
        .may_load(deps.storage, position_id.u128())?
        .ok_or(ContractError::new("Position not found"))?;
    ensure!(
        position_meta.owner == info.sender,
        ContractError::Unauthorized {}
    );
    ensure!(
        position_meta.pool_key == pool_key,
        ContractError::Unauthorized {}
    );

    let req = ConcentratedRemoveLiquidityRequest {
        tx_id: tx_id.clone(),
        sender: sender_addr.clone(),
        pool_key: pool_key.clone(),
        position_id: position_id.u128(),
        lp_allocation,
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
            lp_allocation,
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
    ensure!(
        POOL_KEY_TO_VLP.has(deps.storage, pool_key_to_map_key(&pool_key)),
        ContractError::PoolDoesNotExist {}
    );

    let position_meta = POSITION_ID_TO_METADATA.load(deps.storage, position_id.u128())?;

    ensure!(
        position_meta.owner == info.sender,
        ContractError::Unauthorized {}
    );
    ensure!(
        position_meta.pool_key == pool_key,
        ContractError::new("Pool key mismatch")
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
        POOL_KEY_TO_VLP.has(deps.storage, pool_key_to_map_key(&pool_key)),
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
