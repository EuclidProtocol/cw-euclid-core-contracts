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
use crate::math::{
    calc_d, calc_y, get_xcp, safe_sma_buffer_not_full, safe_sma_calculation, SignedDecimal256,
};
use cosmwasm_std::{
    Decimal, Decimal256, Deps, Env, Fraction, StdError, StdResult, Storage, Uint128,
};
use cw_asset::Asset;
use euclid::error::ContractError;
use euclid::msgs::concentrated_vlp::{
    AmpGamma, BufferManager, DecimalAsset, DecimalToInteger, IntegerToDecimal, Observation,
    PoolParams, PrecommitObservation, PriceState,
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
/// Percentage of 1st pool volume used as offer amount to forecast last price (0.01% or 0.0001).
pub const OFFER_PERCENT: Decimal256 = Decimal256::raw(100000000000000);
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
use crate::state::{Config, Precisions, OBSERVATIONS};

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
            println!("assets: {:?}", assets);
            println!("pools: {:?}", pools);
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
/// Checks whether it possible to make a swap or not.
pub fn before_swap_check(pools: &[DecimalAsset], offer_amount: Decimal256) -> StdResult<()> {
    if offer_amount.is_zero() {
        return Err(StdError::generic_err("Swap amount must not be zero"));
    }
    if pools.iter().any(|a| a.amount.is_zero()) {
        return Err(StdError::generic_err("One of the pools is empty"));
    }

    Ok(())
}

/// Performs swap simulation to calculate a price.
pub fn calc_last_prices(xs: &[Decimal256], config: &Config, env: &Env) -> StdResult<Decimal256> {
    let mut offer_amount = Decimal256::one().min(xs[0] * OFFER_PERCENT);
    if offer_amount.is_zero() {
        offer_amount = Decimal256::raw(1u128);
    }

    let last_price = compute_swap(
        xs,
        offer_amount,
        1,
        config,
        env,
        Decimal256::zero(),
        Decimal256::zero(),
    )?
    .calc_last_price(offer_amount, 0);

    Ok(last_price)
}

/// Calculate swap result.
pub fn compute_swap(
    xs: &[Decimal256],
    offer_amount: Decimal256,
    ask_ind: usize,
    config: &Config,
    env: &Env,
    maker_fee_share: Decimal256,
    share_fee_share: Decimal256,
) -> StdResult<SwapResult> {
    let offer_ind = 1 ^ ask_ind;

    let mut ixs = xs.to_vec();
    ixs[1] *= config.pool_state.price_state.price_scale;

    let amp_gamma = config.pool_state.get_amp_gamma(env);
    let d = calc_d(&ixs, &amp_gamma)?;

    if offer_ind == 1 {
        ixs[offer_ind] += offer_amount * config.pool_state.price_state.price_scale;
    } else {
        ixs[offer_ind] += offer_amount;
    }

    let new_y = calc_y(&ixs, d, &amp_gamma, ask_ind)?;
    let mut dy = ixs[ask_ind] - new_y;
    ixs[ask_ind] = new_y;

    // Derive spread using oracle price
    let spread_fee = if ask_ind == 1 {
        dy /= config.pool_state.price_state.price_scale;
        (offer_amount / config.pool_state.price_state.oracle_price).saturating_sub(dy)
    } else {
        (offer_amount * config.pool_state.price_state.oracle_price).saturating_sub(dy)
    };

    let fee_rate = config.pool_params.fee(&ixs);
    let total_fee = fee_rate * dy;
    dy -= total_fee;

    let share_fee = total_fee * share_fee_share;

    Ok(SwapResult {
        dy,
        spread_fee,
        maker_fee: (total_fee - share_fee) * maker_fee_share,
        share_fee,
        total_fee,
    })
}

/// This structure is for internal use only. Represents swap's result.
#[derive(Debug)]
pub struct SwapResult {
    pub dy: Decimal256,
    pub spread_fee: Decimal256,
    pub maker_fee: Decimal256,
    pub share_fee: Decimal256,
    pub total_fee: Decimal256,
}

impl SwapResult {
    /// Calculates **last price** for PCL repeg algo
    pub fn calc_last_price(&self, offer_amount: Decimal256, offer_ind: usize) -> Decimal256 {
        if offer_ind == 0 {
            offer_amount / (self.dy + self.maker_fee + self.share_fee)
        } else {
            (self.dy + self.maker_fee + self.share_fee) / offer_amount
        }
    }
}

use crate::math::AbsDiff;

/// If `belief_price` and `max_spread` are both specified, we compute a new spread,
/// otherwise we just use the swap spread to check `max_spread`.
///
/// * **belief_price** belief price used in the swap.
///
/// * **max_spread** max spread allowed so that the swap can be executed successfuly.
///
/// * **offer_amount** amount of assets to swap.
///
/// * **return_amount** amount of assets  a user wants to receive from the swap.
///
/// * **spread_amount** spread used in the swap.
pub fn assert_max_spread(
    belief_price: Option<Decimal>,
    max_spread: Option<Decimal>,
    offer_amount: Uint128,
    return_amount: Uint128,
    spread_amount: Uint128,
) -> Result<(), ContractError> {
    let max_spread = max_spread.map(Decimal256::from).unwrap_or(DEFAULT_SLIPPAGE);
    if max_spread > MAX_ALLOWED_SLIPPAGE {
        return Err(ContractError::Generic {
            err: "Allowed spread assertion".to_string(),
        });
    }

    if let Some(belief_price) = belief_price {
        let expected_return = offer_amount
            * belief_price
                .inv()
                .ok_or_else(|| {
                    StdError::generic_err("Invalid belief_price. Check the input values.")
                })?
                .to_uint_floor(); // not sure if this should be floor or ceiling

        let spread_amount = expected_return.saturating_sub(return_amount);

        if return_amount < expected_return
            && Decimal256::from_ratio(spread_amount, expected_return) > max_spread
        {
            return Err(ContractError::Generic {
                err: "Max spread assertion".to_string(),
            });
        }
    } else if Decimal256::from_ratio(spread_amount, return_amount + spread_amount) > max_spread {
        return Err(ContractError::Generic {
            err: "Max spread assertion".to_string(),
        });
    }

    Ok(())
}

/// Calculate and save price moving average
pub fn accumulate_swap_sizes(storage: &mut dyn Storage, env: &Env) -> Result<(), ContractError> {
    if let Some(PrecommitObservation {
        base_amount,
        quote_amount,
        precommit_ts,
    }) = PrecommitObservation::may_load(storage)?
    {
        let mut buffer = BufferManager::new(storage, OBSERVATIONS)?;
        let observed_price = Decimal::from_ratio(base_amount, quote_amount);

        let new_observation;
        if let Some(last_obs) = buffer.read_last(storage)? {
            // Skip saving observation if it has been already saved
            if last_obs.ts < precommit_ts {
                // Since this is circular buffer the next index contains the oldest value
                let count = buffer.capacity();
                if let Some(oldest_obs) = buffer.read_single(storage, buffer.head() + 1)? {
                    let price_sma = safe_sma_calculation(
                        last_obs.price_sma,
                        oldest_obs.price,
                        count,
                        observed_price,
                    )?;
                    new_observation = Observation {
                        ts: precommit_ts,
                        price: observed_price,
                        price_sma,
                    };
                } else {
                    // Buffer is not full yet
                    let count = buffer.head();
                    let price_sma =
                        safe_sma_buffer_not_full(last_obs.price_sma, count, observed_price)?;
                    new_observation = Observation {
                        ts: precommit_ts,
                        price: observed_price,
                        price_sma,
                    };
                }

                buffer.instant_push(storage, &new_observation)?
            }
        } else {
            // Buffer is empty
            if env.block.time.seconds() > precommit_ts {
                new_observation = Observation {
                    ts: precommit_ts,
                    price: observed_price,
                    price_sma: observed_price,
                };

                buffer.instant_push(storage, &new_observation)?
            }
        }
    }

    Ok(())
}

/// Return the amount of tokens that a specific amount of LP tokens would withdraw.
///
/// * **pools** assets available in the pool.
///
/// * **amount** amount of LP tokens to calculate underlying amounts for.
///
/// * **total_share** total amount of LP tokens currently issued by the pool.
pub fn get_share_in_assets(
    pools: &[DecimalAsset],
    amount: Uint128,
    total_share: Uint128,
) -> Vec<DecimalAsset> {
    let share_ratio = if !total_share.is_zero() {
        Decimal256::from_ratio(amount, total_share)
    } else {
        Decimal256::zero()
    };

    pools
        .iter()
        .map(|pool| DecimalAsset {
            info: pool.info.clone(),
            amount: pool.amount * share_ratio,
        })
        .collect()
}
