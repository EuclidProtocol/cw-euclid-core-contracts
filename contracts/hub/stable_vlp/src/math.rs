use cosmwasm_std::{
    ConversionOverflowError, Decimal256, Fraction, StdError, StdResult, Uint128, Uint256, Uint64,
};
use euclid::error::ContractError;
use euclid::msgs::stable_vlp::SwapResult;
/// N = 2
pub const N_COINS: Decimal256 = Decimal256::raw(2000000000000000000);
pub const AMP_PRECISION: u64 = 100;
/// The maximum number of calculation steps for Newton's method.
const ITERATIONS: u8 = 64;
/// 1e-6
pub const TOL: Decimal256 = Decimal256::raw(1000000000000);

pub(crate) fn compute_swap(
    // _storage: &dyn Storage,
    // _env: &Env,
    // config: &Config,
    offer_asset: &Decimal256,
    offer_pool: &Decimal256,
    ask_pool: &Decimal256,
) -> Result<SwapResult, ContractError> {
    let token_precision = 1;

    // let xp = pools.iter().map(|p| p.amount).collect::<Vec<_>>();
    let xp = [
        Decimal256::from_integer(Uint256::from(1000u128)),
        Decimal256::from_integer(Uint256::from(1000u128)),
    ];

    let new_ask_pool = calc_y(
        // compute_current_amp(config, env)?,
        Uint64::new(1000),
        offer_pool + offer_asset,
        &xp,
        token_precision,
    )?;

    let return_amount = ask_pool.to_uint128_with_precision(token_precision)? - new_ask_pool;
    let offer_asset_amount = offer_asset.to_uint128_with_precision(token_precision)?;

    // We consider swap rate 1:1 in stable swap thus any difference is considered as spread.
    let spread_amount = offer_asset_amount.saturating_sub(return_amount);

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

/// Trait extension for Decimal256 to work with token precisions more accurately.
pub trait Decimal256Ext {
    fn to_uint256(&self) -> Uint256;

    fn to_uint128_with_precision(&self, precision: impl Into<u32>) -> StdResult<Uint128>;

    fn to_uint256_with_precision(&self, precision: impl Into<u32>) -> StdResult<Uint256>;

    fn from_integer(i: impl Into<Uint256>) -> Self;

    fn checked_multiply_ratio(
        &self,
        numerator: Decimal256,
        denominator: Decimal256,
    ) -> StdResult<Decimal256>;

    fn with_precision(
        value: impl Into<Uint256>,
        precision: impl Into<u32>,
    ) -> StdResult<Decimal256>;
}
impl Decimal256Ext for Decimal256 {
    fn to_uint256(&self) -> Uint256 {
        self.numerator() / self.denominator()
    }

    fn to_uint128_with_precision(&self, precision: impl Into<u32>) -> StdResult<Uint128> {
        let value = self.atomics();
        let precision = precision.into();

        value
            .checked_div(10u128.pow(self.decimal_places() - precision).into())?
            .try_into()
            .map_err(|o: ConversionOverflowError| {
                StdError::generic_err(format!("Error converting {}", o.value))
            })
    }

    fn to_uint256_with_precision(&self, precision: impl Into<u32>) -> StdResult<Uint256> {
        let value = self.atomics();
        let precision = precision.into();

        value
            .checked_div(10u128.pow(self.decimal_places() - precision).into())
            .map_err(|_| StdError::generic_err("DivideByZeroError"))
    }

    fn from_integer(i: impl Into<Uint256>) -> Self {
        Decimal256::from_ratio(i.into(), 1u8)
    }

    fn checked_multiply_ratio(
        &self,
        numerator: Decimal256,
        denominator: Decimal256,
    ) -> StdResult<Decimal256> {
        Ok(Decimal256::new(
            self.atomics()
                .checked_multiply_ratio(numerator.atomics(), denominator.atomics())
                .map_err(|_| StdError::generic_err("CheckedMultiplyRatioError"))?,
        ))
    }

    fn with_precision(
        value: impl Into<Uint256>,
        precision: impl Into<u32>,
    ) -> StdResult<Decimal256> {
        Decimal256::from_atomics(value, precision.into())
            .map_err(|_| StdError::generic_err("Decimal256 range exceeded"))
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
