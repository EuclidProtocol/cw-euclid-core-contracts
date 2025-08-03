// use cosmwasm_std::Api;
// use cw_asset::AssetInfo;
// use euclid::error::ContractError;

// /// Helper function to check if the given asset infos are valid.
// pub(crate) fn check_asset_infos(
//     api: &dyn Api,
//     asset_infos: &[AssetInfo],
// ) -> Result<(), ContractError> {
//     if !asset_infos.iter().all_unique() {
//         return Err(ContractError::InvalidAsset {
//             asset: "".to_string(),
//         });
//     }

//     asset_infos
//         .iter()
//         .try_for_each(|asset_info| asset_info.check(api))
//         .map_err(Into::into)
// }

use cosmwasm_std::{Decimal, Decimal256, Env, SignedDecimal256, StdError, StdResult, Uint128};
use euclid::error::ContractError;
use euclid::msgs::concentrated_vlp::{AmpGamma, DecimalAsset, PoolParams};
use euclid::pool::State;
use euclid::utils::math::Decimal256Ext;
use euclid::{chain::ChainUid, token::Token};
/// 1e-5
pub const TOL: Decimal256 = Decimal256::raw(10000000000000);
/// Number of coins. (2.0)
pub const N: Decimal256 = Decimal256::raw(2000000000000000000);
/// Internal constant to increase calculation accuracy.
const PADDING: Decimal256 = Decimal256::raw(1e36 as u128);
/// N ^ 2
pub const N_POW2: Decimal256 = Decimal256::raw(4000000000000000000);
/// Iterations limit for Newton's method
pub const MAX_ITER: usize = 64;
/// Min safe trading size (0.00001) to calculate a price. This value considers
/// amount in decimal form with respective token precision.
pub const MIN_TRADE_SIZE: Decimal256 = Decimal256::raw(10000000000000);
use crate::execute::MINIMUM_LIQUIDITY_AMOUNT;
use crate::state::Config;

pub(crate) fn calculate_shares(
    env: &Env,
    config: &mut Config,
    pools: &mut [DecimalAsset],
    total_share: Decimal256,
    deposits: Vec<Decimal256>,
    slippage_tolerance: Option<Decimal>,
) -> Result<(Uint128, Decimal256), ContractError> {
    // Initial provide can not be one-sided
    if total_share.is_zero() && (deposits[0].is_zero() || deposits[1].is_zero()) {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let mut new_xp = pools
        .iter()
        .enumerate()
        .map(|(ind, pool)| pool.amount + deposits[ind])
        .collect::<Vec<_>>();
    new_xp[1] *= config.pool_state.price_state.price_scale;

    let amp_gamma = config.pool_state.get_amp_gamma(env);
    let new_d = calc_d(&new_xp, &amp_gamma)?;

    let share = if total_share.is_zero() {
        let xcp = get_xcp(new_d, config.pool_state.price_state.price_scale);
        let mint_amount = xcp
            .checked_sub(MINIMUM_LIQUIDITY_AMOUNT.to_decimal256(LP_TOKEN_PRECISION)?)
            .map_err(|_| ContractError::Generic {
                err: "Minimum liquidity amount error".to_string(),
            })?;

        // share cannot become zero after minimum liquidity subtraction
        if mint_amount.is_zero() {
            return Err(ContractError::Generic {
                err: "Minimum liquidity amount error".to_string(),
            });
        }

        config.pool_state.price_state.xcp_profit_real = Decimal256::one();
        config.pool_state.price_state.xcp_profit = Decimal256::one();

        mint_amount
    } else {
        let mut old_xp = pools.iter().map(|a| a.amount).collect::<Vec<_>>();
        old_xp[1] *= config.pool_state.price_state.price_scale;
        let old_d = calc_d(&old_xp, &amp_gamma)?;
        let share = (total_share * new_d / old_d).saturating_sub(total_share);

        let mut ideposits = deposits.clone();
        ideposits[1] *= config.pool_state.price_state.price_scale;

        share * (Decimal256::one() - calc_provide_fee(&ideposits, &new_xp, &config.pool_params))
    };

    // calculate accrued share
    let share_ratio = share / (total_share + share);
    let balanced_share = [
        new_xp[0] * share_ratio,
        new_xp[1] * share_ratio / config.pool_state.price_state.price_scale,
    ];
    let assets_diff = [
        deposits[0].diff(balanced_share[0]),
        deposits[1].diff(balanced_share[1]),
    ];

    let mut slippage = Decimal256::zero();

    // If deposit doesn't diverge too much from the balanced share, we don't update the price
    if assets_diff[0] >= MIN_TRADE_SIZE && assets_diff[1] >= MIN_TRADE_SIZE {
        slippage = assert_slippage_tolerance(
            &deposits,
            share,
            &config.pool_state.price_state,
            slippage_tolerance,
        )?;

        let last_price = assets_diff[0] / assets_diff[1];
        config.pool_state.update_price(
            &config.pool_params,
            env,
            total_share + share,
            &new_xp,
            last_price,
        )?;
    }

    Ok((share.to_uint(LP_TOKEN_PRECISION)?, slippage))
}

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

/// Calculate provide fee applied on the amount of LP tokens. Only charged for imbalanced provide.
/// * `deposits` - internal repr of deposit
/// * `xp` - internal repr of pools
pub fn calc_provide_fee(
    deposits: &[Decimal256],
    xp: &[Decimal256],
    params: &PoolParams,
) -> Decimal256 {
    let sum = deposits[0] + deposits[1];
    let avg = sum / N;

    deposits[0].abs_diff(avg) * params.fee(xp) / sum
}
