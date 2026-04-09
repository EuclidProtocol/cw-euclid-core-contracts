use std::collections::HashMap;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    ensure, to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, SubMsg,
    Uint128, Uint256, WasmMsg,
};
use cw2::set_contract_version;
use euclid::{
    admin::EuclidAdmin,
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    error::ContractError,
    events::{clp_add_liquidity_event, liquidity_event, tx_event, TxType},
    fee::{DenomFees, TotalFees},
    msgs::vlp::{
        base::{
            GetSwapQueryResponse, PoolConfig, PoolType, State, VlpConcentratedAddLiquidityResponse,
            VlpConcentratedCollectFeesResponse, VlpConcentratedCollectProtocolFeesResponse,
            VlpConcentratedRemoveLiquidityResponse, VlpSwapMsg, VlpSwapResponse,
            NEXT_SWAP_REPLY_ID,
        },
        concentrated::msg::{
            ExecuteMsg, InstantiateMsg, LegacyLiquidityMode, MigrationStatusResponse,
            ObserveResponse, PositionResponse, ProtocolFeesResponse, QueryMsg, Slot0Response,
            TickResponse, TicksResponse,
        },
    },
    swap::NextSwapVlp,
    token::Token,
};
use euclid_pool::{register_pool, update_admin, update_fee};

use crate::{
    math::{
        liquidity_amounts::{get_amounts_for_liquidity, get_liquidity_for_amounts},
        oracle::{initialize_observation, observe, write_observation},
        position_math::{
            accumulate_fee_growth, fee_growth_inside, fees_owed, flip_fee_growth_outside,
        },
        swap_math::{compute_swap_step_exact_input, FEE_DENOMINATOR_PIPS},
        tick_math::{
            get_sqrt_ratio_at_tick, get_tick_at_sqrt_ratio, max_sqrt_ratio, min_sqrt_ratio,
        },
    },
    query::{
        extract_token_amount, query_all_pools, query_fee, query_liquidity, query_pool, query_state,
        query_total_fees_collected, query_total_fees_per_denom,
    },
    reply,
    state::{
        ConcentratedPosition, MigrationMetadata, Slot0, TickInfo, ACTIVE_LIQUIDITY, ADMIN,
        BALANCES, CHAIN_LP_TOKENS, FEE_GROWTH_GLOBAL_0_X128, FEE_GROWTH_GLOBAL_1_X128, MAX_TICK,
        MIGRATION_METADATA, MIGRATION_REVISION, MIN_TICK, POOL_KEY, POSITIONS, PROTOCOL_FEES_0,
        PROTOCOL_FEES_1, SLOT0, STATE, TICKS,
    },
};

const CONTRACT_NAME: &str = "crates.io:concentrated_vlp";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
const MAX_SWAP_STEPS: u32 = 4096;

#[derive(Clone)]
struct CrossedTickUpdate {
    tick: i64,
    fee_growth_outside_0_x128: Uint256,
    fee_growth_outside_1_x128: Uint256,
}

#[derive(Clone)]
struct SwapSimulation {
    state: State,
    slot0: Slot0,
    liquidity: Uint128,
    fee_growth_global_0_x128: Uint256,
    fee_growth_global_1_x128: Uint256,
    protocol_fees_0: Uint128,
    protocol_fees_1: Uint128,
    amount_out: Uint128,
    asset_out: Token,
    lp_fee: Uint128,
    protocol_fee: Uint128,
    crossed_ticks: Vec<CrossedTickUpdate>,
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    msg.pair.validate()?;
    validate_concentrated_fee_and_spacing(msg.fee_tier_bps, msg.tick_spacing)?;

    let state = State {
        pair: msg.pair.clone(),
        virtual_balance_contract: msg.virtual_balance_contract,
        router: info.sender.clone(),
        fee: msg.fee,
        total_fees_collected: TotalFees {
            lp_fees: DenomFees {
                totals: HashMap::default(),
            },
            euclid_fees: DenomFees {
                totals: HashMap::default(),
            },
        },
        last_updated: 0,
        total_lp_tokens: Uint128::zero(),
    };

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    STATE.save(deps.storage, &state)?;
    ADMIN.save(deps.storage, &EuclidAdmin::default(msg.admin))?;

    BALANCES.save(deps.storage, state.pair.token_1.clone(), &Uint128::zero())?;
    BALANCES.save(deps.storage, state.pair.token_2.clone(), &Uint128::zero())?;
    POOL_KEY.save(
        deps.storage,
        &euclid::msgs::vlp::base::PoolKey {
            pair: msg.pair,
            pool_type: PoolType::Concentrated {
                fee_tier_bps: msg.fee_tier_bps,
                tick_spacing: msg.tick_spacing,
            },
        },
    )?;
    let initial_tick = msg.initial_tick.unwrap_or(0);
    ensure!(
        (MIN_TICK..=MAX_TICK).contains(&initial_tick),
        ContractError::new("initial_tick out of bounds")
    );
    SLOT0.save(
        deps.storage,
        &Slot0 {
            sqrt_price_x96: get_sqrt_ratio_at_tick(initial_tick)?,
            tick: initial_tick,
            observation_index: 0,
            observation_cardinality: 1,
            observation_cardinality_next: 1,
        },
    )?;
    ACTIVE_LIQUIDITY.save(deps.storage, &Uint128::zero())?;
    FEE_GROWTH_GLOBAL_0_X128.save(deps.storage, &Uint256::zero())?;
    FEE_GROWTH_GLOBAL_1_X128.save(deps.storage, &Uint256::zero())?;
    PROTOCOL_FEES_0.save(deps.storage, &Uint128::zero())?;
    PROTOCOL_FEES_1.save(deps.storage, &Uint128::zero())?;
    initialize_observation(deps.storage, env.block.time.seconds())?;
    MIGRATION_REVISION.save(deps.storage, &2)?;
    MIGRATION_METADATA.save(
        deps.storage,
        &MigrationMetadata {
            source_version: CONTRACT_VERSION.to_string(),
            mode: LegacyLiquidityMode::AlreadyV3Liquidity,
            migrated_at: env.block.time.seconds(),
            positions_migrated: 0,
        },
    )?;

    let response =
        msg.execute
            .map_or(Ok(Response::default()), |execute_msg| match execute_msg {
                ExecuteMsg::RegisterPool(register_pool_msg) => {
                    let register_res = register_pool(
                        deps,
                        env.clone(),
                        info.clone(),
                        &STATE,
                        &CHAIN_LP_TOKENS,
                        PoolConfig::Concentrated {
                            fee_tier_bps: msg.fee_tier_bps,
                            tick_spacing: msg.tick_spacing,
                        },
                        register_pool_msg.sender.clone(),
                        register_pool_msg.pool_key.pair.clone(),
                        register_pool_msg.tx_id.clone(),
                    )?;
                    let ack = euclid::msgs::vlp::base::ConcentratedPoolCreationResponse {
                        vlp_contract: env.contract.address.to_string(),
                        tx_id: register_pool_msg.tx_id,
                        sender: register_pool_msg.sender,
                        pool_key: register_pool_msg.pool_key,
                    };
                    Ok(register_res.set_data(to_json_binary(&ack)?))
                }
                _ => Err(ContractError::Unauthorized {}),
            })?;

