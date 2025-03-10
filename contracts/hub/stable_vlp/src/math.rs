use cosmwasm_std::{Decimal256, StdError, StdResult, Uint128, Uint64};
use euclid::error::ContractError;
use euclid::msgs::stable_vlp::SwapResult;
use euclid::utils::math::Decimal256Ext;
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
) -> Result<SwapResult, ContractError> {
    // Use a constant for amplification factor instead of hardcoding
    const AMP_FACTOR: u64 = 1000;
    const TOKEN_PRECISION: u8 = 1;

    // Create array of pool amounts
    let xp = [*offer_pool, *ask_pool];

    // Calculate new pool amount after swap
    let new_ask_pool = calc_y(
        Uint64::new(AMP_FACTOR),
        offer_pool + offer_asset,
        &xp,
        TOKEN_PRECISION,
    )?;

    // Calculate return amount (what user receives)
    let ask_pool_amount = ask_pool.to_uint128_with_precision(TOKEN_PRECISION)?;
    let new_ask_pool_amount = new_ask_pool;
    let return_amount = ask_pool_amount
        .checked_sub(new_ask_pool_amount)
        .map_err(|_| ContractError::new("Negative return amount"))?
        .checked_div(Uint128::new(10))?;

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
