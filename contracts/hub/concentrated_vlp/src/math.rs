use cosmwasm_std::{Decimal, Decimal256, Fraction, StdError, StdResult, Uint128, Uint256, Uint64};
use euclid::msgs::concentrated_vlp::{AmpGamma, Decimal256Ext};
use itertools::Itertools;
use std::fmt::{Display, Formatter};
use std::ops;

/// 2.0
pub const TWO: Decimal256 = Decimal256::raw(2000000000000000000);
/// 1e-5
pub const TOL: Decimal256 = Decimal256::raw(10000000000000);
/// Number of coins. (2.0)
pub const N: Decimal256 = Decimal256::raw(2000000000000000000);
/// Internal constant to increase calculation accuracy.
const PADDING: Decimal256 = Decimal256::raw(1e36 as u128);
/// halfpow tolerance (1e-10)
pub const HALFPOW_TOL: Decimal256 = Decimal256::raw(100000000);
/// N ^ 2
pub const N_POW2: Decimal256 = Decimal256::raw(4000000000000000000);
/// Iterations limit for Newton's method
pub const MAX_ITER: usize = 64;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignedDecimal256 {
    val: Decimal256,
    /// false - positive, true - negative
    neg: bool,
}

impl Display for SignedDecimal256 {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let sign = if self.neg { "-" } else { "" };
        f.write_str(&format!("{sign}{}", self.val))
    }
}

impl SignedDecimal256 {
    pub fn new(val: Decimal256, neg: bool) -> Self {
        Self { val, neg }
    }
    pub fn pow(&self, exp: u32) -> Self {
        if self.val.is_zero() {
            Self::from(Decimal256::zero())
        } else {
            let neg = if exp % 2 == 0 { false } else { self.neg };
            Self {
                val: self.val.pow(exp),
                neg,
            }
        }
    }
    pub fn diff(self, other: SignedDecimal256) -> Decimal256 {
        if self.neg == other.neg {
            self.val.abs_diff(other.val)
        } else {
            self.val + other.val
        }
    }
}

impl From<Decimal256> for SignedDecimal256 {
    fn from(val: Decimal256) -> Self {
        Self { val, neg: false }
    }
}

impl From<&Decimal256> for SignedDecimal256 {
    fn from(val: &Decimal256) -> Self {
        Self::from(*val)
    }
}

impl TryInto<Decimal256> for SignedDecimal256 {
    type Error = StdError;

    fn try_into(self) -> Result<Decimal256, Self::Error> {
        if !self.neg || self.val.is_zero() {
            Ok(self.val)
        } else {
            Err(StdError::generic_err(format!(
                "Unable to convert negative value, {}",
                self
            )))
        }
    }
}

impl ops::Add for SignedDecimal256 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        if self.neg == rhs.neg {
            Self {
                val: self.val + rhs.val,
                ..self
            }
        } else if self.val > rhs.val {
            Self {
                val: self.val - rhs.val,
                ..self
            }
        } else {
            Self {
                val: rhs.val - self.val,
                ..rhs
            }
        }
    }
}

impl ops::Add<Decimal256> for SignedDecimal256 {
    type Output = SignedDecimal256;

    fn add(self, rhs: Decimal256) -> Self::Output {
        self + SignedDecimal256::from(rhs)
    }
}

impl ops::Add<SignedDecimal256> for Decimal256 {
    type Output = SignedDecimal256;

    fn add(self, rhs: SignedDecimal256) -> Self::Output {
        rhs + self
    }
}

impl ops::Sub for SignedDecimal256 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        self + Self {
            neg: !rhs.neg,
            ..rhs
        }
    }
}

impl ops::Sub<Decimal256> for SignedDecimal256 {
    type Output = SignedDecimal256;

    #[allow(clippy::suspicious_arithmetic_impl)]
    fn sub(self, rhs: Decimal256) -> Self::Output {
        self + Self {
            val: rhs,
            neg: true,
        }
    }
}

impl ops::Sub<SignedDecimal256> for Decimal256 {
    type Output = SignedDecimal256;

    fn sub(self, rhs: SignedDecimal256) -> Self::Output {
        SignedDecimal256::from(self) - rhs
    }
}

impl ops::Mul<Decimal256> for SignedDecimal256 {
    type Output = SignedDecimal256;

    fn mul(self, rhs: Decimal256) -> Self::Output {
        Self {
            val: self.val * rhs,
            ..self
        }
    }
}

impl ops::Mul for SignedDecimal256 {
    type Output = SignedDecimal256;

    fn mul(self, rhs: Self) -> Self::Output {
        Self {
            val: self.val * rhs.val,
            neg: self.neg ^ rhs.neg,
        }
    }
}

