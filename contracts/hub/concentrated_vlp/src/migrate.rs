use cosmwasm_std::{entry_point, DepsMut, Env, Isqrt, Order, Response, Storage, Uint128, Uint256};
use cw2::{get_contract_version, set_contract_version};
use euclid::{
    error::ContractError,
    msgs::vlp::{
        base::PoolType,
        concentrated::msg::{LegacyLiquidityMode, MigrateMsg},
    },
};

use crate::{
    math::{
        liquidity_amounts::{get_amounts_for_liquidity, get_liquidity_for_amounts},
        oracle::initialize_observation,
        tick_math::{get_sqrt_ratio_at_tick, get_tick_at_sqrt_ratio},
    },
    state::{
        initialize_position_namespace_if_missing, ConcentratedPosition, MigrationMetadata, Slot0,
        TickInfo, ACTIVE_LIQUIDITY, BALANCES, CHAIN_LP_TOKENS, FEE_GROWTH_GLOBAL_0_X128,
        FEE_GROWTH_GLOBAL_1_X128, MIGRATION_METADATA, MIGRATION_REVISION, OBSERVATIONS, POOL_KEY,
        POSITIONS, PROTOCOL_FEES_0, PROTOCOL_FEES_1, SLOT0, STATE, TICKS,
    },
};

