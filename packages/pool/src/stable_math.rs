use cosmwasm_std::{Decimal256, StdError, StdResult, Uint128, Uint64};
use euclid::error::ContractError;
use euclid::utils::math::Decimal256Ext;

use crate::SwapResult;
/// N = 2
pub const N_COINS: Decimal256 = Decimal256::raw(2000000000000000000);
pub const AMP_PRECISION: u64 = 100;
/// The maximum number of calculation steps for Newton's method.
const ITERATIONS: u8 = 64;
/// 1e-6
pub const TOL: Decimal256 = Decimal256::raw(1000000000000);

/// Computes a stable swap result given integer token amounts.
///
/// All inputs are `Uint128` (integer amounts). The function converts to
/// `Decimal256` internally for the math, then converts the result back
/// to `Uint128`.
///
/// **Maximum supported pool value:** `Uint128::MAX / 10` (~3.4e37).
/// The `TOKEN_PRECISION = 1` scaling multiplies pool values by 10 during
/// the return amount computation, so pools at `Uint128::MAX` would overflow.
pub fn compute_stable_swap(
    offer_amount: Uint128,
    offer_pool: Uint128,
    ask_pool: Uint128,
    amp_factor: Uint64,
) -> Result<SwapResult, ContractError> {
    // Validate inputs
    if amp_factor.is_zero() {
        return Err(ContractError::new("Amp factor must be greater than zero"));
    }
    if offer_pool.is_zero() || ask_pool.is_zero() {
        return Err(ContractError::new("Pool reserves must be non-zero"));
    }
    if offer_amount.is_zero() {
        return Err(ContractError::new("Offer amount must be non-zero"));
    }

    // Convert Uint128 inputs to Decimal256 for internal math
    let offer_amount_dec = Decimal256::checked_from_integer(offer_amount)?;
    let offer_pool_dec = Decimal256::checked_from_integer(offer_pool)?;
    let ask_pool_dec = Decimal256::checked_from_integer(ask_pool)?;

    // Extra decimal digit used during subtraction to reduce rounding error.
    // Both ask_pool and new_ask_pool are scaled by 10 (via to_uint128_with_precision(1)),
    // subtracted as integers, then divided by 10. This preserves one extra digit
    // of precision compared to truncating each value independently.
    // NOTE: This limits max pool value to Uint128::MAX / 10, since the ×10 scaling
    // must fit in Uint128.
    const TOKEN_PRECISION: u8 = 1;

    // Create array of pool amounts
    let xp = [offer_pool_dec, ask_pool_dec];

    // Calculate new pool amount after swap
    let new_offer_pool = offer_pool_dec
        .checked_add(offer_amount_dec)
        .map_err(|e| ContractError::new(&e.to_string()))?;
    let new_ask_pool = calc_y(amp_factor, new_offer_pool, &xp, TOKEN_PRECISION)?;

    // Calculate return amount (what user receives)
    let ask_pool_amount = ask_pool_dec.to_uint128_with_precision(TOKEN_PRECISION)?;
    let new_ask_pool_amount = new_ask_pool;
    let return_amount = ask_pool_amount
        .checked_sub(new_ask_pool_amount)
        .map_err(|_| ContractError::new("Negative return amount"))?
        .checked_div(Uint128::new(10u128.pow(TOKEN_PRECISION as u32)))?;

    // Calculate offer amount for spread calculation
    let offer_amount = offer_amount_dec.to_uint128_with_precision(0_u32)?;

    // Calculate spread (difference between what user provides and receives)
    let spread_amount = offer_amount.abs_diff(return_amount);

    // Never allow the return amount to exceed the ask pool amount
    ask_pool.checked_sub(return_amount).map_err(|_| {
        ContractError::new("Invariant violation: return_amount exceeds pool amount")
    })?;

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

    let numerator = leverage_mul.checked_add(d_p_mul)?;
    let leverage_sub_dec = leverage.checked_sub(Decimal256::one())?;
    let leverage_sub = initial_d.checked_mul(leverage_sub_dec)?;
    let n_coins_sum = d_product.checked_mul(N_COINS.checked_add(Decimal256::one())?)?;
    // (leverage - 1) * initial_d + (n_coins + 1) * d_product
    let r_val = leverage_sub.checked_add(n_coins_sum)?;

    numerator.checked_multiply_ratio(initial_d, r_val)
}

