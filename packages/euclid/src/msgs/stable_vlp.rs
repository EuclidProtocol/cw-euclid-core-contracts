use crate::{
    chain::{ChainUid, CrossChainUser},
    fee::{Fee, TotalFees},
    pool::PoolConfig,
    swap::NextSwapVlp,
    token::{Pair, PairWithAmount, Token},
};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Decimal256, Uint128, Uint64};
use cw_asset::AssetInfo;

#[cw_serde]
pub struct InstantiateMsg {
    pub router: String,
    pub virtual_balance: String,
    pub pair: Pair,
    pub fee: Fee,
    pub execute: Option<ExecuteMsg>,
    pub admin: String,
    pub amp_factor: Option<Uint64>,
}

#[cw_serde]
pub enum ExecuteMsg {
    // Registers a new pool from a new chain to an already existing VLP
    RegisterPool {
        sender: CrossChainUser,
        pair: Pair,
        tx_id: String,
    },

    UpdateFee {
        lp_fee_bps: Option<u64>,
        euclid_fee_bps: Option<u64>,
        recipient: Option<CrossChainUser>,
    },

    Swap {
        sender: CrossChainUser,
        tx_id: String,
        asset_in: Token,
        amount_in: Uint128,
        min_token_out: Uint128,
        next_swaps: Vec<NextSwapVlp>,
        test_fail: Option<bool>,
    },
    AddLiquidity {
        sender: CrossChainUser,
        tx_id: String,
        liquidity: PairWithAmount,
        slippage_tolerance_bps: u64,
    },
    RemoveLiquidity {
        sender: CrossChainUser,
        tx_id: String,
        lp_allocation: Uint128,
    },
    UpdateState {
        // Router Contract
        router: Option<String>,
        // Virtual Coin Contract
        virtual_balance: Option<String>,
        // Fee per swap for each transaction
        fee: Option<Fee>,
        // The last timestamp where the balances for each token have been updated
        last_updated: Option<u64>,
        admin: Option<String>,
        amp_factor: Option<Uint64>,
    },
}

#[cw_serde]
#[derive(QueryResponses)]

pub enum QueryMsg {
    #[returns(GetStateResponse)]
    State {},
    // Query to simulate a swap for the asset
    #[returns(GetSwapResponse)]
    SimulateSwap {
        asset: Token,
        asset_amount: Uint128,
        swaps: Vec<NextSwapVlp>,
    },
    // Queries the total reserve of the pair in the VLP
    #[returns(GetLiquidityResponse)]
    Liquidity {},

    // Queries the fee of this specific pool
    #[returns(FeeResponse)]
    Fee {},

    #[returns(TotalFeesResponse)]
    TotalFeesCollected {},

    #[returns(TotalFeesPerDenomResponse)]
    TotalFeesPerDenom { denom: String },

    // Queries the pool information for a chain id
    #[returns(StablePoolResponse)]
    Pool { chain_uid: ChainUid },
    // Query to get all pools
    #[returns(AllStablePoolsResponse)]
    GetAllPools {},
}

// We define a custom struct for each query response
#[cw_serde]
pub struct GetSwapResponse {
    pub amount_out: Uint128,
    pub asset_out: Token,
    pub spread_amount: Uint128,
}

#[cw_serde]
pub struct GetStateResponse {
    pub pair: Pair,
    pub router: String,
    pub virtual_balance: String,
    pub fee: Fee,
    pub total_fees_collected: TotalFees,
    pub last_updated: u64,
    pub total_lp_tokens: Uint128,
    pub admin: String,
    pub pool_config: PoolConfig,
}

#[cw_serde]
pub struct GetLiquidityResponse {
    pub pair: Pair,
    pub token_1_reserve: Uint128,
    pub token_2_reserve: Uint128,
    pub total_lp_tokens: Uint128,
}

#[cw_serde]
pub struct FeeResponse {
    pub fee: Fee,
}

#[cw_serde]
pub struct TotalFeesResponse {
    pub total_fees: TotalFees,
}

#[cw_serde]
pub struct TotalFeesPerDenomResponse {
    pub lp_fees: Uint128,
    pub euclid_fees: Uint128,
}

#[cw_serde]
pub struct StablePoolResponse {
    pub lp_shares: Uint128,
    pub reserve_1: Uint128,
    pub reserve_2: Uint128,
}

#[cw_serde]
pub struct StablePoolInfo {
    pub chain_uid: ChainUid,
    pub pool: StablePoolResponse,
}
#[cw_serde]
pub struct AllStablePoolsResponse {
    pub pools: Vec<StablePoolInfo>,
}

#[cw_serde]
pub struct MigrateMsg {}

/// Structure for internal use which represents swap result.
#[cw_serde]
pub struct SwapResult {
    pub return_amount: Uint128,
    pub spread_amount: Uint128,
}

/// This struct describes a Terra asset as decimal.
#[cw_serde]
pub struct DecimalAsset {
    pub info: AssetInfo,
    pub amount: Decimal256,
}

use crate::error::ContractError;
use crate::utils::math::Decimal256Ext;
use cosmwasm_std::{StdError, StdResult};
/// N = 2
pub const N_COINS: Decimal256 = Decimal256::raw(2000000000000000000);
pub const AMP_PRECISION: u64 = 100;
/// The maximum number of calculation steps for Newton's method.
const ITERATIONS: u8 = 64;
/// 1e-6
pub const TOL: Decimal256 = Decimal256::raw(1000000000000);