    Ok(response
        .add_attribute("method", "instantiate")
        .add_attribute("vlp_address", env.contract.address.to_string())
        .add_attribute("owner", info.sender)
        .add_attribute("pool_type", "concentrated"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::RegisterPool(register_pool_msg) => {
            let pool_state = POOL_KEY.load(deps.storage)?;
            ensure!(
                pool_state == register_pool_msg.pool_key,
                ContractError::Generic {
                    err: format!(
                        "Invalid pool key: expected {:?}, got {:?}",
                        pool_state, register_pool_msg.pool_key
                    ),
                }
            );
            let register_res = register_pool(
                deps,
                env.clone(),
                info,
                &STATE,
                &CHAIN_LP_TOKENS,
                PoolConfig::Concentrated {
                    fee_tier_bps: pool_state.get_fee_tier_bps()?,
                    tick_spacing: pool_state.get_tick_spacing()?,
                },
                register_pool_msg.sender.clone(),
                register_pool_msg.pool_key.pair.clone(),
                register_pool_msg.tx_id.clone(),
            )?;
            let ack = euclid::msgs::vlp::base::ConcentratedPoolCreationResponse {
                vlp_contract: env.contract.address.to_string(),
                tx_id: register_pool_msg.tx_id,
                sender: register_pool_msg.sender,
                pool_key: register_pool_msg.pool_key,
            };
            Ok(register_res.set_data(to_json_binary(&ack)?))
        }
        ExecuteMsg::AddLiquidity(add_liquidity_msg) => {
            execute_add_concentrated_liquidity(deps, env, info, add_liquidity_msg)
        }
        ExecuteMsg::RemoveLiquidity(remove_liquidity_msg) => {
            execute_remove_concentrated_liquidity(deps, env, info, remove_liquidity_msg)
        }
        ExecuteMsg::CollectFees(msg) => execute_collect_fees(deps, env, info, msg),
        ExecuteMsg::CollectProtocolFees(msg) => execute_collect_protocol_fees(deps, env, info, msg),
        ExecuteMsg::IncreaseObservationCardinalityNext {
            observation_cardinality_next,
        } => increase_observation_cardinality_next(deps, info, observation_cardinality_next),
        ExecuteMsg::Swap(swap_msg) => execute_clp_swap(deps, env, info, swap_msg),
        ExecuteMsg::UpdateFee {
            lp_fee_bps,
            euclid_fee_bps,
            recipient,
        } => update_fee(
            deps,
            info,
            &STATE,
            &ADMIN,
            lp_fee_bps,
            euclid_fee_bps,
            recipient,
        ),
        ExecuteMsg::UpdateAdmin { admin, admin_type } => {
            update_admin(deps, env, info, &ADMIN, admin, admin_type)
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::State {} => query_state(deps),
        QueryMsg::SimulateSwap(msg) => {
            query_clp_simulate_swap(deps, msg.asset, msg.asset_amount, msg.swaps)
        }
        QueryMsg::Liquidity {} => query_liquidity(deps, env),
        QueryMsg::Fee {} => query_fee(deps),
        QueryMsg::TotalFeesCollected {} => query_total_fees_collected(deps),
        QueryMsg::TotalFeesPerDenom { denom } => query_total_fees_per_denom(deps, denom),
        QueryMsg::Pool {
            chain_uid,
            pool_key,
        } => query_pool(deps, chain_uid, pool_key),
        QueryMsg::GetAllPools {} => query_all_pools(deps),
        QueryMsg::Slot0 {} => to_json_binary(&query_slot0(deps)?).map_err(ContractError::from),
        QueryMsg::Position { position_id } => {
            to_json_binary(&query_position(deps, position_id)?).map_err(ContractError::from)
        }
        QueryMsg::Tick { index } => {
            to_json_binary(&query_tick(deps, index)?).map_err(ContractError::from)
        }
        QueryMsg::Ticks { start_after, limit } => to_json_binary(&query_ticks(
            deps,
            start_after,
            limit.unwrap_or(50).min(200),
        )?)
        .map_err(ContractError::from),
        QueryMsg::Observe { seconds_agos } => to_json_binary(&query_observe(
            deps,
            env.block.time.seconds(),
            seconds_agos,
        )?)
        .map_err(ContractError::from),
        QueryMsg::ProtocolFees {} => {
            to_json_binary(&query_protocol_fees(deps)?).map_err(ContractError::from)
        }
        QueryMsg::MigrationStatus {} => {
            to_json_binary(&query_migration_status(deps)?).map_err(ContractError::from)
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        NEXT_SWAP_REPLY_ID => reply::on_next_swap_reply(deps, msg),

        id => Err(ContractError::Generic {
            err: format!("Unknown reply id: {id}"),
        }),
    }
}

pub(crate) fn uint256_to_uint128(value: Uint256) -> Result<Uint128, ContractError> {
    Uint128::try_from(value).map_err(|_| ContractError::new("uint128 overflow"))
}

pub(crate) fn amounts_for_position_liquidity_with_bound(
    sqrt_price_x96: Uint256,
    sqrt_lower_x96: Uint256,
    sqrt_upper_x96: Uint256,
    liquidity_delta: Uint128,
    max_amount_0: Uint128,
    max_amount_1: Uint128,
) -> Result<(Uint128, Uint128, Uint128), ContractError> {
    let fits = |liq: Uint128| -> Result<Option<(Uint128, Uint128)>, ContractError> {
        let (a0_u256, a1_u256) =
            get_amounts_for_liquidity(sqrt_price_x96, sqrt_lower_x96, sqrt_upper_x96, liq, true)?;
        let a0 = uint256_to_uint128(a0_u256)?;
        let a1 = uint256_to_uint128(a1_u256)?;
        if a0 <= max_amount_0 && a1 <= max_amount_1 {
            Ok(Some((a0, a1)))
        } else {
            Ok(None)
        }
    };

    if let Some((a0, a1)) = fits(liquidity_delta)? {
        return Ok((liquidity_delta, a0, a1));
    }

    let mut lo = Uint128::zero();
    let mut hi = liquidity_delta;
    let mut best: Option<(Uint128, Uint128, Uint128)> = None;

    for _ in 0..128u32 {
        if lo >= hi {
            break;
        }
        let mid = lo.checked_add(hi.checked_sub(lo)? / Uint128::new(2))?;
        if let Some((a0, a1)) = fits(mid)? {
            best = Some((mid, a0, a1));
            lo = mid.checked_add(Uint128::new(1))?;
        } else {
            hi = mid;
        }
    }

    best.ok_or_else(|| {
        ContractError::new("failed to fit liquidity amounts within provided amounts")
    })
}

fn assert_unused_within_slippage(
    provided: Uint128,
    used: Uint128,
    slippage_tolerance_bps: u64,
) -> Result<(), ContractError> {
    if provided.is_zero() {
        return Ok(());
    }

    let unused = provided.checked_sub(used)?;
    let lhs = Uint256::from(unused.u128()).checked_mul(Uint256::from(10_000u128))?;
    let rhs = Uint256::from(provided.u128())
        .checked_mul(Uint256::from(u128::from(slippage_tolerance_bps)))?;
    ensure!(
        lhs <= rhs,
        ContractError::new("unused token amount exceeds slippage tolerance")
    );
    Ok(())
}

fn execute_add_concentrated_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    add_liquidity_msg: euclid::msgs::vlp::base::VlpConcentratedAddLiquidityMsg,
) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.router, ContractError::Unauthorized {});
    ensure!(
        add_liquidity_msg.slippage_tolerance_bps <= 10_000,
        ContractError::InvalidSlippageTolerance {}
    );

    let pool_key = add_liquidity_msg.pool_key.clone();
    let stored_pool_key = POOL_KEY.load(deps.storage)?;
    ensure!(
        pool_key == stored_pool_key,
        ContractError::new("pool key mismatch")
    );
    let sender = add_liquidity_msg.sender.clone();
    let tx_id = add_liquidity_msg.tx_id.clone();
    let lower_tick_index = add_liquidity_msg.lower_tick_index;
    let upper_tick_index = add_liquidity_msg.upper_tick_index;
    let position_id = add_liquidity_msg.position_id;
    validate_tick_range(&pool_key, lower_tick_index, upper_tick_index)?;

    ensure!(
        add_liquidity_msg.liquidity.get_pair()? == state.pair,
        ContractError::new("liquidity tokens do not match pool pair")
    );
    let (provided_0, provided_1) = extract_token_amount(&add_liquidity_msg.liquidity, &state.pair);
    ensure!(
        !provided_0.is_zero() || !provided_1.is_zero(),
        ContractError::ZeroAssetAmount {}
    );

    let mut response = Response::new();
    for token in add_liquidity_msg.liquidity.get_vec_token() {
        let transfer_msg = token.token.create_voucher_transfer_msg(
            state.virtual_balance_contract.to_string(),
            token.amount,
            None,
            CrossChainUser {
                address: env.contract.address.to_string(),
                chain_uid: ChainUid::vsl_chain_uid()?,
            },
            Some(sender.clone()),
            None,
        )?;
        response = response.add_message(transfer_msg);
    }

    let slot0 = SLOT0.load(deps.storage)?;
    let sqrt_price_x96 = slot0.sqrt_price_x96;
    let sqrt_lower_x96 = get_sqrt_ratio_at_tick(lower_tick_index)?;
    let sqrt_upper_x96 = get_sqrt_ratio_at_tick(upper_tick_index)?;

    let target_liquidity_delta = get_liquidity_for_amounts(
        sqrt_price_x96,
        sqrt_lower_x96,
        sqrt_upper_x96,
        provided_0,
        provided_1,
    )?;
    ensure!(
        !target_liquidity_delta.is_zero(),
        ContractError::new("liquidity delta is zero")
    );
    let (liquidity_delta, amount_0_used, amount_1_used) =
        amounts_for_position_liquidity_with_bound(
            sqrt_price_x96,
            sqrt_lower_x96,
            sqrt_upper_x96,
            target_liquidity_delta,
            provided_0,
            provided_1,
        )?;
    assert_unused_within_slippage(
        provided_0,
        amount_0_used,
        add_liquidity_msg.slippage_tolerance_bps,
    )?;
    assert_unused_within_slippage(
        provided_1,
        amount_1_used,
        add_liquidity_msg.slippage_tolerance_bps,
    )?;

    let mut position = POSITIONS
        .may_load(deps.storage, position_id.u128())?
        .unwrap_or(ConcentratedPosition {
            chain_uid: sender.chain_uid.clone(),
            lower_tick_index,
            upper_tick_index,
            liquidity: Uint128::zero(),
            fee_growth_inside_0_last_x128: Uint256::zero(),
            fee_growth_inside_1_last_x128: Uint256::zero(),
            tokens_owed_0: Uint128::zero(),
            tokens_owed_1: Uint128::zero(),
        });

    if position.chain_uid != sender.chain_uid {
        return Err(ContractError::Unauthorized {});
    }
    if position.lower_tick_index != lower_tick_index
        || position.upper_tick_index != upper_tick_index
    {
        return Err(ContractError::new("position tick range mismatch"));
    }

    // Update ticks BEFORE settling fees — tick initialization sets
    // fee_growth_outside, which affects the fee_growth_inside calculation.
    // Settling first on a new position computes inside using non-existent
    // ticks (default zeros), recording last=global. Then tick init sets
    // lower.outside=global, making actual inside=0 while last=global.
    // This matches Uniswap V3 ordering: Tick.update then Position.update.
    apply_liquidity_delta(
        deps.storage,
        lower_tick_index,
        upper_tick_index,
        i128::try_from(liquidity_delta.u128())
            .map_err(|_| ContractError::new("liquidity delta overflow"))?,
    )?;
    settle_position_fees(deps.storage, &mut position)?;

    position.liquidity = position.liquidity.checked_add(liquidity_delta)?;
    POSITIONS.save(deps.storage, position_id.u128(), &position)?;

    let mut chain_lp_tokens = CHAIN_LP_TOKENS
        .may_load(deps.storage, sender.chain_uid.clone())?
        .ok_or(ContractError::Generic {
            err: format!(
                "chain {:?} is not registered for this pool",
                sender.chain_uid
            ),
        })?;
    chain_lp_tokens = chain_lp_tokens.checked_add(liquidity_delta)?;
    CHAIN_LP_TOKENS.save(deps.storage, sender.chain_uid.clone(), &chain_lp_tokens)?;

    state.total_lp_tokens = state.total_lp_tokens.checked_add(liquidity_delta)?;
    STATE.save(deps.storage, &state)?;

    // Update oracle so seconds_per_liquidity_cumulative stays accurate
    // across periods with only liquidity changes and no swaps (I-06).
    let slot0 = SLOT0.load(deps.storage)?;
    let active_liq = ACTIVE_LIQUIDITY.load(deps.storage)?;
    write_observation(
        deps.storage,
        env.block.time.seconds(),
        slot0.tick,
        active_liq,
    )?;

    let mut reserve_0 = BALANCES.load(deps.storage, state.pair.token_1.clone())?;
    let mut reserve_1 = BALANCES.load(deps.storage, state.pair.token_2.clone())?;
    reserve_0 = reserve_0.checked_add(amount_0_used)?;
    reserve_1 = reserve_1.checked_add(amount_1_used)?;
    BALANCES.save(deps.storage, state.pair.token_1.clone(), &reserve_0)?;
    BALANCES.save(deps.storage, state.pair.token_2.clone(), &reserve_1)?;

    let refund_0 = provided_0.checked_sub(amount_0_used)?;
    let refund_1 = provided_1.checked_sub(amount_1_used)?;
    if !refund_0.is_zero() {
        response = response.add_message(state.pair.token_1.create_voucher_transfer_msg(
            state.virtual_balance_contract.to_string(),
            refund_0,
            None,
            sender.clone(),
            None,
            None,
        )?);
    }
    if !refund_1.is_zero() {
        response = response.add_message(state.pair.token_2.create_voucher_transfer_msg(
            state.virtual_balance_contract.to_string(),
            refund_1,
            None,
            sender.clone(),
            None,
            None,
        )?);
    }

    let liquidity_added = state
        .pair
        .get_pair_with_amount(amount_0_used, amount_1_used)?;
    let concentrated_ack = VlpConcentratedAddLiquidityResponse {
        liquidity_added: liquidity_added.clone(),
        liquidity_delta,
        position_id,
        tx_id: tx_id.clone(),
        sender: sender.clone(),
        vlp_address: env.contract.address.to_string(),
        pool_key,
    };

