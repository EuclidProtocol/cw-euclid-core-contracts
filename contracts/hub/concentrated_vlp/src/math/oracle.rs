use cosmwasm_std::{Order, Storage, Uint128, Uint256};
use euclid::error::ContractError;

use crate::state::{Observation, OBSERVATIONS, SLOT0};

fn q128() -> Uint256 {
    Uint256::one() << 128u32
}

pub fn initialize_observation(
    storage: &mut dyn Storage,
    timestamp: u64,
) -> Result<(), ContractError> {
    OBSERVATIONS.save(
        storage,
        0,
        &Observation {
            block_timestamp: timestamp,
            tick_cumulative: 0,
            seconds_per_liquidity_cumulative_x128: Uint256::zero(),
            initialized: true,
        },
    )?;
    Ok(())
}

pub fn write_observation(
    storage: &mut dyn Storage,
    block_timestamp: u64,
    current_tick: i64,
    liquidity: Uint128,
) -> Result<(), ContractError> {
    let mut slot0 = SLOT0.load(storage)?;
    let last = OBSERVATIONS
        .may_load(storage, slot0.observation_index)?
        .unwrap_or(Observation {
            block_timestamp,
            tick_cumulative: 0,
            seconds_per_liquidity_cumulative_x128: Uint256::zero(),
            initialized: true,
        });

    if block_timestamp == last.block_timestamp {
        return Ok(());
    }

    let delta = block_timestamp.saturating_sub(last.block_timestamp);
    let tick_cumulative = last
        .tick_cumulative
        .saturating_add((current_tick as i128).saturating_mul(delta as i128));
    let seconds_per_liquidity_delta = if liquidity.is_zero() {
        Uint256::zero()
    } else {
        Uint256::from(delta as u128)
            .checked_mul(q128())?
            .checked_div(Uint256::from(liquidity.u128()))?
    };
    let seconds_per_liquidity_cumulative_x128 = last
        .seconds_per_liquidity_cumulative_x128
        .checked_add(seconds_per_liquidity_delta)?;

    let mut cardinality = slot0.observation_cardinality;
    if cardinality < slot0.observation_cardinality_next {
        cardinality = cardinality.saturating_add(1);
    }
    let next_index = if cardinality == 0 {
        0
    } else {
        (slot0.observation_index + 1) % (cardinality as u64)
    };

    OBSERVATIONS.save(
        storage,
        next_index,
        &Observation {
            block_timestamp,
            tick_cumulative,
            seconds_per_liquidity_cumulative_x128,
            initialized: true,
        },
    )?;

    slot0.observation_index = next_index;
    slot0.observation_cardinality = cardinality.max(1);
    SLOT0.save(storage, &slot0)?;

    Ok(())
}

fn interpolate(left: &Observation, right: &Observation, target: u64) -> Observation {
    if left.block_timestamp == right.block_timestamp || target <= left.block_timestamp {
        return left.clone();
    }
    if target >= right.block_timestamp {
        return right.clone();
    }
    let total = right.block_timestamp - left.block_timestamp;
    let elapsed = target - left.block_timestamp;

    let tick_delta = right.tick_cumulative.saturating_sub(left.tick_cumulative);
    let tick_interp = left.tick_cumulative.saturating_add(
        tick_delta.saturating_mul(elapsed as i128) / (total as i128),
    );

    let spl_delta = right
        .seconds_per_liquidity_cumulative_x128
        .checked_sub(left.seconds_per_liquidity_cumulative_x128)
        .unwrap_or_default();
    let spl_interp = left
        .seconds_per_liquidity_cumulative_x128
        .checked_add(
            spl_delta
                .checked_mul(Uint256::from(elapsed as u128))
                .unwrap_or_default()
                .checked_div(Uint256::from(total as u128))
                .unwrap_or_default(),
        )
        .unwrap_or(left.seconds_per_liquidity_cumulative_x128);

    Observation {
        block_timestamp: target,
        tick_cumulative: tick_interp,
        seconds_per_liquidity_cumulative_x128: spl_interp,
        initialized: true,
    }
}

pub fn observe(
    storage: &dyn Storage,
    now: u64,
    seconds_agos: Vec<u64>,
) -> Result<(Vec<i128>, Vec<Uint256>), ContractError> {
    let mut observations: Vec<Observation> = OBSERVATIONS
        .range(storage, None, None, Order::Ascending)
        .filter_map(|item| item.ok().map(|(_, obs)| obs))
        .filter(|obs| obs.initialized)
        .collect();
    if observations.is_empty() {
        return Err(ContractError::new("no observations"));
    }
    observations.sort_by_key(|obs| obs.block_timestamp);

    let oldest = observations
        .first()
        .cloned()
        .ok_or_else(|| ContractError::new("no observations"))?;
    let newest = observations
        .last()
        .cloned()
        .ok_or_else(|| ContractError::new("no observations"))?;

    let mut tick_cumulatives = Vec::with_capacity(seconds_agos.len());
    let mut seconds_per_liquidity = Vec::with_capacity(seconds_agos.len());

    for seconds_ago in seconds_agos {
        let target = now.saturating_sub(seconds_ago);
        let resolved = if target <= oldest.block_timestamp {
            oldest.clone()
        } else if target >= newest.block_timestamp {
            newest.clone()
        } else {
            let mut resolved = newest.clone();
            for window in observations.windows(2) {
                let left = &window[0];
                let right = &window[1];
                if left.block_timestamp <= target && target <= right.block_timestamp {
                    resolved = interpolate(left, right, target);
                    break;
                }
            }
            resolved
        };
        tick_cumulatives.push(resolved.tick_cumulative);
        seconds_per_liquidity.push(resolved.seconds_per_liquidity_cumulative_x128);
    }

    Ok((tick_cumulatives, seconds_per_liquidity))
}
