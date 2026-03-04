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

/// Advances the oracle cumulative values by the given time delta.
///
/// tick_cumulative += current_tick * delta (wrapping i128)
/// seconds_per_liquidity_cumulative += delta * 2^128 / liquidity (wrapping Uint256)
///
/// Both use wrapping arithmetic to match Uniswap V3's model — the absolute
/// values are meaningless, only differences between two observations matter.
pub fn advance_observation_cumulatives(
    tick_cumulative: i128,
    seconds_per_liquidity_cumulative_x128: Uint256,
    current_tick: i64,
    liquidity: Uint128,
    delta: u64,
) -> Result<(i128, Uint256), ContractError> {
    let new_tick_cumulative =
        tick_cumulative.wrapping_add((current_tick as i128).wrapping_mul(delta as i128));
    let seconds_per_liquidity_delta = if liquidity.is_zero() {
        Uint256::zero()
    } else {
        Uint256::from(delta as u128)
            .checked_mul(q128())?
            .checked_div(Uint256::from(liquidity.u128()))?
    };
    let new_spl = seconds_per_liquidity_cumulative_x128.wrapping_add(seconds_per_liquidity_delta);
    Ok((new_tick_cumulative, new_spl))
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
    let (tick_cumulative, seconds_per_liquidity_cumulative_x128) =
        advance_observation_cumulatives(
            last.tick_cumulative,
            last.seconds_per_liquidity_cumulative_x128,
            current_tick,
            liquidity,
            delta,
        )?;

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

    // Wrapping arithmetic — cumulative values may have wrapped
    let tick_delta = right.tick_cumulative.wrapping_sub(left.tick_cumulative);
    let tick_interp = left.tick_cumulative.wrapping_add(
        tick_delta.wrapping_mul(elapsed as i128) / (total as i128),
    );

    let spl_delta = right
        .seconds_per_liquidity_cumulative_x128
        .wrapping_sub(left.seconds_per_liquidity_cumulative_x128);
    let spl_interp = left
        .seconds_per_liquidity_cumulative_x128
        .wrapping_add(
            spl_delta
                .checked_mul(Uint256::from(elapsed as u128))
                .unwrap_or_default()
                .checked_div(Uint256::from(total as u128))
                .unwrap_or_default(),
        );

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advance_observation_cumulatives_table() {
        struct Case {
            name: &'static str,
            tick_cumulative: i128,
            spl_cumulative: Uint256,
            current_tick: i64,
            liquidity: Uint128,
            delta: u64,
            expected_tick: i128,
            expected_spl: Uint256,
        }

        let q128 = Uint256::one() << 128u32;

        let cases = vec![
            Case {
                name: "normal — positive tick",
                tick_cumulative: 0,
                spl_cumulative: Uint256::zero(),
                current_tick: 100,
                liquidity: Uint128::new(1000),
                delta: 10,
                // tick_cum = 0 + 100 * 10 = 1000
                // spl = 0 + 10 * 2^128 / 1000
                expected_tick: 1000,
                expected_spl: Uint256::from(10u128) * q128 / Uint256::from(1000u128),
            },
            Case {
                name: "normal — negative tick",
                tick_cumulative: 0,
                spl_cumulative: Uint256::zero(),
                current_tick: -500,
                liquidity: Uint128::new(1),
                delta: 5,
                expected_tick: -2500,
                expected_spl: Uint256::from(5u128) * q128,
            },
            Case {
                name: "extreme negative tick (V3 min tick)",
                tick_cumulative: 0,
                spl_cumulative: Uint256::zero(),
                current_tick: -887272,
                liquidity: Uint128::new(1),
                delta: 1000,
                // -887272 * 1000 = -887_272_000 — fits comfortably in i128
                expected_tick: -887_272_000,
                expected_spl: Uint256::from(1000u128) * q128,
            },
            Case {
                name: "zero delta — no change",
                tick_cumulative: 1000,
                spl_cumulative: Uint256::from(999u128),
                current_tick: 100,
                liquidity: Uint128::new(1),
                delta: 0,
                expected_tick: 1000,
                expected_spl: Uint256::from(999u128),
            },
            Case {
                name: "zero liquidity — spl unchanged",
                tick_cumulative: 0,
                spl_cumulative: Uint256::from(50u128),
                current_tick: 10,
                liquidity: Uint128::zero(),
                delta: 100,
                expected_tick: 1000,
                expected_spl: Uint256::from(50u128),
            },
            Case {
                name: "tick_cumulative wraps i128 upward",
                tick_cumulative: i128::MAX - 500,
                spl_cumulative: Uint256::zero(),
                current_tick: 1,
                liquidity: Uint128::new(1),
                delta: 1000,
                // (MAX - 500) + 1000 wraps to MIN + 499
                expected_tick: i128::MAX.wrapping_add(500),
                expected_spl: Uint256::from(1000u128) * q128,
            },
            Case {
                name: "tick_cumulative wraps i128 downward",
                // Near MIN, negative tick pushes it past MIN
                tick_cumulative: i128::MIN + 500,
                spl_cumulative: Uint256::zero(),
                current_tick: -1,
                liquidity: Uint128::new(1),
                delta: 1000,
                // (MIN + 500) + (-1 * 1000) = MIN - 500 wraps to MAX - 499
                expected_tick: (i128::MIN + 500).wrapping_add(-1000),
                expected_spl: Uint256::from(1000u128) * q128,
            },
            Case {
                name: "spl_cumulative wraps Uint256",
                tick_cumulative: 0,
                spl_cumulative: Uint256::MAX - Uint256::from(100u128),
                current_tick: 0,
                liquidity: Uint128::new(1),
                delta: 1,
                // (MAX - 100) + 2^128 wraps
                expected_tick: 0,
                expected_spl: (Uint256::MAX - Uint256::from(100u128)).wrapping_add(q128),
            },
        ];

        for case in cases {
            let (tick, spl) = advance_observation_cumulatives(
                case.tick_cumulative,
                case.spl_cumulative,
                case.current_tick,
                case.liquidity,
                case.delta,
            )
            .unwrap_or_else(|e| panic!("{}: unexpected error: {}", case.name, e));
            assert_eq!(tick, case.expected_tick, "FAILED tick: {}", case.name);
            assert_eq!(spl, case.expected_spl, "FAILED spl: {}", case.name);
        }
    }

    #[test]
    fn interpolate_table() {
        struct Case {
            name: &'static str,
            left: Observation,
            right: Observation,
            target: u64,
            expected_tick: i128,
            expected_spl: Uint256,
        }

        let cases = vec![
            Case {
                name: "normal midpoint",
                left: Observation {
                    block_timestamp: 100,
                    tick_cumulative: 1000,
                    seconds_per_liquidity_cumulative_x128: Uint256::from(500u128),
                    initialized: true,
                },
                right: Observation {
                    block_timestamp: 200,
                    tick_cumulative: 3000,
                    seconds_per_liquidity_cumulative_x128: Uint256::from(900u128),
                    initialized: true,
                },
                target: 150,
                // tick: 1000 + (3000-1000) * 50/100 = 2000
                // spl: 500 + (900-500) * 50/100 = 700
                expected_tick: 2000,
                expected_spl: Uint256::from(700u128),
            },
            Case {
                name: "target at left boundary — returns left",
                left: Observation {
                    block_timestamp: 100,
                    tick_cumulative: 1000,
                    seconds_per_liquidity_cumulative_x128: Uint256::from(500u128),
                    initialized: true,
                },
                right: Observation {
                    block_timestamp: 200,
                    tick_cumulative: 3000,
                    seconds_per_liquidity_cumulative_x128: Uint256::from(900u128),
                    initialized: true,
                },
                target: 100,
                expected_tick: 1000,
                expected_spl: Uint256::from(500u128),
            },
            Case {
                name: "target at right boundary — returns right",
                left: Observation {
                    block_timestamp: 100,
                    tick_cumulative: 1000,
                    seconds_per_liquidity_cumulative_x128: Uint256::from(500u128),
                    initialized: true,
                },
                right: Observation {
                    block_timestamp: 200,
                    tick_cumulative: 3000,
                    seconds_per_liquidity_cumulative_x128: Uint256::from(900u128),
                    initialized: true,
                },
                target: 200,
                expected_tick: 3000,
                expected_spl: Uint256::from(900u128),
            },
            Case {
                name: "negative tick delta",
                left: Observation {
                    block_timestamp: 100,
                    tick_cumulative: 5000,
                    seconds_per_liquidity_cumulative_x128: Uint256::from(500u128),
                    initialized: true,
                },
                right: Observation {
                    block_timestamp: 200,
                    tick_cumulative: 3000,
                    seconds_per_liquidity_cumulative_x128: Uint256::from(900u128),
                    initialized: true,
                },
                target: 150,
                // tick: 5000 + (3000-5000) * 50/100 = 4000
                expected_tick: 4000,
                expected_spl: Uint256::from(700u128),
            },
            Case {
                name: "wrapping tick and spl at midpoint",
                left: Observation {
                    block_timestamp: 100,
                    tick_cumulative: i128::MAX - 10,
                    seconds_per_liquidity_cumulative_x128: Uint256::MAX - Uint256::from(20u128),
                    initialized: true,
                },
                right: Observation {
                    block_timestamp: 200,
                    tick_cumulative: (i128::MAX - 10).wrapping_add(100),
                    seconds_per_liquidity_cumulative_x128: (Uint256::MAX - Uint256::from(20u128))
                        .wrapping_add(Uint256::from(200u128)),
                    initialized: true,
                },
                target: 150,
                // tick delta (wrapping) = 100, half = 50
                expected_tick: (i128::MAX - 10).wrapping_add(50),
                // spl delta (wrapping) = 200, half = 100
                expected_spl: (Uint256::MAX - Uint256::from(20u128))
                    .wrapping_add(Uint256::from(100u128)),
            },
        ];

        for case in cases {
            let result = interpolate(&case.left, &case.right, case.target);
            assert_eq!(result.tick_cumulative, case.expected_tick, "FAILED tick: {}", case.name);
            assert_eq!(
                result.seconds_per_liquidity_cumulative_x128, case.expected_spl,
                "FAILED spl: {}", case.name
            );
        }
    }
}