    Ok(response
        .add_event(tx_event(
            &tx_id,
            &sender.to_sender_string(),
            TxType::AddLiquidity,
        ))
        .add_event(liquidity_event(
            &state
                .pair
                .get_pair_with_amount(reserve_0, reserve_1)?
                .get_vec_token(),
            &liquidity_added.get_vec_token(),
            &tx_id,
        ))
        .add_attribute("action", "add_concentrated_liquidity")
        .add_attribute("position_id", position_id)
        .add_attribute("liquidity_delta", liquidity_delta)
        .add_attribute("used_token_1", amount_0_used)
        .add_attribute("used_token_2", amount_1_used)
        .add_event(clp_add_liquidity_event(
            &tx_id,
            position_id,
            liquidity_delta,
            amount_0_used,
            amount_1_used,
        ))
        .set_data(to_json_binary(&concentrated_ack)?))
}

fn execute_remove_concentrated_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    remove_liquidity_msg: euclid::msgs::vlp::base::VlpConcentratedRemoveLiquidityMsg,
) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.router, ContractError::Unauthorized {});
    ensure!(
        !remove_liquidity_msg.liquidity_delta.is_zero(),
        ContractError::ZeroAssetAmount {}
    );

    let mut position = POSITIONS
        .may_load(deps.storage, remove_liquidity_msg.position_id.u128())?
        .ok_or(ContractError::new("Position not found"))?;

    if position.chain_uid != remove_liquidity_msg.sender.chain_uid {
        return Err(ContractError::Unauthorized {});
    }
    settle_position_fees(deps.storage, &mut position)?;
    ensure!(
        position.liquidity >= remove_liquidity_msg.liquidity_delta,
        ContractError::new("insufficient position liquidity")
    );

    let slot0 = SLOT0.load(deps.storage)?;
    let sqrt_price_x96 = slot0.sqrt_price_x96;
    let sqrt_lower_x96 = get_sqrt_ratio_at_tick(position.lower_tick_index)?;
    let sqrt_upper_x96 = get_sqrt_ratio_at_tick(position.upper_tick_index)?;
    let (amount_0_out_u256, amount_1_out_u256) = get_amounts_for_liquidity(
        sqrt_price_x96,
        sqrt_lower_x96,
        sqrt_upper_x96,
        remove_liquidity_msg.liquidity_delta,
        false,
    )?;
    let amount_0_out = uint256_to_uint128(amount_0_out_u256)?;
    let amount_1_out = uint256_to_uint128(amount_1_out_u256)?;

    apply_liquidity_delta(
        deps.storage,
        position.lower_tick_index,
        position.upper_tick_index,
        -i128::try_from(remove_liquidity_msg.liquidity_delta.u128())
            .map_err(|_| ContractError::new("liquidity delta overflow"))?,
    )?;
    position.liquidity = position
        .liquidity
        .checked_sub(remove_liquidity_msg.liquidity_delta)?;
    let liquidity_after = position.liquidity;

    // When fully removing liquidity, auto-collect any pending fees so the
    // position can be deleted in a single transaction. Without this, users
    // would need a separate collect_fees call to clear tokens_owed before
    // the position is cleaned up from storage.
    let (fee_0_collected, fee_1_collected) = if position.liquidity.is_zero() {
        let f0 = position.tokens_owed_0;
        let f1 = position.tokens_owed_1;
        position.tokens_owed_0 = Uint128::zero();
        position.tokens_owed_1 = Uint128::zero();
        (f0, f1)
    } else {
        (Uint128::zero(), Uint128::zero())
    };

    let position_burned = position.liquidity.is_zero()
        && position.tokens_owed_0.is_zero()
        && position.tokens_owed_1.is_zero();
    if position_burned {
        POSITIONS.remove(deps.storage, remove_liquidity_msg.position_id.u128());
    } else {
        POSITIONS.save(
            deps.storage,
            remove_liquidity_msg.position_id.u128(),
            &position,
        )?;
    }

    let mut chain_lp_tokens = CHAIN_LP_TOKENS
        .may_load(deps.storage, remove_liquidity_msg.sender.chain_uid.clone())?
        .ok_or(ContractError::Generic {
            err: format!(
                "chain {:?} is not registered for this pool",
                remove_liquidity_msg.sender.chain_uid
            ),
        })?;
    chain_lp_tokens = chain_lp_tokens.checked_sub(remove_liquidity_msg.liquidity_delta)?;
    CHAIN_LP_TOKENS.save(
        deps.storage,
        remove_liquidity_msg.sender.chain_uid.clone(),
        &chain_lp_tokens,
    )?;

    state.total_lp_tokens = state
        .total_lp_tokens
        .checked_sub(remove_liquidity_msg.liquidity_delta)?;
    STATE.save(deps.storage, &state)?;

    // Update oracle so seconds_per_liquidity_cumulative stays accurate (I-06).
    let slot0 = SLOT0.load(deps.storage)?;
    let active_liq = ACTIVE_LIQUIDITY.load(deps.storage)?;
    write_observation(
        deps.storage,
        env.block.time.seconds(),
        slot0.tick,
        active_liq,
    )?;

    let total_0_out = amount_0_out.checked_add(fee_0_collected)?;
    let total_1_out = amount_1_out.checked_add(fee_1_collected)?;

    let mut reserve_0 = BALANCES.load(deps.storage, state.pair.token_1.clone())?;
    let mut reserve_1 = BALANCES.load(deps.storage, state.pair.token_2.clone())?;
    reserve_0 = reserve_0.checked_sub(total_0_out)?;
    reserve_1 = reserve_1.checked_sub(total_1_out)?;
    BALANCES.save(deps.storage, state.pair.token_1.clone(), &reserve_0)?;
    BALANCES.save(deps.storage, state.pair.token_2.clone(), &reserve_1)?;

    let liquidity_released = state
        .pair
        .get_pair_with_amount(amount_0_out, amount_1_out)?;
    let concentrated_ack = VlpConcentratedRemoveLiquidityResponse {
        liquidity_released: liquidity_released.clone(),
        liquidity_delta: remove_liquidity_msg.liquidity_delta,
        liquidity_after,
        position_id: remove_liquidity_msg.position_id,
        tx_id: remove_liquidity_msg.tx_id.clone(),
        sender: remove_liquidity_msg.sender.clone(),
        vlp_address: env.contract.address.to_string(),
        pool_key: remove_liquidity_msg.pool_key,
        position_burned,
    };

    let mut response = Response::new();
    if !total_0_out.is_zero() {
        response = response.add_message(state.pair.token_1.create_voucher_transfer_msg(
            state.virtual_balance_contract.to_string(),
            total_0_out,
            None,
            remove_liquidity_msg.sender.clone(),
            None,
            None,
        )?);
    }
    if !total_1_out.is_zero() {
        response = response.add_message(state.pair.token_2.create_voucher_transfer_msg(
            state.virtual_balance_contract.to_string(),
            total_1_out,
            None,
            remove_liquidity_msg.sender.clone(),
            None,
            None,
        )?);
    }

    Ok(response
        .add_event(tx_event(
            &remove_liquidity_msg.tx_id,
            &remove_liquidity_msg.sender.to_sender_string(),
            TxType::RemoveLiquidity,
        ))
        .add_event(liquidity_event(
            &state
                .pair
                .get_pair_with_amount(reserve_0, reserve_1)?
                .get_vec_token(),
            &liquidity_released.get_vec_token(),
            &remove_liquidity_msg.tx_id,
        ))
        .add_attribute("action", "remove_concentrated_liquidity")
        .add_attribute("position_id", remove_liquidity_msg.position_id)
        .add_attribute("liquidity_delta", remove_liquidity_msg.liquidity_delta)
        .set_data(to_json_binary(&concentrated_ack)?))
}

fn tick_spacing_from_pool_key(
    pool_key: &euclid::msgs::vlp::base::PoolKey,
) -> Result<u64, ContractError> {
    match pool_key.pool_type {
        PoolType::Concentrated { tick_spacing, .. } => Ok(tick_spacing),
        _ => Err(ContractError::new("invalid pool type")),
    }
}

fn validate_tick_range(
    pool_key: &euclid::msgs::vlp::base::PoolKey,
    lower_tick_index: i64,
    upper_tick_index: i64,
) -> Result<(), ContractError> {
    ensure!(
        lower_tick_index < upper_tick_index,
        ContractError::new("invalid tick range")
    );
    ensure!(
        lower_tick_index >= MIN_TICK && upper_tick_index <= MAX_TICK,
        ContractError::new("tick out of bounds")
    );
    let tick_spacing = tick_spacing_from_pool_key(pool_key)? as i64;
    ensure!(
        lower_tick_index % tick_spacing == 0 && upper_tick_index % tick_spacing == 0,
        ContractError::new("tick not aligned with spacing")
    );
    Ok(())
}

fn add_signed_liquidity(value: Uint128, delta: i128) -> Result<Uint128, ContractError> {
    if delta >= 0 {
        value
            .checked_add(Uint128::from(delta as u128))
            .map_err(ContractError::from)
    } else {
        value
            .checked_sub(Uint128::from((-delta) as u128))
            .map_err(ContractError::from)
    }
}

fn update_tick_state(
    storage: &mut dyn cosmwasm_std::Storage,
    tick: i64,
    liquidity_delta: i128,
    upper: bool,
) -> Result<(), ContractError> {
    let current_tick = SLOT0.load(storage)?.tick;
    let fee_growth_global_0_x128 = FEE_GROWTH_GLOBAL_0_X128.load(storage)?;
    let fee_growth_global_1_x128 = FEE_GROWTH_GLOBAL_1_X128.load(storage)?;
    let mut info = TICKS.may_load(storage, tick)?.unwrap_or(TickInfo {
        initialized: false,
        liquidity_gross: Uint128::zero(),
        liquidity_net: 0,
        fee_growth_outside_0_x128: Uint256::zero(),
        fee_growth_outside_1_x128: Uint256::zero(),
    });
    let was_initialized = !info.liquidity_gross.is_zero();

    info.liquidity_gross = add_signed_liquidity(info.liquidity_gross, liquidity_delta)?;
    if upper {
        info.liquidity_net = info
            .liquidity_net
            .checked_sub(liquidity_delta)
            .ok_or_else(|| ContractError::new("liquidity net overflow"))?;
    } else {
        info.liquidity_net = info
            .liquidity_net
            .checked_add(liquidity_delta)
            .ok_or_else(|| ContractError::new("liquidity net overflow"))?;
    }

    let is_initialized = !info.liquidity_gross.is_zero();
    if !was_initialized && is_initialized {
        info.initialized = true;
        if tick <= current_tick {
            info.fee_growth_outside_0_x128 = fee_growth_global_0_x128;
            info.fee_growth_outside_1_x128 = fee_growth_global_1_x128;
        }
    }

    if is_initialized {
        TICKS.save(storage, tick, &info)?;
    } else {
        TICKS.remove(storage, tick);
    }

    Ok(())
}