const CONTRACT_NAME: &str = "crates.io:concentrated_vlp";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
const TARGET_MIGRATION_REVISION: u16 = 2;
const SUPPORTED_SOURCE_VERSIONS: [&str; 2] = ["0.0.1", "0.1.0"];

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, env: Env, msg: MigrateMsg) -> Result<Response, ContractError> {
    let prev = get_contract_version(deps.storage)?;
    if prev.contract != CONTRACT_NAME {
        return Err(ContractError::InvalidMigration {
            prev: prev.contract,
        });
    }

    if let Some(expected_prev) = msg.expected_prev_version.as_ref() {
        if prev.version != *expected_prev {
            return Err(ContractError::new("unexpected previous contract version"));
        }
    }

    let force_rebuild = msg.force_rebuild.unwrap_or(false);
    let current_revision = MIGRATION_REVISION
        .may_load(deps.storage)?
        .unwrap_or_default();
    if current_revision >= TARGET_MIGRATION_REVISION && !force_rebuild {
        set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
        return Ok(Response::new()
            .add_attribute("action", "migrate_concentrated_vlp_noop")
            .add_attribute("reason", "already_migrated")
            .add_attribute("revision", current_revision.to_string())
            .add_attribute("from_version", prev.version)
            .add_attribute("to_version", CONTRACT_VERSION));
    }

    let supported_version = SUPPORTED_SOURCE_VERSIONS.contains(&prev.version.as_str())
        || prev.version == CONTRACT_VERSION;
    if !supported_version {
        return Err(ContractError::new("unsupported migration source version"));
    }

    let state = STATE.load(deps.storage)?;
    let pool_key = POOL_KEY.load(deps.storage)?;
    let tick_spacing = match pool_key.pool_type {
        PoolType::Concentrated { tick_spacing, .. } => {
            i64::try_from(tick_spacing).map_err(|_| ContractError::new("tick spacing overflow"))?
        }
        _ => return Err(ContractError::new("pool key must be concentrated")),
    };

    initialize_position_namespace_if_missing(deps.storage, env.contract.address.as_str())?;

    let reserve_0 = BALANCES
        .may_load(deps.storage, state.pair.token_1.clone())?
        .unwrap_or_default();
    let reserve_1 = BALANCES
        .may_load(deps.storage, state.pair.token_2.clone())?
        .unwrap_or_default();

    let mut slot0 = resolve_slot0(
        deps.storage,
        &msg.legacy_liquidity_mode,
        reserve_0,
        reserve_1,
    )?;
    slot0.tick = get_tick_at_sqrt_ratio(slot0.sqrt_price_x96).unwrap_or(0);
    slot0.observation_index = 0;
    slot0.observation_cardinality = 1;
    slot0.observation_cardinality_next = 1;

    let positions: Vec<(u128, ConcentratedPosition)> = POSITIONS
        .range(deps.storage, None, None, Order::Ascending)
        .collect::<Result<Vec<_>, _>>()?;
    let positions_migrated = positions.len() as u64;
    let total_legacy_liquidity = positions.iter().try_fold(
        Uint128::zero(),
        |acc, (_, position)| -> Result<Uint128, ContractError> {
            acc.checked_add(position.liquidity)
                .map_err(ContractError::from)
        },
    )?;

    let mut normalized: Vec<(u128, ConcentratedPosition, Uint256, Uint256)> =
        Vec::with_capacity(positions.len());
    for (position_id, mut position) in positions {
        validate_position(&position, &pool_key, tick_spacing)?;
        let sqrt_lower = get_sqrt_ratio_at_tick(position.lower_tick_index)?;
        let sqrt_upper = get_sqrt_ratio_at_tick(position.upper_tick_index)?;

        let migrated_liquidity = match msg.legacy_liquidity_mode {
            LegacyLiquidityMode::AlreadyV3Liquidity => position.liquidity,
            LegacyLiquidityMode::LegacyShareProRata => {
                if total_legacy_liquidity.is_zero() || position.liquidity.is_zero() {
                    Uint128::zero()
                } else {
                    let nominal_0 = Uint128::try_from(
                        Uint256::from(reserve_0.u128())
                            .checked_mul(Uint256::from(position.liquidity.u128()))?
                            .checked_div(Uint256::from(total_legacy_liquidity.u128()))?,
                    )
                    .map_err(|_| ContractError::new("nominal token0 overflow"))?;
                    let nominal_1 = Uint128::try_from(
                        Uint256::from(reserve_1.u128())
                            .checked_mul(Uint256::from(position.liquidity.u128()))?
                            .checked_div(Uint256::from(total_legacy_liquidity.u128()))?,
                    )
                    .map_err(|_| ContractError::new("nominal token1 overflow"))?;
                    let target = get_liquidity_for_amounts(
                        slot0.sqrt_price_x96,
                        sqrt_lower,
                        sqrt_upper,
                        nominal_0,
                        nominal_1,
                    )?;
                    let (fitted, _, _) = fit_liquidity_with_bound(
                        slot0.sqrt_price_x96,
                        sqrt_lower,
                        sqrt_upper,
                        target,
                        nominal_0,
                        nominal_1,
                    )?;
                    fitted
                }
            }
        };

        position.liquidity = migrated_liquidity;
        position.fee_growth_inside_0_last_x128 = Uint256::zero();
        position.fee_growth_inside_1_last_x128 = Uint256::zero();
        position.tokens_owed_0 = Uint128::zero();
        position.tokens_owed_1 = Uint128::zero();
        normalized.push((position_id, position, sqrt_lower, sqrt_upper));
    }

    clear_ticks(deps.storage)?;
    clear_observations(deps.storage)?;
    clear_chain_lp_tokens(deps.storage)?;

    let mut total_liquidity = Uint128::zero();
    let mut active_liquidity = Uint128::zero();
    let mut principal_0 = Uint256::zero();
    let mut principal_1 = Uint256::zero();

    let mut tick_aggregates: std::collections::BTreeMap<i64, TickInfo> =
        std::collections::BTreeMap::new();
    let current_tick = slot0.tick;

    for (position_id, position, sqrt_lower, sqrt_upper) in normalized {
        let liquidity_i128 = i128::try_from(position.liquidity.u128())
            .map_err(|_| ContractError::new("position liquidity overflow"))?;
        if !position.liquidity.is_zero() {
            apply_tick_delta(
                tick_aggregates
                    .entry(position.lower_tick_index)
                    .or_insert_with(TickInfo::default),
                liquidity_i128,
                false,
            )?;
            apply_tick_delta(
                tick_aggregates
                    .entry(position.upper_tick_index)
                    .or_insert_with(TickInfo::default),
                liquidity_i128,
                true,
            )?;

            if position.lower_tick_index <= current_tick && current_tick < position.upper_tick_index
            {
                active_liquidity = active_liquidity.checked_add(position.liquidity)?;
            }
        }

        let (amount_0, amount_1) = get_amounts_for_liquidity(
            slot0.sqrt_price_x96,
            sqrt_lower,
            sqrt_upper,
            position.liquidity,
            false,
        )?;
        principal_0 = principal_0.checked_add(amount_0)?;
        principal_1 = principal_1.checked_add(amount_1)?;

        total_liquidity = total_liquidity.checked_add(position.liquidity)?;
        let existing_chain_liquidity = CHAIN_LP_TOKENS
            .may_load(deps.storage, position.owner.chain_uid.clone())?
            .unwrap_or_default();
        CHAIN_LP_TOKENS.save(
            deps.storage,
            position.owner.chain_uid.clone(),
            &existing_chain_liquidity.checked_add(position.liquidity)?,
        )?;

        POSITIONS.save(deps.storage, position_id, &position)?;
    }

    let principal_0_u128 = Uint128::try_from(principal_0)
        .map_err(|_| ContractError::new("principal token0 overflow"))?;
    let principal_1_u128 = Uint128::try_from(principal_1)
        .map_err(|_| ContractError::new("principal token1 overflow"))?;

    if principal_0_u128 > reserve_0 || principal_1_u128 > reserve_1 {
        return Err(ContractError::new("implied principal exceeds reserves"));
    }

    let residual_0 = reserve_0.checked_sub(principal_0_u128)?;
    let residual_1 = reserve_1.checked_sub(principal_1_u128)?;

    let protocol_fees_0 = PROTOCOL_FEES_0
        .may_load(deps.storage)?
        .unwrap_or_default()
        .checked_add(residual_0)?;
    let protocol_fees_1 = PROTOCOL_FEES_1
        .may_load(deps.storage)?
        .unwrap_or_default()
        .checked_add(residual_1)?;

    FEE_GROWTH_GLOBAL_0_X128.save(deps.storage, &Uint256::zero())?;
    FEE_GROWTH_GLOBAL_1_X128.save(deps.storage, &Uint256::zero())?;
    PROTOCOL_FEES_0.save(deps.storage, &protocol_fees_0)?;
    PROTOCOL_FEES_1.save(deps.storage, &protocol_fees_1)?;
    SLOT0.save(deps.storage, &slot0)?;
    ACTIVE_LIQUIDITY.save(deps.storage, &active_liquidity)?;
    initialize_observation(deps.storage, env.block.time.seconds())?;

    let mut rebuilt_tick_count: u64 = 0;
    for (tick_index, mut tick_info) in tick_aggregates {
        if tick_info.liquidity_gross.is_zero() {
            continue;
        }
        tick_info.initialized = true;
        tick_info.fee_growth_outside_0_x128 = Uint256::zero();
        tick_info.fee_growth_outside_1_x128 = Uint256::zero();
        TICKS.save(deps.storage, tick_index, &tick_info)?;
        rebuilt_tick_count = rebuilt_tick_count.saturating_add(1);
    }

    let mut updated_state = state;
    updated_state.total_lp_tokens = total_liquidity;
    STATE.save(deps.storage, &updated_state)?;

    MIGRATION_REVISION.save(deps.storage, &TARGET_MIGRATION_REVISION)?;
    let prev_version = prev.version.clone();
    MIGRATION_METADATA.save(
        deps.storage,
        &MigrationMetadata {
            source_version: prev_version.clone(),
            mode: msg.legacy_liquidity_mode.clone(),
            migrated_at: env.block.time.seconds(),
            positions_migrated,
        },
    )?;
    initialize_position_namespace_if_missing(deps.storage, env.contract.address.as_str())?;
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new()
        .add_attribute("action", "migrate_concentrated_vlp")
        .add_attribute("pool_key", format!("{pool_key:?}"))
        .add_attribute("source_version", prev_version.clone())
        .add_attribute("mode", format!("{:?}", msg.legacy_liquidity_mode))
        .add_attribute("positions_migrated", positions_migrated.to_string())
        .add_attribute("rebuilt_tick_count", rebuilt_tick_count.to_string())
        .add_attribute("active_liquidity", active_liquidity)
        .add_attribute("total_liquidity", total_liquidity)
        .add_attribute("residual_0", residual_0)
        .add_attribute("residual_1", residual_1)
        .add_attribute("from_version", prev_version)
        .add_attribute("to_version", CONTRACT_VERSION))
}