pub fn compute_d(amp: Uint64, pools: &[Decimal256]) -> StdResult<Decimal256> {
    let leverage = Decimal256::from_ratio(amp, AMP_PRECISION).checked_mul(N_COINS)?;
    let amount_a_times_coins = pools[0].checked_mul(N_COINS)?;
    let amount_b_times_coins = pools[1].checked_mul(N_COINS)?;

    let sum_x = pools[0].checked_add(pools[1])?; // sum(x_i), a.k.a S
    if sum_x.is_zero() {
        Ok(Decimal256::zero())
    } else {
        let mut d_previous: Decimal256;
        let mut d: Decimal256 = sum_x;

        // Newton's method to approximate D
        for _ in 0..ITERATIONS {
            // d_product = D^3 / (pool_a * n * pool_b * n)
            // Computed iteratively via checked_multiply_ratio (uses Uint512 internally)
            // to avoid D^3 intermediate overflow
            let d_product = d
                .checked_multiply_ratio(d, amount_a_times_coins)?
                .checked_multiply_ratio(d, amount_b_times_coins)?;
            d_previous = d;
            d = calculate_step(d, leverage, sum_x, d_product)?;
            // Equality with the precision of 1e-6
            if d.abs_diff(d_previous) <= TOL {
                return Ok(d);
            }
        }

        Err(StdError::msg(
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
    // Use same amp scaling as compute_d: leverage = (amp / AMP_PRECISION) * N_COINS
    let leverage = Decimal256::from_ratio(amp, AMP_PRECISION).checked_mul(N_COINS)?;

    // Precompute denominators for c/denom factoring.
    // c = D^3 / (new_amount * N_COINS^2 * leverage)
    // For large pool values, c itself can exceed Decimal256 range.
    // Instead of computing c upfront, we compute c/denom directly in the loop:
    //   c/denom = D * D / (new_amount * N) * D / (N * leverage * denom)
    // Each checked_multiply_ratio uses Uint512 intermediate, and the per-step
    // results stay within Decimal256 range.
    let new_amount_times_n = new_amount.checked_mul(N_COINS)?;
    let n_times_leverage = N_COINS.checked_mul(leverage)?;

    let b = new_amount.checked_add(
        d.checked_div(leverage)
            .map_err(|e| StdError::msg(e.to_string()))?,
    )?;

    // Solve for y by approximating: y**2 + b*y = c
    let mut y_prev;
    let mut y = d;
    for _ in 0..ITERATIONS {
        y_prev = y;
        // y_new = (y^2 + c) / denom = y^2/denom + c/denom
        // where denom = 2y + b - d
        let denom = y
            .checked_mul(N_COINS)?
            .checked_add(b)?
            .checked_sub(d)
            .map_err(|e| StdError::msg(e.to_string()))?;

        // y^2 / denom (Uint512 intermediate via checked_multiply_ratio)
        let y_sq_over_denom = y.checked_multiply_ratio(y, denom)?;

        // c/denom = D^3 / (new_amount * N * N * leverage * denom)
        // Computed iteratively to keep each intermediate within Decimal256 range.
        // Step 1: D^2 / (new_amount * N) — stays ~ O(D) for balanced pools
        // Step 2: * D / (N * leverage * denom) — divides by large denom, result ~ O(y)
        let c_over_denom = d
            .checked_multiply_ratio(d, new_amount_times_n)?
            .checked_multiply_ratio(d, n_times_leverage.checked_mul(denom)?)?;

        y = y_sq_over_denom.checked_add(c_over_denom)?;

        if y.abs_diff(y_prev) <= TOL {
            return y.to_uint128_with_precision(target_precision);
        }
    }

    // Should definitely converge in 64 iterations.
    Err(StdError::msg("y is not converging"))
}