fn apply_liquidity_delta(
    storage: &mut dyn cosmwasm_std::Storage,
    lower_tick_index: i64,
    upper_tick_index: i64,
    liquidity_delta: i128,
) -> Result<(), ContractError> {
    update_tick_state(storage, lower_tick_index, liquidity_delta, false)?;
    update_tick_state(storage, upper_tick_index, liquidity_delta, true)?;

    let current_tick = SLOT0.load(storage)?.tick;
    if lower_tick_index <= current_tick && current_tick < upper_tick_index {
        let liquidity = ACTIVE_LIQUIDITY.load(storage)?;
        ACTIVE_LIQUIDITY.save(storage, &add_signed_liquidity(liquidity, liquidity_delta)?)?;
    }
    Ok(())
}

fn settle_position_fees(
    storage: &dyn cosmwasm_std::Storage,
    position: &mut ConcentratedPosition,
) -> Result<(), ContractError> {
    let slot0 = SLOT0.load(storage)?;
    let fee_growth_global_0_x128 = FEE_GROWTH_GLOBAL_0_X128.load(storage)?;
    let fee_growth_global_1_x128 = FEE_GROWTH_GLOBAL_1_X128.load(storage)?;
    let (inside_0, inside_1) = fee_growth_inside(
        slot0.tick,
        position.lower_tick_index,
        position.upper_tick_index,
        fee_growth_global_0_x128,
        fee_growth_global_1_x128,
        TICKS.may_load(storage, position.lower_tick_index)?,
        TICKS.may_load(storage, position.upper_tick_index)?,
    )?;
    let owed_0 = fees_owed(
        position.liquidity,
        inside_0,
        position.fee_growth_inside_0_last_x128,
    )?;
    let owed_1 = fees_owed(
        position.liquidity,
        inside_1,
        position.fee_growth_inside_1_last_x128,
    )?;

    position.tokens_owed_0 = position.tokens_owed_0.checked_add(owed_0)?;
    position.tokens_owed_1 = position.tokens_owed_1.checked_add(owed_1)?;
    position.fee_growth_inside_0_last_x128 = inside_0;
    position.fee_growth_inside_1_last_x128 = inside_1;
    Ok(())
}

fn find_next_initialized_tick(
    storage: &dyn cosmwasm_std::Storage,
    current_tick: i64,
    zero_for_one: bool,
) -> Result<(i64, bool), ContractError> {
    if zero_for_one {
        let mut iter = TICKS.range(
            storage,
            None,
            Some(cw_storage_plus::Bound::inclusive(current_tick)),
            cosmwasm_std::Order::Descending,
        );
        if let Some(item) = iter.next() {
            let (tick, info) = item?;
            return Ok((tick, info.initialized));
        }
        Ok((MIN_TICK, false))
    } else {
        let mut iter = TICKS.range(
            storage,
            Some(cw_storage_plus::Bound::exclusive(current_tick)),
            None,
            cosmwasm_std::Order::Ascending,
        );
        if let Some(item) = iter.next() {
            let (tick, info) = item?;
            return Ok((tick, info.initialized));
        }
        Ok((MAX_TICK, false))
    }
}

fn run_swap_simulation(
    deps: Deps,
    asset_in: Token,
    amount_in: Uint128,
    _test_fail: Option<bool>,
    sqrt_price_limit_x96: Option<Uint256>,
) -> Result<SwapSimulation, ContractError> {
    #[cfg(test)]
    ensure!(
        !_test_fail.unwrap_or(false),
        ContractError::new("Force fail flag")
    );
    ensure!(!amount_in.is_zero(), ContractError::ZeroAssetAmount {});

    let state = STATE.load(deps.storage)?;
    ensure!(
        asset_in.exists(state.pair.clone()),
        ContractError::AssetDoesNotExist {}
    );
    let asset_out = state.pair.get_other_token(asset_in.clone());
    let zero_for_one = asset_in == state.pair.token_1;

    let pool_key = POOL_KEY.load(deps.storage)?;
    let fee_pips = match pool_key.pool_type {
        PoolType::Concentrated { fee_tier_bps, .. } => fee_tier_bps,
        _ => return Err(ContractError::new("invalid pool type")),
    };
    ensure!(
        fee_pips < FEE_DENOMINATOR_PIPS,
        ContractError::new("invalid fee tier")
    );

    let mut slot0 = SLOT0.load(deps.storage)?;
    let mut liquidity = ACTIVE_LIQUIDITY.load(deps.storage)?;
    let mut fee_growth_global_0_x128 = FEE_GROWTH_GLOBAL_0_X128.load(deps.storage)?;
    let mut fee_growth_global_1_x128 = FEE_GROWTH_GLOBAL_1_X128.load(deps.storage)?;
    let mut protocol_fees_0 = PROTOCOL_FEES_0.load(deps.storage)?;
    let mut protocol_fees_1 = PROTOCOL_FEES_1.load(deps.storage)?;

    ensure!(
        !liquidity.is_zero(),
        ContractError::new("no active liquidity")
    );

    // Validate sqrt_price_limit_x96 if provided
    if let Some(limit) = sqrt_price_limit_x96 {
        if zero_for_one {
            ensure!(
                limit < slot0.sqrt_price_x96,
                ContractError::new(
                    "sqrt_price_limit_x96 must be less than current price for zero_for_one"
                )
            );
            ensure!(
                limit > min_sqrt_ratio(),
                ContractError::new("sqrt_price_limit_x96 must be greater than min_sqrt_ratio")
            );
        } else {
            ensure!(
                limit > slot0.sqrt_price_x96,
                ContractError::new(
                    "sqrt_price_limit_x96 must be greater than current price for one_for_zero"
                )
            );
            ensure!(
                limit < max_sqrt_ratio(),
                ContractError::new("sqrt_price_limit_x96 must be less than max_sqrt_ratio")
            );
        }
    }

    let mut amount_remaining = Uint256::from(amount_in.u128());
    let mut amount_out_total = Uint256::zero();
    let mut lp_fee_total = Uint256::zero();
    let mut protocol_fee_total = Uint256::zero();
    let mut crossed_ticks: Vec<CrossedTickUpdate> = Vec::new();
    let protocol_cut_bps = state.fee.euclid_fee_bps.min(10_000);
    let mut last_crossed_tick: Option<i64> = None;

    for _ in 0..MAX_SWAP_STEPS {
        if amount_remaining.is_zero() {
            break;
        }
        if let Some(limit) = sqrt_price_limit_x96 {
            let at_limit = if zero_for_one {
                slot0.sqrt_price_x96 <= limit
            } else {
                slot0.sqrt_price_x96 >= limit
            };
            if at_limit {
                break;
            }
        }
        ensure!(
            !liquidity.is_zero(),
            ContractError::new("insufficient liquidity")
        );

        let (next_tick, initialized) =
            find_next_initialized_tick(deps.storage, slot0.tick, zero_for_one)?;
        let target_tick = if zero_for_one {
            next_tick.max(MIN_TICK)
        } else {
            next_tick.min(MAX_TICK)
        };
        let sqrt_target = get_sqrt_ratio_at_tick(target_tick)?;
        // Clamp target to price limit; track whether we clamped to avoid
        // crossing the tick when the step ends at the limit rather than the tick.
        let (sqrt_target, clamped_to_limit) = if let Some(limit) = sqrt_price_limit_x96 {
            if zero_for_one {
                let clamped = sqrt_target.max(limit);
                (clamped, clamped != sqrt_target)
            } else {
                let clamped = sqrt_target.min(limit);
                (clamped, clamped != sqrt_target)
            }
        } else {
            (sqrt_target, false)
        };

        let step = compute_swap_step_exact_input(
            slot0.sqrt_price_x96,
            sqrt_target,
            liquidity,
            amount_remaining,
            fee_pips,
        )?;

        let consumed = step.amount_in.checked_add(step.fee_amount)?;
        ensure!(
            consumed <= amount_remaining,
            ContractError::new("swap step over-consumed input")
        );
        amount_remaining = amount_remaining.checked_sub(consumed)?;
        amount_out_total = amount_out_total.checked_add(step.amount_out)?;

        // Protocol fee truncates down; the remainder accrues to LPs.
        // A second truncation in fee_growth accumulation (fee * 2^128 / liquidity)
        // means a small amount of dust per swap step is unclaimable by either party
        // and remains locked in reserves. This matches Uniswap V3 behavior.
        let protocol_fee_step = step
            .fee_amount
            .checked_mul(Uint256::from(protocol_cut_bps as u128))?
            .checked_div(Uint256::from(10_000u128))?;
        let lp_fee_step = step.fee_amount.checked_sub(protocol_fee_step)?;
        lp_fee_total = lp_fee_total.checked_add(lp_fee_step)?;
        protocol_fee_total = protocol_fee_total.checked_add(protocol_fee_step)?;

        if zero_for_one {
            fee_growth_global_0_x128 =
                accumulate_fee_growth(fee_growth_global_0_x128, lp_fee_step, liquidity)?;
        } else {
            fee_growth_global_1_x128 =
                accumulate_fee_growth(fee_growth_global_1_x128, lp_fee_step, liquidity)?;
        }

        if !protocol_fee_step.is_zero() {
            let protocol_fee_step_u128 = Uint128::try_from(protocol_fee_step)
                .map_err(|_| ContractError::new("protocol fee overflow"))?;
            if zero_for_one {
                protocol_fees_0 = protocol_fees_0.checked_add(protocol_fee_step_u128)?;
            } else {
                protocol_fees_1 = protocol_fees_1.checked_add(protocol_fee_step_u128)?;
            }
        }

        let reached_target = step.sqrt_ratio_next_x96 == sqrt_target;
        slot0.sqrt_price_x96 = step.sqrt_ratio_next_x96;

        // Only cross the tick if we reached the tick target (not clamped to price limit).
        // When clamped, the step ended at the limit price — not at a tick boundary.
        if reached_target && !clamped_to_limit {
            if initialized {
                if let Some(info) = TICKS.may_load(deps.storage, target_tick)? {
                    let new_fee_growth_outside_0_x128 = flip_fee_growth_outside(
                        fee_growth_global_0_x128,
                        info.fee_growth_outside_0_x128,
                    );
                    let new_fee_growth_outside_1_x128 = flip_fee_growth_outside(
                        fee_growth_global_1_x128,
                        info.fee_growth_outside_1_x128,
                    );
                    crossed_ticks.push(CrossedTickUpdate {
                        tick: target_tick,
                        fee_growth_outside_0_x128: new_fee_growth_outside_0_x128,
                        fee_growth_outside_1_x128: new_fee_growth_outside_1_x128,
                    });
                    let liq_net = if zero_for_one {
                        -info.liquidity_net
                    } else {
                        info.liquidity_net
                    };
                    liquidity = add_signed_liquidity(liquidity, liq_net)?;
                }
            }
            last_crossed_tick = Some(target_tick);
            slot0.tick = if zero_for_one {
                target_tick.saturating_sub(1)
            } else {
                target_tick
            };
        } else {
            let new_tick = get_tick_at_sqrt_ratio(slot0.sqrt_price_x96)?;
            // After crossing tick T, slot0.tick is set to T-1 (zero_for_one) or T
            // (!zero_for_one) and ACTIVE_LIQUIDITY is adjusted accordingly. If the
            // next step barely moves the price, get_tick_at_sqrt_ratio can round
            // back to T, creating a tick/liquidity desync where the position appears
            // in-range but its liquidity isn't in ACTIVE_LIQUIDITY. Clamp against
            // only the immediately preceding crossed tick to prevent this.
            slot0.tick = match last_crossed_tick {
                Some(crossed) if zero_for_one && new_tick == crossed => crossed - 1,
                Some(crossed) if !zero_for_one && new_tick == crossed - 1 => crossed,
                _ => new_tick,
            };
        }
    }

    if sqrt_price_limit_x96.is_none() {
        ensure!(
            amount_remaining.is_zero(),
            ContractError::new("insufficient range liquidity for amount in")
        );
    }

    let amount_out = Uint128::try_from(amount_out_total)
        .map_err(|_| ContractError::new("amount out overflow"))?;
    let lp_fee =
        Uint128::try_from(lp_fee_total).map_err(|_| ContractError::new("lp fee overflow"))?;
    let protocol_fee = Uint128::try_from(protocol_fee_total)
        .map_err(|_| ContractError::new("protocol fee overflow"))?;

    Ok(SwapSimulation {
        state,
        slot0,
        liquidity,
        fee_growth_global_0_x128,
        fee_growth_global_1_x128,
        protocol_fees_0,
        protocol_fees_1,
        amount_out,
        asset_out,
        lp_fee,
        protocol_fee,
        crossed_ticks,
    })
}

