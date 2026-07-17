use cosmwasm_std::{Storage, Uint128, Uint256};
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
    let (tick_cumulative, seconds_per_liquidity_cumulative_x128) = advance_observation_cumulatives(
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

fn interpolate(
    left: &Observation,
    right: &Observation,
    target: u64,
) -> Result<Observation, ContractError> {
    if left.block_timestamp == right.block_timestamp || target <= left.block_timestamp {
        return Ok(left.clone());
    }
    if target >= right.block_timestamp {
        return Ok(right.clone());
    }
    let total = right.block_timestamp - left.block_timestamp;
    let elapsed = target - left.block_timestamp;

    // Wrapping arithmetic — cumulative values may have wrapped
    let tick_delta = right.tick_cumulative.wrapping_sub(left.tick_cumulative);
    let tick_interp = left
        .tick_cumulative
        .wrapping_add(tick_delta.wrapping_mul(elapsed as i128) / (total as i128));

    let spl_delta = right
        .seconds_per_liquidity_cumulative_x128
        .wrapping_sub(left.seconds_per_liquidity_cumulative_x128);
    let spl_product = spl_delta
        .checked_mul(Uint256::from(elapsed as u128))
        .map_err(|_| ContractError::new("oracle interpolation overflow in spl_delta * elapsed"))?;
    let spl_interp = left.seconds_per_liquidity_cumulative_x128.wrapping_add(
        spl_product
            .checked_div(Uint256::from(total as u128))
            .map_err(|_| ContractError::new("oracle interpolation division by zero"))?,
    );

    Ok(Observation {
        block_timestamp: target,
        tick_cumulative: tick_interp,
        seconds_per_liquidity_cumulative_x128: spl_interp,
        initialized: true,
    })
}

pub fn observe(
    storage: &dyn Storage,
    now: u64,
    seconds_agos: Vec<u64>,
) -> Result<(Vec<i128>, Vec<Uint256>), ContractError> {
    let slot0 = SLOT0.load(storage)?;
    let cardinality = slot0.observation_cardinality as u64;

    if cardinality == 0 {
        return Err(ContractError::new("no observations"));
    }

    // Read only valid ring buffer entries and order chronologically.
    // Oldest entry is at (observation_index + 1) % cardinality,
    // newest is at observation_index.
    let mut observations: Vec<Observation> = Vec::with_capacity(cardinality as usize);
    for i in 0..cardinality {
        let idx = (slot0.observation_index + 1 + i) % cardinality;
        if let Some(obs) = OBSERVATIONS.may_load(storage, idx)? {
            if obs.initialized {
                observations.push(obs);
            }
        }
    }

    if observations.is_empty() {
        return Err(ContractError::new("no observations"));
    }

    // Observations are now in chronological order (oldest to newest).
    // No need to sort — the ring buffer traversal order guarantees this.
    #[cfg(debug_assertions)]
    for w in observations.windows(2) {
        debug_assert!(
            w[0].block_timestamp <= w[1].block_timestamp,
            "observations not chronological: {} > {}",
            w[0].block_timestamp,
            w[1].block_timestamp
        );
    }

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
                    resolved = interpolate(left, right, target)?;
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
            let result = interpolate(&case.left, &case.right, case.target).unwrap();
            assert_eq!(
                result.tick_cumulative, case.expected_tick,
                "FAILED tick: {}",
                case.name
            );
            assert_eq!(
                result.seconds_per_liquidity_cumulative_x128, case.expected_spl,
                "FAILED spl: {}",
                case.name
            );
        }
    }

    use crate::state::Slot0;
    use cosmwasm_std::testing::MockStorage;

    /// Helper: save a Slot0 with the given observation ring buffer metadata.
    fn setup_slot0(storage: &mut dyn Storage, index: u64, cardinality: u16) {
        SLOT0
            .save(
                storage,
                &Slot0 {
                    sqrt_price_x96: Uint256::zero(),
                    tick: 0,
                    observation_index: index,
                    observation_cardinality: cardinality,
                    observation_cardinality_next: cardinality,
                },
            )
            .expect("save slot0");
    }

    /// Helper: write an observation at a specific storage index.
    fn put_obs(storage: &mut dyn Storage, idx: u64, ts: u64, tick_cum: i128) {
        OBSERVATIONS
            .save(
                storage,
                idx,
                &Observation {
                    block_timestamp: ts,
                    tick_cumulative: tick_cum,
                    seconds_per_liquidity_cumulative_x128: Uint256::zero(),
                    initialized: true,
                },
            )
            .expect("save observation");
    }

    // ------------------------------------------------------------------
    // Regression: ring buffer wraparound must not include stale data
    // ------------------------------------------------------------------

    /// Test cases for ring buffer observation reads.
    /// Each case writes observations at specific indices, sets the ring buffer
    /// pointer, then queries and checks that only valid entries are returned.
    struct ObserveCase {
        name: &'static str,
        /// (index, timestamp, tick_cumulative) tuples to write
        observations: Vec<(u64, u64, i128)>,
        /// Slot0 observation_index (points to newest)
        obs_index: u64,
        /// Slot0 observation_cardinality
        cardinality: u16,
        /// Current time for the observe call
        now: u64,
        /// seconds_agos to query
        seconds_agos: Vec<u64>,
        /// Expected tick_cumulatives
        expected_ticks: Vec<i128>,
    }

    #[test]
    fn observe_ring_buffer_cases() {
        let cases = vec![
            // Case 1: Simple linear buffer (no wraparound) — 3 observations
            ObserveCase {
                name: "linear buffer, no wraparound",
                observations: vec![(0, 100, 0), (1, 200, 1000), (2, 300, 3000)],
                obs_index: 2,
                cardinality: 3,
                now: 300,
                seconds_agos: vec![0, 100, 200],
                // now-0=300 → newest(3000), now-100=200 → obs1(1000), now-200=100 → oldest(0)
                expected_ticks: vec![3000, 1000, 0],
            },
            // Case 2: Buffer has wrapped — stale observation at index 3 must be ignored.
            // Ring: capacity=3, newest at index 1, so valid = indices 2,0,1
            // Index 3 holds a stale observation from a previous cycle.
            ObserveCase {
                name: "wrapped buffer ignores stale entries",
                observations: vec![
                    // Current cycle (valid)
                    (0, 400, 4000), // second-newest
                    (1, 500, 6000), // newest (obs_index=1)
                    (2, 300, 2000), // oldest
                    // Stale from previous cycle — should NOT be read
                    (3, 150, 999),
                ],
                obs_index: 1,
                cardinality: 3,
                now: 500,
                seconds_agos: vec![0, 200],
                // now-0=500 → newest(6000), now-200=300 → oldest(2000)
                expected_ticks: vec![6000, 2000],
            },
            // Case 3: Single observation
            ObserveCase {
                name: "single observation",
                observations: vec![(0, 100, 500)],
                obs_index: 0,
                cardinality: 1,
                now: 200,
                seconds_agos: vec![0, 100],
                // Both resolve to the single observation
                expected_ticks: vec![500, 500],
            },
            // Case 4: Wrapped buffer with interpolation between valid entries
            ObserveCase {
                name: "wrapped buffer with interpolation",
                observations: vec![(0, 400, 4000), (1, 500, 6000), (2, 300, 2000)],
                obs_index: 1,
                cardinality: 3,
                now: 500,
                // Query at now-150 = 350 → between obs at 300(2000) and 400(4000)
                // interpolated = 2000 + (4000-2000) * (350-300) / (400-300) = 2000 + 1000 = 3000
                seconds_agos: vec![150],
                expected_ticks: vec![3000],
            },
            // Case 5: Full wraparound — buffer written twice, only latest cycle valid
            ObserveCase {
                name: "full double-wrap ignores all stale",
                observations: vec![
                    (0, 700, 7000), // newest (obs_index=0)
                    (1, 600, 5000), // oldest
                    // Stale entries outside cardinality
                    (2, 200, 111),
                    (3, 100, 222),
                ],
                obs_index: 0,
                cardinality: 2,
                now: 700,
                seconds_agos: vec![0, 100],
                // now-0=700 → newest(7000), now-100=600 → oldest(5000)
                expected_ticks: vec![7000, 5000],
            },
        ];

        for case in &cases {
            let mut storage = MockStorage::new();

            for &(idx, ts, tick_cum) in &case.observations {
                put_obs(&mut storage, idx, ts, tick_cum);
            }
            setup_slot0(&mut storage, case.obs_index, case.cardinality);

            let result = observe(&storage, case.now, case.seconds_agos.clone());
            let (ticks, _) = result.unwrap_or_else(|e| {
                panic!("case '{}' failed: {}", case.name, e);
            });

            assert_eq!(
                ticks, case.expected_ticks,
                "case '{}': tick_cumulatives mismatch",
                case.name
            );
        }
    }

    /// Verify that write_observation + observe round-trip works correctly
    /// through a full ring buffer wraparound cycle.
    #[test]
    fn write_then_observe_through_wraparound() {
        let mut storage = MockStorage::new();
        let cardinality: u16 = 3;

        // Initialize slot0 and first observation
        SLOT0
            .save(
                &mut storage,
                &Slot0 {
                    sqrt_price_x96: Uint256::zero(),
                    tick: 0,
                    observation_index: 0,
                    observation_cardinality: 1,
                    observation_cardinality_next: cardinality,
                },
            )
            .expect("save slot0");
        initialize_observation(&mut storage, 100).expect("init obs");

        // Write 5 observations — forces 2 wraparound cycles through a buffer of 3
        let writes: Vec<(u64, i64)> = vec![
            (200, 10), // tick=10
            (300, 20), // tick=20
            (400, 30), // tick=30 — buffer full, next write wraps
            (500, 40), // overwrites index 0
            (600, 50), // overwrites index 1
        ];
        let liquidity = Uint128::new(1_000_000);
        for (ts, tick) in &writes {
            write_observation(&mut storage, *ts, *tick, liquidity).expect("write obs");
        }

        // Now query — only the last 3 observations should be visible
        let (ticks, _) = observe(&storage, 600, vec![0, 100, 200]).expect("observe");

        // The last 3 writes were at t=400(tick=30), t=500(tick=40), t=600(tick=50)
        // Cumulative at t=400: 0 + 10*100 + 20*100 + 30*100 = 6000
        // Cumulative at t=500: 6000 + 40*100 = 10000
        // Cumulative at t=600: 10000 + 50*100 = 15000
        //
        // now-0=600 → newest, now-100=500 → middle, now-200=400 → oldest
        assert_eq!(ticks.len(), 3);
        // Verify ordering: newest has highest cumulative
        assert!(ticks[0] >= ticks[1], "t=600 cumulative should >= t=500");
        assert!(ticks[1] >= ticks[2], "t=500 cumulative should >= t=400");
    }
}
