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
use crate::math::{calc_d, get_xcp, SignedDecimal256};
use cosmwasm_std::{Decimal, Decimal256, Deps, Env, StdError, StdResult, Uint128};
use cw_asset::Asset;
use euclid::error::ContractError;
use euclid::msgs::concentrated_vlp::{
    AmpGamma, DecimalAsset, DecimalToInteger, IntegerToDecimal, PoolParams, PriceState,
};
use euclid::pool::State;
use euclid::utils::math::Decimal256Ext;
use euclid::{chain::ChainUid, token::Token};
use itertools::Itertools;
/// 2.0
pub const TWO: Decimal256 = Decimal256::raw(2000000000000000000);
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
/// 0.05
pub const DEFAULT_SLIPPAGE: Decimal256 = Decimal256::raw(50000000000000000);
/// 0.5
pub const MAX_ALLOWED_SLIPPAGE: Decimal256 = Decimal256::raw(500000000000000000);
use crate::contract::LP_TOKEN_PRECISION;
use crate::execute::MINIMUM_LIQUIDITY_AMOUNT;
use crate::state::{Config, Precisions};

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
        deposits[0].abs_diff(balanced_share[0]),
        deposits[1].abs_diff(balanced_share[1]),
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

    Ok((
        share
            .to_uint(LP_TOKEN_PRECISION)
            .map_err(|_| ContractError::Generic {
                err: "Conversion overflow".to_string(),
            })?,
        slippage,
    ))
}

/// This is an internal function that enforces slippage tolerance for provides. Returns actual slippage.
pub fn assert_slippage_tolerance(
    deposits: &[Decimal256],
    actual_share: Decimal256,
    price_state: &PriceState,
    slippage_tolerance: Option<Decimal>,
) -> Result<Decimal256, ContractError> {
    let slippage_tolerance = slippage_tolerance
        .map(Into::into)
        .unwrap_or(DEFAULT_SLIPPAGE);
    if slippage_tolerance > MAX_ALLOWED_SLIPPAGE {
        return Err(ContractError::Generic {
            err: "Allowed spread assertion".to_string(),
        });
    }

    let deposit_value = deposits[0] + deposits[1] * price_state.price_scale;
    let lp_expected = (deposit_value / TWO * deposit_value / (TWO * price_state.price_scale))
        .sqrt()
        / price_state.xcp_profit_real;
    let slippage = lp_expected.saturating_sub(actual_share) / lp_expected;

    if slippage > slippage_tolerance {
        return Err(ContractError::Generic {
            err: "Max spread assertion".to_string(),
        });
    }

    Ok(slippage)
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

pub(crate) fn get_assets_with_precision(
    deps: Deps,
    config: &Config,
    assets: &mut Vec<Asset>,
    pools: Vec<DecimalAsset>,
    precisions: &Precisions,
) -> Result<Vec<Decimal256>, ContractError> {
    // if !check_pair_registered(
    //     deps.querier,
    //     &config.factory_addr,
    //     &config.pair_info.asset_infos,
    // )? {
    //     return Err(ContractError::Generic {
    //         err: "Pair is not registered".to_string(),
    //     });
    // }

    match assets.len() {
        0 => {
            return Err(StdError::generic_err("Nothing to provide").into());
        }
        1 => {
            // Append omitted asset with explicit zero amount
            let (given_ind, _) = config
                .pair_info
                .asset_infos
                .iter()
                .find_position(|pool| *pool == &assets[0].info)
                .ok_or_else(|| ContractError::Generic {
                    err: "Invalid asset".to_string(),
                })?;
            assets.push(Asset {
                info: config.pair_info.asset_infos[1 ^ given_ind].clone(),
                amount: Uint128::zero(),
            });
        }
        2 => {}
        _ => {
            return Err(ContractError::Generic {
                err: "Invalid number of assets".to_string(),
            });
        }
    }

    // check_assets(deps.api, assets)?;

    if pools[0].info == assets[1].info {
        assets.swap(0, 1);
    }

    // precisions.get_precision() also validates that the asset belongs to the pool
    Ok(vec![
        Decimal256::with_precision(assets[0].amount, precisions.get_precision(&assets[0].info)?)?,
        Decimal256::with_precision(assets[1].amount, precisions.get_precision(&assets[1].info)?)?,
    ])
}