fn execute_clp_swap(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    swap_msg: VlpSwapMsg,
) -> Result<Response, ContractError> {
    let simulation = run_swap_simulation(
        deps.as_ref(),
        swap_msg.asset_in.clone(),
        swap_msg.amount_in,
        swap_msg.test_fail,
        None,
    )?;
    let mut response = Response::new();

    let voucher_sender = if info.sender == simulation.state.router {
        swap_msg.sender.clone()
    } else {
        CrossChainUser {
            address: info.sender.to_string(),
            chain_uid: ChainUid::vsl_chain_uid()?,
        }
    };

    response = response.add_message(swap_msg.asset_in.create_voucher_transfer_msg(
        simulation.state.virtual_balance_contract.to_string(),
        swap_msg.amount_in,
        None,
        CrossChainUser {
            address: env.contract.address.to_string(),
            chain_uid: ChainUid::vsl_chain_uid()?,
        },
        Some(voucher_sender),
        None,
    )?);

    let mut reserve_in = BALANCES.load(deps.storage, swap_msg.asset_in.clone())?;
    let mut reserve_out = BALANCES.load(deps.storage, simulation.asset_out.clone())?;
    reserve_in = reserve_in.checked_add(swap_msg.amount_in)?;
    reserve_out = reserve_out.checked_sub(simulation.amount_out)?;
    BALANCES.save(deps.storage, swap_msg.asset_in.clone(), &reserve_in)?;
    BALANCES.save(deps.storage, simulation.asset_out.clone(), &reserve_out)?;

    for crossed in simulation.crossed_ticks {
        if let Some(mut tick) = TICKS.may_load(deps.storage, crossed.tick)? {
            tick.fee_growth_outside_0_x128 = crossed.fee_growth_outside_0_x128;
            tick.fee_growth_outside_1_x128 = crossed.fee_growth_outside_1_x128;
            TICKS.save(deps.storage, crossed.tick, &tick)?;
        }
    }

    SLOT0.save(deps.storage, &simulation.slot0)?;
    ACTIVE_LIQUIDITY.save(deps.storage, &simulation.liquidity)?;
    FEE_GROWTH_GLOBAL_0_X128.save(deps.storage, &simulation.fee_growth_global_0_x128)?;
    FEE_GROWTH_GLOBAL_1_X128.save(deps.storage, &simulation.fee_growth_global_1_x128)?;
    PROTOCOL_FEES_0.save(deps.storage, &simulation.protocol_fees_0)?;
    PROTOCOL_FEES_1.save(deps.storage, &simulation.protocol_fees_1)?;
    write_observation(
        deps.storage,
        env.block.time.seconds(),
        simulation.slot0.tick,
        simulation.liquidity,
    )?;

    let mut state = simulation.state.clone();
    state
        .total_fees_collected
        .lp_fees
        .add_fee(swap_msg.asset_in.to_string(), simulation.lp_fee);
    state
        .total_fees_collected
        .euclid_fees
        .add_fee(swap_msg.asset_in.to_string(), simulation.protocol_fee);
    STATE.save(deps.storage, &state)?;

    let swap_response = VlpSwapResponse {
        sender: swap_msg.sender.clone(),
        tx_id: swap_msg.tx_id.clone(),
        asset_out: simulation.asset_out.clone(),
        amount_out: simulation.amount_out,
    };

    match swap_msg.next_swaps.split_first() {
        Some((next_swap, forward_swaps)) => {
            let approve_msg = cosmwasm_std::WasmMsg::Execute {
                contract_addr: state.virtual_balance_contract.to_string(),
                msg: to_json_binary(&euclid::msgs::virtual_balance::msg::ExecuteMsg::Approve(
                    euclid::msgs::virtual_balance::msg::ExecuteApprove {
                        amount: swap_response.amount_out,
                        token_id: swap_response.asset_out.to_string(),
                        owner: CrossChainUser {
                            address: env.contract.address.to_string(),
                            chain_uid: ChainUid::vsl_chain_uid()?,
                        },
                        spender: CrossChainUser {
                            address: next_swap.vlp_address.clone(),
                            chain_uid: ChainUid::vsl_chain_uid()?,
                        },
                    },
                ))?,
                funds: vec![],
            };
            let next_swap_msg = WasmMsg::Execute {
                contract_addr: next_swap.vlp_address.clone(),
                msg: to_json_binary(&euclid::msgs::vlp::cp::msg::ExecuteMsg::Swap(VlpSwapMsg {
                    sender: swap_msg.sender.clone(),
                    tx_id: swap_msg.tx_id.clone(),
                    asset_in: swap_response.asset_out.clone(),
                    amount_in: swap_response.amount_out,
                    min_token_out: swap_msg.min_token_out,
                    next_swaps: forward_swaps.to_vec(),
                    test_fail: next_swap.test_fail,
                }))?,
                funds: vec![],
            };
            response = response
                .add_message(approve_msg)
                .add_submessage(SubMsg::reply_always(next_swap_msg, NEXT_SWAP_REPLY_ID));
        }
        None => {
            ensure!(
                swap_response.amount_out >= swap_msg.min_token_out,
                ContractError::SlippageExceeded {
                    amount: swap_response.amount_out,
                    min_amount_out: swap_msg.min_token_out
                }
            );
            response = response.add_message(simulation.asset_out.create_voucher_transfer_msg(
                state.virtual_balance_contract.to_string(),
                swap_response.amount_out,
                None,
                swap_msg.sender.clone(),
                None,
                None,
            )?);
        }
    };

    Ok(response
        .add_event(tx_event(
            &swap_msg.tx_id,
            &swap_msg.sender.to_sender_string(),
            TxType::Swap,
        ))
        .add_event(liquidity_event(
            &[
                swap_msg.asset_in.with_amount(reserve_in),
                simulation.asset_out.with_amount(reserve_out),
            ],
            &[
                swap_msg.asset_in.with_amount(swap_msg.amount_in),
                simulation.asset_out.with_amount(simulation.amount_out),
            ],
            &swap_msg.tx_id,
        ))
        .add_attribute("action", "swap")
        .add_attribute("amount_in", swap_msg.amount_in)
        .add_attribute("amount_out", simulation.amount_out)
        .add_attribute("asset_in", swap_msg.asset_in.to_string())
        .add_attribute("asset_out", simulation.asset_out.to_string())
        .set_data(to_json_binary(&swap_response)?))
}

fn query_clp_simulate_swap(
    deps: Deps,
    asset_in: Token,
    amount_in: Uint128,
    next_swaps: Vec<NextSwapVlp>,
) -> Result<Binary, ContractError> {
    let sim = run_swap_simulation(deps, asset_in, amount_in, None, None)?;
    let response = GetSwapQueryResponse {
        amount_out: sim.amount_out,
        asset_out: sim.asset_out,
        spread_amount: Uint128::zero(),
        lp_fee: sim.lp_fee,
        euclid_fee: sim.protocol_fee,
    };
    match next_swaps.split_first() {
        Some((next_swap, forward_swaps)) => {
            let next_response: GetSwapQueryResponse = deps.querier.query_wasm_smart(
                next_swap.vlp_address.clone(),
                &euclid::msgs::vlp::concentrated::msg::QueryMsg::SimulateSwap(
                    euclid::msgs::vlp::base::VlpSimulateSwapMsg {
                        asset: response.asset_out,
                        asset_amount: response.amount_out,
                        swaps: forward_swaps.to_vec(),
                    },
                ),
            )?;
            Ok(to_json_binary(&next_response)?)
        }
        None => Ok(to_json_binary(&response)?),
    }
}

fn execute_collect_fees(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: euclid::msgs::vlp::base::VlpConcentratedCollectFeesMsg,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.router, ContractError::Unauthorized {});

    let mut position = POSITIONS
        .may_load(deps.storage, msg.position_id.u128())?
        .ok_or_else(|| ContractError::new("position not found"))?;
    ensure!(
        position.chain_uid == msg.sender.chain_uid,
        ContractError::Unauthorized {}
    );

    settle_position_fees(deps.storage, &mut position)?;

    let amount_0 = position.tokens_owed_0;
    let amount_1 = position.tokens_owed_1;
    position.tokens_owed_0 = Uint128::zero();
    position.tokens_owed_1 = Uint128::zero();
    if position.liquidity.is_zero() {
        POSITIONS.remove(deps.storage, msg.position_id.u128());
    } else {
        POSITIONS.save(deps.storage, msg.position_id.u128(), &position)?;
    }

    let mut reserve_0 = BALANCES.load(deps.storage, state.pair.token_1.clone())?;
    let mut reserve_1 = BALANCES.load(deps.storage, state.pair.token_2.clone())?;
    reserve_0 = reserve_0.checked_sub(amount_0)?;
    reserve_1 = reserve_1.checked_sub(amount_1)?;
    BALANCES.save(deps.storage, state.pair.token_1.clone(), &reserve_0)?;
    BALANCES.save(deps.storage, state.pair.token_2.clone(), &reserve_1)?;

    let mut response = Response::new().add_attribute("action", "collect_fees");
    if !amount_0.is_zero() {
        response = response.add_message(state.pair.token_1.create_voucher_transfer_msg(
            state.virtual_balance_contract.to_string(),
            amount_0,
            None,
            msg.recipient.clone(),
            None,
            None,
        )?);
    }
    if !amount_1.is_zero() {
        response = response.add_message(state.pair.token_2.create_voucher_transfer_msg(
            state.virtual_balance_contract.to_string(),
            amount_1,
            None,
            msg.recipient.clone(),
            None,
            None,
        )?);
    }

    let ack = VlpConcentratedCollectFeesResponse {
        pool_key: msg.pool_key,
        position_id: msg.position_id,
        amount_0,
        amount_1,
        tx_id: msg.tx_id,
        sender: msg.sender,
        recipient: msg.recipient,
        vlp_address: env.contract.address.to_string(),
    };
    Ok(response.set_data(to_json_binary(&ack)?))
}