fn resolve_slot0(
    storage: &dyn Storage,
    mode: &LegacyLiquidityMode,
    reserve_0: Uint128,
    reserve_1: Uint128,
) -> Result<Slot0, ContractError> {
    let reserve_ratio_slot0 = || -> Result<Slot0, ContractError> {
        let sqrt_price_x96 = if reserve_0.is_zero() || reserve_1.is_zero() {
            get_sqrt_ratio_at_tick(0)?
        } else {
            let ratio_x192 = Uint256::from(reserve_1.u128())
                .checked_shl(192)?
                .checked_div(Uint256::from(reserve_0.u128()))?;
            ratio_x192.isqrt()
        };
        let tick = get_tick_at_sqrt_ratio(sqrt_price_x96).unwrap_or(0);
        Ok(Slot0 {
            sqrt_price_x96,
            tick,
            observation_index: 0,
            observation_cardinality: 1,
            observation_cardinality_next: 1,
        })
    };

    match mode {
        LegacyLiquidityMode::AlreadyV3Liquidity => {
            if let Some(existing) = SLOT0.may_load(storage)? {
                Ok(existing)
            } else {
                reserve_ratio_slot0()
            }
        }
        LegacyLiquidityMode::LegacyShareProRata => reserve_ratio_slot0(),
    }
}