impl ops::Mul<SignedDecimal256> for Decimal256 {
    type Output = SignedDecimal256;

    fn mul(self, rhs: SignedDecimal256) -> Self::Output {
        rhs * self
    }
}

impl ops::Div for SignedDecimal256 {
    type Output = SignedDecimal256;

    fn div(self, rhs: Self) -> Self::Output {
        Self {
            val: self.val / rhs.val,
            neg: self.neg ^ rhs.neg,
        }
    }
}

impl ops::Div<Decimal256> for SignedDecimal256 {
    type Output = SignedDecimal256;

    fn div(self, rhs: Decimal256) -> Self::Output {
        self / SignedDecimal256::from(rhs)
    }
}

impl ops::Div<SignedDecimal256> for Decimal256 {
    type Output = SignedDecimal256;

    fn div(self, rhs: SignedDecimal256) -> Self::Output {
        Self::Output {
            val: self / rhs.val,
            neg: rhs.neg,
        }
    }
}

impl ops::Neg for SignedDecimal256 {
    type Output = SignedDecimal256;

    fn neg(self) -> Self::Output {
        Self {
            neg: !self.neg,
            ..self
        }
    }
}

pub trait AbsDiff
where
    Self: Copy + PartialOrd + ops::Sub<Output = Self>,
{
    fn diff(self, rhs: Self) -> Self {
        if self > rhs {
            self - rhs
        } else {
            rhs - self
        }
    }
}

impl AbsDiff for Uint256 {}
impl AbsDiff for Uint128 {}
impl AbsDiff for Uint64 {}
impl AbsDiff for Decimal {}
impl AbsDiff for Decimal256 {}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::consts::TWO;
//     use cosmwasm_std::StdResult;
//     use std::str::FromStr;

//     #[test]
//     fn test_signed_arithmetics() {
//         let val = Decimal256::from_str("0.1").unwrap();
//         let pos = SignedDecimal256::from(val);
//         let neg = SignedDecimal256::new(val, true);

//         let res: Decimal256 = (pos + neg).try_into().unwrap();
//         assert_eq!(res, Decimal256::zero());

//         let res: Decimal256 = (pos - neg).try_into().unwrap();
//         assert_eq!(res, Decimal256::from_str("0.2").unwrap());

//         assert_eq!(
//             neg + neg,
//             SignedDecimal256::new(Decimal256::from_str("0.2").unwrap(), true)
//         );

//         let res: Decimal256 = (neg - neg).try_into().unwrap();
//         assert_eq!(res, Decimal256::zero());

//         let res = neg + neg;
//         assert_eq!(res.to_string(), "-0.2");
//     }

//     #[test]
//     fn test_signed_division() {
//         let pos = SignedDecimal256::from(Decimal256::from_str("1").unwrap());
//         let neg = SignedDecimal256::new(Decimal256::from_str("2").unwrap(), true);

//         assert_eq!(
//             pos / neg,
//             SignedDecimal256::new(Decimal256::from_str("0.5").unwrap(), true)
//         );

//         assert_eq!(
//             neg / pos,
//             SignedDecimal256::new(Decimal256::from_str("2").unwrap(), true)
//         );

//         assert_eq!(neg / neg, SignedDecimal256::new(Decimal256::one(), false));
//         assert_eq!(pos / pos, SignedDecimal256::new(Decimal256::one(), false));
//     }

//     #[test]
//     fn test_mixed_decimals() {
//         let a = Decimal256::one();
//         let b = SignedDecimal256::new(a, true);

//         let res: Decimal256 = (b + a).try_into().unwrap();
//         assert_eq!(res, Decimal256::zero());

//         let minus_two = SignedDecimal256::new(TWO, true);
//         let res: StdResult<Decimal256> = minus_two.try_into();
//         assert_eq!(
//             res.unwrap_err().to_string(),
//             "Generic error: Unable to convert negative value, -2"
//         );

//         assert_eq!(b / a, SignedDecimal256::new(Decimal256::one(), true));
//         assert_eq!(a - b, SignedDecimal256::from(TWO));
//         assert_eq!(b - a, minus_two);
//         assert_eq!(SignedDecimal256::from(a).diff(b), TWO)
//     }

//     #[test]
//     fn test_pow() {
//         let a = SignedDecimal256::from(Decimal256::zero());
//         let two = SignedDecimal256::from(TWO);
//         let minus_two = -two;