fn execute_collect_protocol_fees(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: euclid::msgs::vlp::base::VlpConcentratedCollectProtocolFeesMsg,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.router, ContractError::Unauthorized {});

    let admins = ADMIN.load(deps.storage)?;
    ensure!(
        msg.sender.address == admins.fee_admin.to_string(),
        ContractError::Unauthorized {}
    );

    let mut protocol_fees_0 = PROTOCOL_FEES_0.load(deps.storage)?;
    let mut protocol_fees_1 = PROTOCOL_FEES_1.load(deps.storage)?;
    let amount_0 = protocol_fees_0.min(msg.amount_0_requested);
    let amount_1 = protocol_fees_1.min(msg.amount_1_requested);
    protocol_fees_0 = protocol_fees_0.checked_sub(amount_0)?;
    protocol_fees_1 = protocol_fees_1.checked_sub(amount_1)?;
    PROTOCOL_FEES_0.save(deps.storage, &protocol_fees_0)?;
    PROTOCOL_FEES_1.save(deps.storage, &protocol_fees_1)?;

    let mut reserve_0 = BALANCES.load(deps.storage, state.pair.token_1.clone())?;
    let mut reserve_1 = BALANCES.load(deps.storage, state.pair.token_2.clone())?;
    reserve_0 = reserve_0.checked_sub(amount_0)?;
    reserve_1 = reserve_1.checked_sub(amount_1)?;
    BALANCES.save(deps.storage, state.pair.token_1.clone(), &reserve_0)?;
    BALANCES.save(deps.storage, state.pair.token_2.clone(), &reserve_1)?;

    let mut response = Response::new().add_attribute("action", "collect_protocol_fees");
    if !amount_0.is_zero() {
        response = response.add_message(state.pair.token_1.create_voucher_transfer_msg(
            state.virtual_balance_contract.to_string(),
            amount_0,
            None,
            msg.recipient.clone(),
            None,
            None,
        )?);
    }
    if !amount_1.is_zero() {
        response = response.add_message(state.pair.token_2.create_voucher_transfer_msg(
            state.virtual_balance_contract.to_string(),
            amount_1,
            None,
            msg.recipient.clone(),
            None,
            None,
        )?);
    }

    let ack = VlpConcentratedCollectProtocolFeesResponse {
        pool_key: msg.pool_key,
        amount_0,
        amount_1,
        tx_id: msg.tx_id,
        sender: msg.sender,
        recipient: msg.recipient,
        vlp_address: env.contract.address.to_string(),
    };
    Ok(response.set_data(to_json_binary(&ack)?))
}

