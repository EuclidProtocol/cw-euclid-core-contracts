use cosmwasm_std::Uint128;

/// All tunable parameters for concentrated liquidity fuzz testing
#[derive(Debug, Clone)]
pub struct ConcentratedConfig {
    /// Pool fee tier in basis points.
    pub fee_tier_bps: u64,
    /// Minimum tick spacing between positions.
    pub tick_spacing: u64,
    /// Slippage tolerance in basis points for add-liquidity operations.
    pub slippage_tolerance_bps: u64,

    /// Initial token 0 amount deposited during pool creation.
    pub initial_amount_0: u128,
    /// Initial token 1 amount deposited during pool creation.
    pub initial_amount_1: u128,

    /// Weight for swap operations (all weights should sum to 100).
    pub weight_swap: u32,
    /// Weight for add-liquidity operations.
    pub weight_add: u32,
    /// Weight for remove-liquidity operations.
    pub weight_remove: u32,
    /// Weight for collect-fees operations.
    pub weight_collect: u32,

    /// Min/max swap amount range.
    pub swap_amount_range: (u128, u128),
    /// Min/max add-liquidity amount range.
    pub add_amount_range: (u128, u128),
    /// Min/max remove fraction range in basis points.
    pub remove_fraction_bps_range: (u64, u64),
    /// Min/max seed position amount range.
    pub seed_amount_range: (u128, u128),

    /// Tick range for random operations.
    pub tick_range: (i64, i64),

    /// Number of users (1 = single-user mode, >1 = multi-user mode).
    pub num_users: usize,

    /// Maximum tracked positions before strategy biases toward full removes.
    /// Prevents O(n) snapshot degradation in long-running tests.
    pub max_positions: usize,
}

impl Default for ConcentratedConfig {
    fn default() -> Self {
        let config = Self {
            fee_tier_bps: 500,
            tick_spacing: 10,
            slippage_tolerance_bps: 10_000,
            initial_amount_0: 50_000,
            initial_amount_1: 50_000,
            weight_swap: 40,
            weight_add: 30,
            weight_remove: 15,
            weight_collect: 15,
            swap_amount_range: (100, 50_000),
            add_amount_range: (1_000, 100_000),
            remove_fraction_bps_range: (1_000, 10_000),
            seed_amount_range: (5_000, 50_000),
            tick_range: (-5_000, 5_000),
            num_users: 1,
            max_positions: 200,
        };
        assert_eq!(
            config.weight_swap + config.weight_add + config.weight_remove + config.weight_collect,
            100,
            "operation weights must sum to 100"
        );
        config
    }
}

/// Tracked position info
#[derive(Debug, Clone)]
pub(crate) struct TrackedPosition {
    pub(crate) position_id: Uint128,
    pub(crate) owner: String,
    pub(crate) lower_tick: i64,
    pub(crate) upper_tick: i64,
}