//         assert_eq!(a.pow(10), SignedDecimal256::from(Decimal256::zero()));
//         assert_eq!(
//             two.pow(3),
//             SignedDecimal256::from(Decimal256::from_str("8").unwrap())
//         );
//         assert_eq!(
//             minus_two.pow(2),
//             SignedDecimal256::from(Decimal256::from_str("4").unwrap())
//         );
//         assert_eq!(
//             minus_two.pow(3),
//             SignedDecimal256::new(Decimal256::from_str("8").unwrap(), true)
//         );
//     }
// }

/// Calculate D invariant based on known pool volumes.
///
/// * **xs** - internal representation of pool volumes.
/// * **amp_gamma** - an object which represents current Amp and Gamma parameters.
pub fn calc_d(xs: &[Decimal256], amp_gamma: &AmpGamma) -> StdResult<Decimal256> {
    newton_d(xs, amp_gamma.amp.into(), amp_gamma.gamma.into())
}
pub(crate) fn newton_d(
    x: &[Decimal256],
    a: Decimal256,
    gamma: Decimal256,
) -> StdResult<Decimal256> {
    let mut d_prev: SignedDecimal256 = (N * geometric_mean(x)).into();
    let x = x.iter().map(SignedDecimal256::from).collect::<Vec<_>>();

    for _ in 0..MAX_ITER {
        let d = d_prev - f(d_prev, &x, a, gamma) / df_dd(d_prev, &x, a, gamma);
        if d.diff(d_prev) <= TOL {
            return d.try_into();
        }
        d_prev = d;
    }

    Err(StdError::generic_err("newton_d is not converging"))
}
pub fn geometric_mean(x: &[Decimal256]) -> Decimal256 {
    (x[0] * x[1]).sqrt()
}

/// df/dD
pub(crate) fn df_dd(
    d: SignedDecimal256,
    x: &[SignedDecimal256],
    a: Decimal256,
    gamma: Decimal256,
) -> SignedDecimal256 {
    let a_gamma_pow_2 = a * gamma.pow(2); // A * gamma^2
    let gamma_plus_1 = gamma + Decimal256::one();
    let d_pow_n = d.pow(2);
    let prod_n_n = x[0] * x[1] * N_POW2;
    let sum = x[0] + x[1];

    let k0 = prod_n_n / d_pow_n;
    let k0_prime = -SignedDecimal256::from(N) * prod_n_n;

    let gamma_one_k0 = gamma_plus_1 - k0; // gamma + 1 - K0

    let k = a_gamma_pow_2 * k0 / (gamma_plus_1 - k0).pow(2);
    let k_prime_numerator = PADDING * a_gamma_pow_2 * k0_prime * (gamma_plus_1 + k0);
    let k_prime_denominator = PADDING * d.pow(3) * gamma_one_k0 * gamma_one_k0 * gamma_one_k0;

    k_prime_numerator * d * sum / k_prime_denominator + k * sum
        - k_prime_numerator * d_pow_n / k_prime_denominator
        - N * k * d
        - d / N
}

pub(crate) fn f(
    d: SignedDecimal256,
    x: &[SignedDecimal256],
    a: Decimal256,
    gamma: Decimal256,
) -> SignedDecimal256 {
    let mul = x[0] * x[1];
    let d_pow2 = d.pow(2);

    let prod_n_n = mul * N_POW2;
    let k = a * gamma.pow(2) * prod_n_n
        / ((gamma + Decimal256::one() - prod_n_n / d_pow2).pow(2) * d_pow2);

    d * (x[0] + x[1]) * k + mul - k * d_pow2 - d_pow2 / N_POW2
}
/// Get current XCP.
/// * **d** - internal D invariant.
/// * **price_scale** - x_0/x_1 exchange rate.
pub fn get_xcp(d: Decimal256, price_scale: Decimal256) -> Decimal256 {
    let xs = [d / N, d / (N * price_scale)];
    geometric_mean(&xs)
}

/// Calculates 0.5^power.
pub fn half_float_pow(power: Decimal256) -> StdResult<Decimal256> {
    let intpow = power.floor();
    let intpow_u128: Uint128 = (intpow.numerator() / intpow.denominator()).try_into()?;

    let half = Decimal256::from_ratio(1u8, 2u8);
    let frac_pow = power - intpow;

    // 0.5 ^ int_power
    let result = half.pow(intpow_u128.u128() as u32);

    let mut term = Decimal256::one();
    let mut sum = Decimal256::one();

    for i in 1..(MAX_ITER as u128) {
        let k = Decimal256::from_atomics(i, 0).unwrap();
        let mut c = k - Decimal256::one();

        c = frac_pow.abs_diff(c);
        term = term * c * half / k;
        sum -= term;

        if term < HALFPOW_TOL {
            return Ok(result * sum);
        }
    }

    Err(StdError::generic_err("halfpow is not converging"))
}