pub(crate) fn compute_swap(
    offer_asset: &Decimal256,
    offer_pool: &Decimal256,
    ask_pool: &Decimal256,
    amp_factor: Uint64,
) -> Result<SwapResult, ContractError> {
    // Use a constant for amplification factor instead of hardcoding
    const TOKEN_PRECISION: u8 = 1;

    // Create array of pool amounts
    let xp = [*offer_pool, *ask_pool];

    // Calculate new pool amount after swap
    let new_ask_pool = calc_y(amp_factor, offer_pool + offer_asset, &xp, TOKEN_PRECISION)?;

    // Calculate return amount (what user receives)
    let ask_pool_amount = ask_pool.to_uint128_with_precision(TOKEN_PRECISION)?;
    let new_ask_pool_amount = new_ask_pool;
    let return_amount = ask_pool_amount
        .checked_sub(new_ask_pool_amount)
        .map_err(|_| ContractError::new("Negative return amount"))?
        .checked_div(Uint128::new(10u128.pow(TOKEN_PRECISION as u32)))?;

    // Calculate offer amount (what user provides)
    let offer_amount = offer_asset.to_uint128_with_precision(0_u32)?;

    // Calculate spread (difference between what user provides and receives)
    let spread_amount = offer_amount.saturating_sub(return_amount);

    Ok(SwapResult {
        return_amount,
        spread_amount,
    })
}

/// Computes the stableswap invariant (D).
///
/// * **Equation**
///
/// A * sum(x_i) * n**n + D = A * D * n**n + D**(n+1) / (n**n * prod(x_i))
/// Helper function used to calculate the D invariant as a last step in the `compute_d` public function.
///
/// * **Equation**:
///
/// d = (leverage * sum_x + d_product * n_coins) * initial_d / ((leverage - 1) * initial_d + (n_coins + 1) * d_product)
fn calculate_step(
    initial_d: Decimal256,
    leverage: Decimal256,
    sum_x: Decimal256,
    d_product: Decimal256,
) -> StdResult<Decimal256> {
    let leverage_mul = leverage.checked_mul(sum_x)?;
    let d_p_mul = d_product.checked_mul(N_COINS)?;

    let l_val = leverage_mul.checked_add(d_p_mul)?.checked_mul(initial_d)?;
    let leverage_sub = initial_d.checked_mul(leverage - Decimal256::one())?;
    let n_coins_sum = d_product.checked_mul(N_COINS.checked_add(Decimal256::one())?)?;

    let r_val = leverage_sub.checked_add(n_coins_sum)?;

    l_val
        .checked_div(r_val)
        .map_err(|e| StdError::generic_err(e.to_string()))
}

pub fn compute_d(amp: Uint64, pools: &[Decimal256]) -> StdResult<Decimal256> {
    let leverage = Decimal256::from_ratio(amp, AMP_PRECISION) * N_COINS;
    let amount_a_times_coins = pools[0] * N_COINS;
    let amount_b_times_coins = pools[1] * N_COINS;

    let sum_x = pools[0].checked_add(pools[1])?; // sum(x_i), a.k.a S
    if sum_x.is_zero() {
        Ok(Decimal256::zero())
    } else {
        let mut d_previous: Decimal256;
        let mut d: Decimal256 = sum_x;

        // Newton's method to approximate D
        for _ in 0..ITERATIONS {
            let d_product = d.pow(3) / (amount_a_times_coins * amount_b_times_coins);
            d_previous = d;
            d = calculate_step(d, leverage, sum_x, d_product)?;
            // Equality with the precision of 1e-6
            if d.abs_diff(d_previous) <= TOL {
                return Ok(d);
            }
        }

        Err(StdError::generic_err(
            "Newton method for D failed to converge",
        ))
    }
}

/// Compute the swap amount `y` in proportion to `x`.
///
/// * **Solve for y**
///
/// y**2 + y * (sum' - (A*n**n - 1) * D / (A * n**n)) = D ** (n + 1) / (n ** (2 * n) * prod' * A)
///
/// y**2 + b*y = c
pub(crate) fn calc_y(
    amp: Uint64,
    new_amount: Decimal256,
    xp: &[Decimal256],
    target_precision: u8,
) -> StdResult<Uint128> {
    let d = compute_d(amp, xp)?;
    let leverage = Decimal256::from_ratio(amp, 1u8) * N_COINS;
    let amp_prec = Decimal256::from_ratio(AMP_PRECISION, 1u8);

    let c = d.checked_pow(3)?.checked_mul(amp_prec)?
        / new_amount
            .checked_mul(N_COINS * N_COINS)?
            .checked_mul(leverage)?;

    let b = new_amount.checked_add(d.checked_mul(amp_prec)? / leverage)?;

    // Solve for y by approximating: y**2 + b*y = c
    let mut y_prev;
    let mut y = d;
    for _ in 0..ITERATIONS {
        y_prev = y;
        y = y
            .checked_pow(2)?
            .checked_add(c)?
            .checked_div(y.checked_mul(N_COINS)?.checked_add(b)?.checked_sub(d)?)
            .map_err(|e| StdError::generic_err(e.to_string()))?;
        if y.abs_diff(y_prev) <= TOL {
            return y.to_uint128_with_precision(target_precision);
        }
    }

    // Should definitely converge in 64 iterations.
    Err(StdError::generic_err("y is not converging"))
}