fn increase_observation_cardinality_next(
    deps: DepsMut,
    info: MessageInfo,
    observation_cardinality_next: u16,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    let admins = ADMIN.load(deps.storage)?;
    ensure!(
        info.sender == state.router || info.sender == admins.general_admin,
        ContractError::Unauthorized {}
    );
    let mut slot0 = SLOT0.load(deps.storage)?;
    if observation_cardinality_next > slot0.observation_cardinality_next {
        slot0.observation_cardinality_next = observation_cardinality_next;
        SLOT0.save(deps.storage, &slot0)?;
    }
    Ok(Response::new()
        .add_attribute("action", "increase_observation_cardinality_next")
        .add_attribute(
            "observation_cardinality_next",
            slot0.observation_cardinality_next.to_string(),
        ))
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

fn query_slot0(deps: Deps) -> Result<Slot0Response, ContractError> {
    let slot0 = SLOT0.load(deps.storage)?;
    Ok(Slot0Response {
        sqrt_price_x96: slot0.sqrt_price_x96,
        tick: slot0.tick,
        observation_index: slot0.observation_index,
        observation_cardinality: slot0.observation_cardinality,
        observation_cardinality_next: slot0.observation_cardinality_next,
        liquidity: ACTIVE_LIQUIDITY.load(deps.storage)?,
        fee_growth_global_0_x128: FEE_GROWTH_GLOBAL_0_X128.load(deps.storage)?,
        fee_growth_global_1_x128: FEE_GROWTH_GLOBAL_1_X128.load(deps.storage)?,
    })
}

fn query_position(deps: Deps, position_id: Uint128) -> Result<PositionResponse, ContractError> {
    let position = POSITIONS
        .may_load(deps.storage, position_id.u128())?
        .ok_or_else(|| ContractError::new("position not found"))?;
    Ok(PositionResponse {
        position_id,
        chain_uid: position.chain_uid,
        lower_tick_index: position.lower_tick_index,
        upper_tick_index: position.upper_tick_index,
        liquidity: position.liquidity,
        fee_growth_inside_0_last_x128: position.fee_growth_inside_0_last_x128,
        fee_growth_inside_1_last_x128: position.fee_growth_inside_1_last_x128,
        tokens_owed_0: position.tokens_owed_0,
        tokens_owed_1: position.tokens_owed_1,
    })
}

fn query_tick(deps: Deps, index: i64) -> Result<TickResponse, ContractError> {
    let tick = TICKS.may_load(deps.storage, index)?.unwrap_or(TickInfo {
        initialized: false,
        liquidity_gross: Uint128::zero(),
        liquidity_net: 0,
        fee_growth_outside_0_x128: Uint256::zero(),
        fee_growth_outside_1_x128: Uint256::zero(),
    });
    Ok(TickResponse {
        index,
        initialized: tick.initialized,
        liquidity_gross: tick.liquidity_gross,
        liquidity_net: tick.liquidity_net,
        fee_growth_outside_0_x128: tick.fee_growth_outside_0_x128,
        fee_growth_outside_1_x128: tick.fee_growth_outside_1_x128,
    })
}

fn query_ticks(
    deps: Deps,
    start_after: Option<i64>,
    limit: u32,
) -> Result<TicksResponse, ContractError> {
    let ticks: Result<Vec<_>, ContractError> = TICKS
        .range(
            deps.storage,
            start_after.map(cw_storage_plus::Bound::exclusive),
            None,
            cosmwasm_std::Order::Ascending,
        )
        .take(limit as usize)
        .map(|item| {
            let (index, tick) = item?;
            Ok(TickResponse {
                index,
                initialized: tick.initialized,
                liquidity_gross: tick.liquidity_gross,
                liquidity_net: tick.liquidity_net,
                fee_growth_outside_0_x128: tick.fee_growth_outside_0_x128,
                fee_growth_outside_1_x128: tick.fee_growth_outside_1_x128,
            })
        })
        .collect();
    Ok(TicksResponse { ticks: ticks? })
}

fn query_observe(
    deps: Deps,
    now: u64,
    seconds_agos: Vec<u64>,
) -> Result<ObserveResponse, ContractError> {
    let (tick_cumulatives, seconds_per_liquidity_cumulative_x128s) =
        observe(deps.storage, now, seconds_agos)?;
    Ok(ObserveResponse {
        tick_cumulatives,
        seconds_per_liquidity_cumulative_x128s,
    })
}

fn query_protocol_fees(deps: Deps) -> Result<ProtocolFeesResponse, ContractError> {
    Ok(ProtocolFeesResponse {
        amount_0: PROTOCOL_FEES_0.load(deps.storage)?,
        amount_1: PROTOCOL_FEES_1.load(deps.storage)?,
    })
}

fn query_migration_status(deps: Deps) -> Result<MigrationStatusResponse, ContractError> {
    let revision = MIGRATION_REVISION.may_load(deps.storage)?.unwrap_or(0);
    let metadata = MIGRATION_METADATA.may_load(deps.storage)?;
    let state = STATE.load(deps.storage)?;

    let (source_version, mode, migrated_at, positions_migrated) = metadata.map_or(
        (
            "unknown".to_string(),
            LegacyLiquidityMode::AlreadyV3Liquidity,
            0u64,
            0u64,
        ),
        |meta| {
            (
                meta.source_version,
                meta.mode,
                meta.migrated_at,
                meta.positions_migrated,
            )
        },
    );

    Ok(MigrationStatusResponse {
        revision,
        source_version,
        mode,
        migrated_at,
        positions_migrated,
        active_liquidity: ACTIVE_LIQUIDITY.may_load(deps.storage)?.unwrap_or_default(),
        total_liquidity: state.total_lp_tokens,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::tick_math::{get_sqrt_ratio_at_tick, max_sqrt_ratio, min_sqrt_ratio};
    use crate::state::*;
    use cosmwasm_std::testing::mock_dependencies;
    use cosmwasm_std::{Addr, Uint128, Uint256};
    use euclid::fee::{DenomFees, Fee, TotalFees};
    use euclid::msgs::vlp::base::{PoolKey, PoolType};
    use euclid::token::Pair;

    /// Set up minimal contract state for `run_swap_simulation` tests.
    /// Places liquidity across a tick range centered on tick 0.
    fn setup_pool(
        deps: &mut cosmwasm_std::OwnedDeps<
            cosmwasm_std::MemoryStorage,
            cosmwasm_std::testing::MockApi,
            cosmwasm_std::testing::MockQuerier,
        >,
    ) {
        let token_a = Token::create("alpha".to_string()).expect("token");
        let token_b = Token::create("beta".to_string()).expect("token");
        let pair = Pair::new(token_a.clone(), token_b.clone()).expect("pair");

        let state = State {
            pair: pair.clone(),
            router: Addr::unchecked("router"),
            virtual_balance_contract: Addr::unchecked("vb"),
            fee: Fee::new(
                3000,
                1000,
                CrossChainUser {
                    chain_uid: ChainUid::create("vsl".to_string()).expect("chain"),
                    address: "fee_recipient".to_string(),
                },
            ),
            total_fees_collected: TotalFees {
                lp_fees: DenomFees {
                    totals: HashMap::new(),
                },
                euclid_fees: DenomFees {
                    totals: HashMap::new(),
                },
            },
            last_updated: 0,
            total_lp_tokens: Uint128::zero(),
        };
        STATE
            .save(deps.as_mut().storage, &state)
            .expect("save state");

        let pool_key = PoolKey {
            pair: pair.clone(),
            pool_type: PoolType::Concentrated {
                fee_tier_bps: 3000, // 0.3%
                tick_spacing: 60,
            },
        };
        POOL_KEY
            .save(deps.as_mut().storage, &pool_key)
            .expect("save pool_key");

        // Price at tick 0 = 1:1
        let sqrt_price = get_sqrt_ratio_at_tick(0).expect("sqrt at 0");
        let slot0 = Slot0 {
            sqrt_price_x96: sqrt_price,
            tick: 0,
            observation_index: 0,
            observation_cardinality: 1,
            observation_cardinality_next: 1,
        };
        SLOT0
            .save(deps.as_mut().storage, &slot0)
            .expect("save slot0");

        let liquidity = Uint128::new(10_000_000_000);
        ACTIVE_LIQUIDITY
            .save(deps.as_mut().storage, &liquidity)
            .expect("save liq");

        FEE_GROWTH_GLOBAL_0_X128
            .save(deps.as_mut().storage, &Uint256::zero())
            .expect("save fg0");
        FEE_GROWTH_GLOBAL_1_X128
            .save(deps.as_mut().storage, &Uint256::zero())
            .expect("save fg1");
        PROTOCOL_FEES_0
            .save(deps.as_mut().storage, &Uint128::zero())
            .expect("save pf0");
        PROTOCOL_FEES_1
            .save(deps.as_mut().storage, &Uint128::zero())
            .expect("save pf1");

        // Place initialized ticks at -600 and +600 to create a liquidity range
        let lower_tick = -600i64;
        let upper_tick = 600i64;
        let tick_info = TickInfo {
            initialized: true,
            liquidity_gross: liquidity,
            liquidity_net: liquidity.u128() as i128,
            fee_growth_outside_0_x128: Uint256::zero(),
            fee_growth_outside_1_x128: Uint256::zero(),
        };
        TICKS
            .save(deps.as_mut().storage, lower_tick, &tick_info)
            .expect("save lower tick");
        let upper_info = TickInfo {
            initialized: true,
            liquidity_gross: liquidity,
            liquidity_net: -(liquidity.u128() as i128),
            fee_growth_outside_0_x128: Uint256::zero(),
            fee_growth_outside_1_x128: Uint256::zero(),
        };
        TICKS
            .save(deps.as_mut().storage, upper_tick, &upper_info)
            .expect("save upper tick");
    }

    // ------------------------------------------------------------------
    // sqrt_price_limit_x96 validation and swap loop clamping
    // ------------------------------------------------------------------

    struct PriceLimitValidationCase {
        name: &'static str,
        /// true = sell token_1 (price goes down), false = sell token_2 (price goes up)
        zero_for_one: bool,
        /// The price limit to pass
        limit: Uint256,
        /// Whether the call should succeed
        expect_ok: bool,
    }

    #[test]
    fn sqrt_price_limit_validation() {
        let mut deps = mock_dependencies();
        setup_pool(&mut deps);

        let slot0 = SLOT0.load(deps.as_ref().storage).expect("load slot0");
        let current_price = slot0.sqrt_price_x96;

        let cases = vec![
            // --- zero_for_one: limit must be < current and > min ---
            PriceLimitValidationCase {
                name: "z41: valid limit below current price",
                zero_for_one: true,
                limit: current_price - Uint256::one(),
                expect_ok: true,
            },
            PriceLimitValidationCase {
                name: "z41: limit equal to current price rejected",
                zero_for_one: true,
                limit: current_price,
                expect_ok: false,
            },
            PriceLimitValidationCase {
                name: "z41: limit above current price rejected",
                zero_for_one: true,
                limit: current_price + Uint256::one(),
                expect_ok: false,
            },
            PriceLimitValidationCase {
                name: "z41: limit at min_sqrt_ratio rejected",
                zero_for_one: true,
                limit: min_sqrt_ratio(),
                expect_ok: false,
            },
            // --- one_for_zero: limit must be > current and < max ---
            PriceLimitValidationCase {
                name: "1f0: valid limit above current price",
                zero_for_one: false,
                limit: current_price + Uint256::one(),
                expect_ok: true,
            },
            PriceLimitValidationCase {
                name: "1f0: limit equal to current price rejected",
                zero_for_one: false,
                limit: current_price,
                expect_ok: false,
            },
            PriceLimitValidationCase {
                name: "1f0: limit below current price rejected",
                zero_for_one: false,
                limit: current_price - Uint256::one(),
                expect_ok: false,
            },
            PriceLimitValidationCase {
                name: "1f0: limit at max_sqrt_ratio rejected",
                zero_for_one: false,
                limit: max_sqrt_ratio(),
                expect_ok: false,
            },
        ];

        let state = STATE.load(deps.as_ref().storage).expect("load state");

        for case in &cases {
            let asset_in = if case.zero_for_one {
                state.pair.token_1.clone()
            } else {
                state.pair.token_2.clone()
            };

            let result = run_swap_simulation(
                deps.as_ref(),
                asset_in,
                Uint128::new(1_000),
                None,
                Some(case.limit),
            );

            assert_eq!(
                result.is_ok(),
                case.expect_ok,
                "case '{}': expected ok={}, got {:?}",
                case.name,
                case.expect_ok,
                result.err()
            );
        }
    }

    #[test]
    fn swap_stops_at_price_limit() {
        let mut deps = mock_dependencies();
        setup_pool(&mut deps);

        let state = STATE.load(deps.as_ref().storage).expect("load state");
        let slot0 = SLOT0.load(deps.as_ref().storage).expect("load slot0");

        // Swap zero_for_one with a price limit slightly below current price.
        // The limit should cause the swap to stop early, producing less output
        // than a swap without a limit for the same input amount.
        let amount_in = Uint128::new(100_000);
        let asset_in = state.pair.token_1.clone();

        // Unlimited swap
        let unlimited = run_swap_simulation(deps.as_ref(), asset_in.clone(), amount_in, None, None)
            .expect("unlimited swap");

        // Limited swap — set limit near current price so it stops early
        // Use a price ~halfway between current and the lower tick
        let lower_sqrt = get_sqrt_ratio_at_tick(-300).expect("sqrt at -300");
        // Ensure limit is valid (below current, above min)
        assert!(
            lower_sqrt < slot0.sqrt_price_x96,
            "limit must be below current price"
        );
        assert!(lower_sqrt > min_sqrt_ratio(), "limit must be above min");

        let limited =
            run_swap_simulation(deps.as_ref(), asset_in, amount_in, None, Some(lower_sqrt))
                .expect("limited swap");

        // The limited swap should produce less or equal output
        assert!(
            limited.amount_out <= unlimited.amount_out,
            "limited swap output ({}) should be <= unlimited ({})",
            limited.amount_out,
            unlimited.amount_out
        );

        // The limited swap's final price should not have crossed the limit
        assert!(
            limited.slot0.sqrt_price_x96 >= lower_sqrt,
            "final price ({}) crossed below limit ({})",
            limited.slot0.sqrt_price_x96,
            lower_sqrt
        );
    }

    #[test]
    fn no_price_limit_requires_full_consumption() {
        let mut deps = mock_dependencies();
        setup_pool(&mut deps);

        let state = STATE.load(deps.as_ref().storage).expect("load state");

        // A very large swap with no price limit should fail if liquidity is exhausted
        let result = run_swap_simulation(
            deps.as_ref(),
            state.pair.token_1.clone(),
            Uint128::new(u128::MAX / 2),
            None,
            None,
        );

        match result {
            Ok(_) => panic!("unlimited swap with excessive input should fail"),
            Err(e) => {
                let err = e.to_string();
                assert!(
                    err.contains("insufficient") || err.contains("liquidity"),
                    "error should mention insufficient liquidity, got: {}",
                    err
                );
            }
        }
    }

    #[test]
    fn price_limit_allows_partial_fill() {
        let mut deps = mock_dependencies();
        setup_pool(&mut deps);

        let state = STATE.load(deps.as_ref().storage).expect("load state");
        let slot0 = SLOT0.load(deps.as_ref().storage).expect("load slot0");

        // Set a price limit partway between current price and the lower tick.
        // This should allow partial fill even with large input.
        let lower_sqrt = get_sqrt_ratio_at_tick(-100).expect("sqrt at -100");
        let tight_limit = lower_sqrt;
        assert!(tight_limit < slot0.sqrt_price_x96);
        assert!(tight_limit > min_sqrt_ratio());

        let result = run_swap_simulation(
            deps.as_ref(),
            state.pair.token_1.clone(),
            Uint128::new(1_000_000_000),
            None,
            Some(tight_limit),
        );

        // Should succeed (partial fill allowed with price limit)
        assert!(
            result.is_ok(),
            "swap with price limit should allow partial fill, got: {:?}",
            result.err()
        );

        let sim = result.expect("simulation");
        // Output should be small due to tight limit
        assert!(sim.amount_out > Uint128::zero(), "should have some output");
    }

    fn set_tick(storage: &mut dyn cosmwasm_std::Storage, tick: i64) {
        TICKS
            .save(
                storage,
                tick,
                &TickInfo {
                    initialized: true,
                    liquidity_gross: Uint128::new(1),
                    liquidity_net: 1,
                    fee_growth_outside_0_x128: Uint256::zero(),
                    fee_growth_outside_1_x128: Uint256::zero(),
                },
            )
            .unwrap();
    }

    struct Case {
        name: &'static str,
        initialized_ticks: &'static [i64],
        current_tick: i64,
        zero_for_one: bool,
        expected_tick: i64,
        expected_init: bool,
    }

    #[test]
    fn find_next_initialized_tick_cases() {
        let cases = [
            // --- Descending (zero_for_one = true) ---
            Case {
                name: "descending: finds nearest below",
                initialized_ticks: &[100, 500],
                current_tick: 600,
                zero_for_one: true,
                expected_tick: 500,
                expected_init: true,
            },
            Case {
                name: "descending: inclusive of current tick",
                initialized_ticks: &[200],
                current_tick: 200,
                zero_for_one: true,
                expected_tick: 200,
                expected_init: true,
            },
            Case {
                name: "descending: finds negative tick",
                initialized_ticks: &[-100],
                current_tick: 50,
                zero_for_one: true,
                expected_tick: -100,
                expected_init: true,
            },
            Case {
                name: "descending: returns MIN_TICK when empty",
                initialized_ticks: &[],
                current_tick: 500,
                zero_for_one: true,
                expected_tick: MIN_TICK,
                expected_init: false,
            },
            // --- Ascending (zero_for_one = false) ---
            Case {
                name: "ascending: finds nearest above",
                initialized_ticks: &[100, 500],
                current_tick: 50,
                zero_for_one: false,
                expected_tick: 100,
                expected_init: true,
            },
            Case {
                name: "ascending: excludes current tick",
                initialized_ticks: &[200, 300],
                current_tick: 200,
                zero_for_one: false,
                expected_tick: 300,
                expected_init: true,
            },
            Case {
                name: "ascending: includes tick above current",
                initialized_ticks: &[200],
                current_tick: 195,
                zero_for_one: false,
                expected_tick: 200,
                expected_init: true,
            },
            Case {
                name: "ascending: finds distant tick",
                initialized_ticks: &[2600],
                current_tick: 100,
                zero_for_one: false,
                expected_tick: 2600,
                expected_init: true,
            },
            Case {
                name: "ascending: returns MAX_TICK when empty",
                initialized_ticks: &[],
                current_tick: 500,
                zero_for_one: false,
                expected_tick: MAX_TICK,
                expected_init: false,
            },
            // --- Multiple ticks / negative ---
            Case {
                name: "descending: picks nearest among many",
                initialized_ticks: &[-500, -200, 100, 400, 700],
                current_tick: 300,
                zero_for_one: true,
                expected_tick: 100,
                expected_init: true,
            },
            Case {
                name: "ascending: picks nearest among many",
                initialized_ticks: &[-500, -200, 100, 400, 700],
                current_tick: 300,
                zero_for_one: false,
                expected_tick: 400,
                expected_init: true,
            },
            Case {
                name: "descending: negative ticks, nearest",
                initialized_ticks: &[-3000, -100],
                current_tick: -50,
                zero_for_one: true,
                expected_tick: -100,
                expected_init: true,
            },
            Case {
                name: "descending: negative ticks, skip nearest",
                initialized_ticks: &[-3000, -100],
                current_tick: -150,
                zero_for_one: true,
                expected_tick: -3000,
                expected_init: true,
            },
            Case {
                name: "ascending: skips ticks at current position",
                initialized_ticks: &[2550, 2560],
                current_tick: 2550,
                zero_for_one: false,
                expected_tick: 2560,
                expected_init: true,
            },
        ];

        for case in &cases {
            let mut deps = mock_dependencies();
            for &tick in case.initialized_ticks {
                set_tick(deps.as_mut().storage, tick);
            }
            let (tick, init) = find_next_initialized_tick(
                deps.as_ref().storage,
                case.current_tick,
                case.zero_for_one,
            )
            .unwrap_or_else(|e| panic!("{}: unexpected error: {}", case.name, e));

            assert_eq!(tick, case.expected_tick, "{}: wrong tick", case.name);
            assert_eq!(init, case.expected_init, "{}: wrong init flag", case.name);
        }
    }

    #[test]
    fn add_liquidity_rejects_mismatched_pool_key() {
        use cosmwasm_std::testing::mock_env;
        use euclid::msgs::vlp::base::VlpConcentratedAddLiquidityMsg;

        let mut deps = mock_dependencies();
        setup_pool(&mut deps);

        let state = STATE.load(deps.as_ref().storage).expect("load state");
        let stored_pool_key = POOL_KEY.load(deps.as_ref().storage).expect("load pool_key");

        // Build a pool_key that differs from the stored one (different fee tier)
        let wrong_pool_key = PoolKey {
            pair: stored_pool_key.pair.clone(),
            pool_type: PoolType::Concentrated {
                fee_tier_bps: 500, // stored is 3000
                tick_spacing: 10,
            },
        };

        let info = cosmwasm_std::testing::message_info(&Addr::unchecked("router"), &[]);

        let msg = VlpConcentratedAddLiquidityMsg {
            sender: CrossChainUser {
                chain_uid: ChainUid::create("vsl".to_string()).unwrap(),
                address: "alice".to_string(),
            },
            tx_id: "tx_bad_key".to_string(),
            pool_key: wrong_pool_key,
            liquidity: state
                .pair
                .get_pair_with_amount(Uint128::new(1000), Uint128::new(1000))
                .unwrap(),
            lower_tick_index: -600,
            upper_tick_index: 600,
            position_id: Uint128::new(1),
            slippage_tolerance_bps: 100,
        };

        let err =
            execute_add_concentrated_liquidity(deps.as_mut(), mock_env(), info, msg).unwrap_err();

        assert!(
            err.to_string().contains("pool key mismatch"),
            "expected pool key mismatch error, got: {}",
            err
        );
    }

    #[test]
    fn add_liquidity_rejects_wrong_token_pair() {
        use cosmwasm_std::testing::mock_env;
        use euclid::msgs::vlp::base::VlpConcentratedAddLiquidityMsg;
        use euclid::token::TokenWithAmount;

        let mut deps = mock_dependencies();
        setup_pool(&mut deps);

        let stored_pool_key = POOL_KEY.load(deps.as_ref().storage).expect("load pool_key");

        // Build liquidity with wrong tokens
        let wrong_token_a = Token::create("wrong1".to_string()).expect("token");
        let wrong_token_b = Token::create("wrong2".to_string()).expect("token");
        let wrong_pair = euclid::token::PairWithAmount::new(
            TokenWithAmount {
                token: wrong_token_a,
                amount: Uint128::new(1000),
            },
            TokenWithAmount {
                token: wrong_token_b,
                amount: Uint128::new(1000),
            },
        )
        .unwrap();

        let info = cosmwasm_std::testing::message_info(&Addr::unchecked("router"), &[]);

        let msg = VlpConcentratedAddLiquidityMsg {
            sender: CrossChainUser {
                chain_uid: ChainUid::create("vsl".to_string()).unwrap(),
                address: "alice".to_string(),
            },
            tx_id: "tx_bad_tokens".to_string(),
            pool_key: stored_pool_key,
            liquidity: wrong_pair,
            lower_tick_index: -600,
            upper_tick_index: 600,
            position_id: Uint128::new(1),
            slippage_tolerance_bps: 100,
        };

        let err =
            execute_add_concentrated_liquidity(deps.as_mut(), mock_env(), info, msg).unwrap_err();

        assert!(
            err.to_string()
                .contains("liquidity tokens do not match pool pair"),
            "expected liquidity tokens mismatch error, got: {}",
            err
        );
    }

    #[rstest::rstest]
    #[case::none_defaults_to_zero(None, 0)]
    #[case::positive_tick(Some(23027), 23027)]
    #[case::negative_tick(Some(-23027), -23027)]
    #[case::explicit_zero(Some(0), 0)]
    #[case::min_tick_boundary(Some(MIN_TICK), MIN_TICK)]
    #[case::max_tick_boundary(Some(MAX_TICK), MAX_TICK)]
    fn instantiate_initial_tick_ok(#[case] initial_tick: Option<i64>, #[case] expected_tick: i64) {
        use cosmwasm_std::testing::{message_info, mock_env};

        let mut deps = mock_dependencies();
        let info = message_info(&Addr::unchecked("router"), &[]);
        let pair = Pair::new(
            Token::create("alpha".to_string()).unwrap(),
            Token::create("beta".to_string()).unwrap(),
        )
        .unwrap();

        let msg = InstantiateMsg {
            virtual_balance_contract: Addr::unchecked("vb"),
            pair,
            fee: Fee::new(
                500,
                0,
                CrossChainUser {
                    chain_uid: ChainUid::create("vsl".to_string()).unwrap(),
                    address: "fee".to_string(),
                },
            ),
            execute: None,
            admin: Addr::unchecked("admin"),
            fee_tier_bps: 500,
            tick_spacing: 10,
            initial_tick,
        };

        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let slot0 = SLOT0.load(deps.as_ref().storage).unwrap();
        assert_eq!(slot0.tick, expected_tick);
        assert_eq!(
            slot0.sqrt_price_x96,
            get_sqrt_ratio_at_tick(expected_tick).unwrap()
        );
    }

    #[rstest::rstest]
    #[case::below_min(Some(MIN_TICK - 1))]
    #[case::above_max(Some(MAX_TICK + 1))]
    fn instantiate_initial_tick_out_of_bounds(#[case] initial_tick: Option<i64>) {
        use cosmwasm_std::testing::{message_info, mock_env};

        let mut deps = mock_dependencies();
        let info = message_info(&Addr::unchecked("router"), &[]);
        let pair = Pair::new(
            Token::create("alpha".to_string()).unwrap(),
            Token::create("beta".to_string()).unwrap(),
        )
        .unwrap();

        let msg = InstantiateMsg {
            virtual_balance_contract: Addr::unchecked("vb"),
            pair,
            fee: Fee::new(
                500,
                0,
                CrossChainUser {
                    chain_uid: ChainUid::create("vsl".to_string()).unwrap(),
                    address: "fee".to_string(),
                },
            ),
            execute: None,
            admin: Addr::unchecked("admin"),
            fee_tier_bps: 500,
            tick_spacing: 10,
            initial_tick,
        };

        let err = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert!(
            err.to_string().contains("initial_tick out of bounds"),
            "expected out of bounds error, got: {}",
            err
        );
    }

    #[test]
    fn query_pool_rejects_mismatched_pool_key() {
        use crate::query::query_pool;
        use euclid::token::Token;

        let mut deps = mock_dependencies();
        setup_pool(&mut deps);

        let state = STATE.load(deps.as_ref().storage).unwrap();
        let chain_uid = ChainUid::create("vsl".to_string()).unwrap();
        CHAIN_LP_TOKENS
            .save(deps.as_mut().storage, chain_uid.clone(), &Uint128::new(100))
            .unwrap();
        BALANCES
            .save(
                deps.as_mut().storage,
                state.pair.token_1.clone(),
                &Uint128::new(5000),
            )
            .unwrap();
        BALANCES
            .save(
                deps.as_mut().storage,
                state.pair.token_2.clone(),
                &Uint128::new(5000),
            )
            .unwrap();

        let stored_pool_key = POOL_KEY.load(deps.as_ref().storage).unwrap();

        // Query with the correct pool_key should succeed
        let result = query_pool(deps.as_ref(), chain_uid.clone(), stored_pool_key);
        assert!(
            result.is_ok(),
            "correct pool_key should work: {:?}",
            result.err()
        );

        // Query with a wrong pool_key should fail
        let wrong_pool_key = PoolKey {
            pair: Pair::new(
                Token::create("alpha".to_string()).unwrap(),
                Token::create("beta".to_string()).unwrap(),
            )
            .unwrap(),
            pool_type: PoolType::Concentrated {
                fee_tier_bps: 500,
                tick_spacing: 10,
            },
        };

        let err = query_pool(deps.as_ref(), chain_uid, wrong_pool_key).unwrap_err();
        assert!(
            err.to_string().contains("pool key mismatch"),
            "expected pool key mismatch error, got: {}",
            err
        );
    }
}