fn validate_position(
    position: &ConcentratedPosition,
    expected_pool_key: &euclid::msgs::vlp::base::PoolKey,
    tick_spacing: i64,
) -> Result<(), ContractError> {
    position.owner.validate()?;
    if position.pool_key != *expected_pool_key {
        return Err(ContractError::new("position pool key mismatch"));
    }
    if position.lower_tick_index >= position.upper_tick_index {
        return Err(ContractError::new("invalid tick range"));
    }
    if position.lower_tick_index < crate::state::MIN_TICK
        || position.upper_tick_index > crate::state::MAX_TICK
    {
        return Err(ContractError::new("tick out of bounds"));
    }
    if position.lower_tick_index % tick_spacing != 0
        || position.upper_tick_index % tick_spacing != 0
    {
        return Err(ContractError::new("tick not aligned with spacing"));
    }
    Ok(())
}

fn fit_liquidity_with_bound(
    sqrt_price_x96: Uint256,
    sqrt_lower_x96: Uint256,
    sqrt_upper_x96: Uint256,
    liquidity_delta: Uint128,
    max_amount_0: Uint128,
    max_amount_1: Uint128,
) -> Result<(Uint128, Uint128, Uint128), ContractError> {
    if liquidity_delta.is_zero() {
        return Ok((Uint128::zero(), Uint128::zero(), Uint128::zero()));
    }
    let fits = |liq: Uint128| -> Result<Option<(Uint128, Uint128)>, ContractError> {
        let (a0_u256, a1_u256) =
            get_amounts_for_liquidity(sqrt_price_x96, sqrt_lower_x96, sqrt_upper_x96, liq, true)?;
        let a0 =
            Uint128::try_from(a0_u256).map_err(|_| ContractError::new("amount0 overflow"))?;
        let a1 =
            Uint128::try_from(a1_u256).map_err(|_| ContractError::new("amount1 overflow"))?;
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

fn apply_tick_delta(
    tick_info: &mut TickInfo,
    liquidity_delta: i128,
    upper: bool,
) -> Result<(), ContractError> {
    tick_info.liquidity_gross = add_signed_liquidity(tick_info.liquidity_gross, liquidity_delta)?;
    if upper {
        tick_info.liquidity_net = tick_info
            .liquidity_net
            .checked_sub(liquidity_delta)
            .ok_or_else(|| ContractError::new("liquidity net overflow"))?;
    } else {
        tick_info.liquidity_net = tick_info
            .liquidity_net
            .checked_add(liquidity_delta)
            .ok_or_else(|| ContractError::new("liquidity net overflow"))?;
    }
    Ok(())
}

impl Default for TickInfo {
    fn default() -> Self {
        Self {
            initialized: false,
            liquidity_gross: Uint128::zero(),
            liquidity_net: 0,
            fee_growth_outside_0_x128: Uint256::zero(),
            fee_growth_outside_1_x128: Uint256::zero(),
        }
    }
}

fn clear_ticks(storage: &mut dyn Storage) -> Result<(), ContractError> {
    let keys: Vec<i64> = TICKS
        .keys(storage, None, None, Order::Ascending)
        .collect::<Result<Vec<_>, _>>()?;
    for key in keys {
        TICKS.remove(storage, key);
    }
    Ok(())
}

fn clear_observations(storage: &mut dyn Storage) -> Result<(), ContractError> {
    let keys: Vec<u64> = OBSERVATIONS
        .keys(storage, None, None, Order::Ascending)
        .collect::<Result<Vec<_>, _>>()?;
    for key in keys {
        OBSERVATIONS.remove(storage, key);
    }
    Ok(())
}

fn clear_chain_lp_tokens(storage: &mut dyn Storage) -> Result<(), ContractError> {
    let keys = CHAIN_LP_TOKENS
        .keys(storage, None, None, Order::Ascending)
        .collect::<Result<Vec<_>, _>>()?;
    for key in keys {
        CHAIN_LP_TOKENS.remove(storage, key);
    }
    Ok(())
}
