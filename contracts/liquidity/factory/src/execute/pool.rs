use cosmwasm_std::{ensure, DepsMut, Env, Int256, MessageInfo, Response, SubMsg, Uint128};
use cw20::Logo;
use euclid::{
    cross_chain_user::CrossChainUser,
    error::ContractError,
    events::tx_event,
    fee::BPS_100_PERCENT,
    liquidity::{AddLiquidityRequest, RemoveLiquidityRequest, MAX_TICK, MIN_TICK},
    msgs::{
        self,
        cross_chain_config::CrossChainConfig,
        escrow::AllowedTokenResponse,
        position_token::{
            OwnerOfResponse, PositionInfoResponse, QueryMsg as PositionTokenQueryMsg,
        },
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
        ConcentratedAddLiquidityRequest, ConcentratedCollectFeesRequest,
        ConcentratedCollectProtocolFeesRequest, ConcentratedPoolCreateRequest,
        ConcentratedRemoveLiquidityRequest, PoolCreateRequest, ADMIN, PAIR_TO_VLP,
        PENDING_ADD_LIQUIDITY, PENDING_CONCENTRATED_ADD_LIQUIDITY,
        PENDING_CONCENTRATED_COLLECT_FEES, PENDING_CONCENTRATED_COLLECT_PROTOCOL_FEES,
        PENDING_CONCENTRATED_POOL_REQUESTS, PENDING_CONCENTRATED_REMOVE_LIQUIDITY,
        PENDING_POOL_REQUESTS, PENDING_REMOVE_LIQUIDITY, POOL_KEY_TO_VLP, POSITION_TOKEN_CONTRACT,
        STATE, TOKEN_TO_ESCROW, VLP_TO_LP_TOKEN,
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
                TokenType::Native { denom } => {
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
        slippage_tolerance_bps >= 1 && slippage_tolerance_bps <= BPS_100_PERCENT,
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
    lp_allocation: Uint128,
    recipient: CrossChainUser,
    cross_chain_config: CrossChainConfig,
) -> Result<Response, ContractError> {
    // Reject mixed-case or empty addresses before mutating state
    sender.validate()?;
    recipient.validate()?;

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

#[cfg(test)]
mod tests {
    use cosmwasm_std::{
        testing::{message_info, mock_dependencies, mock_env},
        Uint128,
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
                },
                amount: Uint128::new(100),
            },
            token_2: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("usdc".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uusdc".to_string(),
                },
                amount: Uint128::new(100),
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
                },
                amount: Uint128::new(100),
            },
            token_2: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("usdc".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uusdc".to_string(),
                },
                amount: Uint128::new(100),
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
                },
                amount: Uint128::zero(),
            },
            token_2: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("usdc".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uusdc".to_string(),
                },
                amount: Uint128::new(100),
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
                },
                amount: Uint128::new(100),
            },
            token_2: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("usdc".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uusdc".to_string(),
                },
                amount: Uint128::new(100),
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
                },
                amount: Uint128::new(100),
            },
            token_2: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("usdc".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uusdc".to_string(),
                },
                amount: Uint128::new(100),
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
                },
                amount: Uint128::new(100),
            },
            token_2: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("usdc".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uusdc".to_string(),
                },
                amount: Uint128::new(100),
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
                },
                amount: Uint128::new(100),
            },
            token_2: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("bbb".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "ubbb".to_string(),
                },
                amount: Uint128::new(100),
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
}