/// The maximum number of calculation steps for Newton's method.
const ITERATIONS: u8 = 64;

pub const MAX_AMP: u64 = 1_000_000;
pub const MAX_AMP_CHANGE: u64 = 10;
pub const MIN_AMP_CHANGING_TIME: u64 = 86400;
pub const AMP_PRECISION: u64 = 100;
/// N = 2
pub const N_COINS: Decimal256 = Decimal256::raw(2000000000000000000);
/// Calculate unknown pool's volume based on the other side of pools which is known and D.
///
/// * **xs** - internal representation of pool volumes.
/// * **d** - current D invariant.
/// * **amp_gamma** - an object which represents current Amp and Gamma parameters.
/// * **ask_ind** - the index of pool which is unknown.
pub fn calc_y(
    xs: &[Decimal256],
    d: Decimal256,
    amp_gamma: &AmpGamma,
    ask_ind: usize,
) -> StdResult<Decimal256> {
    newton_y(xs, amp_gamma.amp.into(), amp_gamma.gamma.into(), d, ask_ind)
}

pub(crate) fn newton_y(
    xs: &[Decimal256],
    a: Decimal256,
    gamma: Decimal256,
    d: Decimal256,
    j: usize,
) -> StdResult<Decimal256> {
    let mut x = xs.iter().map(SignedDecimal256::from).collect_vec();
    let x0 = d.pow(2) / (N_POW2 * x[1 - j]);
    let mut xi_1 = x0;
    x[j] = x0;

    for _ in 0..MAX_ITER {
        let xi = xi_1 - f(d.into(), &x, a, gamma) / df_dx(d, &x, a, gamma, j);
        if xi.diff(xi_1) <= TOL {
            return xi.try_into();
        }
        x[j] = xi;
        xi_1 = xi;
    }

    Err(StdError::generic_err("newton_y is not converging"))
}

/// df/dx
pub(crate) fn df_dx(
    d: Decimal256,
    x: &[SignedDecimal256],
    a: Decimal256,
    gamma: Decimal256,
    i: usize,
) -> SignedDecimal256 {
    let x_r = x[1 - i];
    let d_pow2 = d.pow(2);

    let k0 = x[0] * x[1] * N_POW2 / d_pow2;
    let gamma_one_k0 = gamma + Decimal256::one() - k0;
    let gamma_one_k0_pow2 = gamma_one_k0.pow(2);
    let a_gamma_pow2 = a * gamma.pow(2);

    let k = a_gamma_pow2 * k0 / gamma_one_k0_pow2;
    let k0_x = x_r * N_POW2;
    let k_x = k0_x * a_gamma_pow2 * (gamma + Decimal256::one() + k0) * PADDING
        / (PADDING * d_pow2 * gamma_one_k0 * gamma_one_k0_pow2);

    (k_x * (x[0] + x[1]) + k) * d + x_r - k_x * d_pow2
}

/// Computes the stableswap invariant (D).
///
/// * **Equation**
///
/// A * sum(x_i) * n**n + D = A * D * n**n + D**(n+1) / (n**n * prod(x_i))
///
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

/// Internal function to calculate new moving average using Uint256.
/// Overflow is possible only if new average price is greater than 2^128 - 1 which is unlikely.
/// Formula: (sma * count + new_price - oldest_price) / count
pub fn safe_sma_calculation(
    price_sma: Decimal,
    oldest_price: Decimal,
    count: u32,
    new_price: Decimal,
) -> StdResult<Decimal> {
    let sma_times_count = price_sma.numerator().full_mul(count);
    let res = Decimal256::from_ratio(
        sma_times_count + Uint256::from(new_price.numerator())
            - Uint256::from(oldest_price.numerator()),
        price_sma.denominator().full_mul(count),
    );

    try_dec256_into_dec(res)
}
pub fn try_dec256_into_dec(val: Decimal256) -> StdResult<Decimal> {
    let numerator: Uint128 = val.numerator().try_into()?;

    Ok(Decimal::from_ratio(numerator, Decimal::one().denominator()))
}

/// Same as [`safe_sma_calculation`] but is being used when buffer is not full yet.
/// Formula: (sma * count + new_price) / (count + 1)
pub fn safe_sma_buffer_not_full(
    price_sma: Decimal,
    count: u32,
    new_price: Decimal,
) -> StdResult<Decimal> {
    let sma_times_count = price_sma.numerator().full_mul(count);
    let res = Decimal256::from_ratio(
        sma_times_count + Uint256::from(new_price.numerator()),
        price_sma.denominator().full_mul(count + 1),
    );

    try_dec256_into_dec(res)
}
