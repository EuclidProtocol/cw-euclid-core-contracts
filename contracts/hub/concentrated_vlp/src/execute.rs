// Concentrated VLP

use cosmwasm_std::{
    attr, coin, ensure, ensure_eq, to_json_binary, wasm_execute, Addr, Coin, CosmosMsg, Decimal,
    Decimal256, DepsMut, Env, MessageInfo, Response, StdResult, Uint128, Uint256,
};
use cw20::Cw20ExecuteMsg;
use cw_asset::{Asset, AssetInfo};
use cw_utils::one_coin;
use euclid::{
    chain::CrossChainUser,
    error::ContractError,
    liquidity::AddLiquidityResponse,
    msgs::concentrated_vlp::{
        query_fee_info, query_native_supply, tf_burn_msg, DecimalToInteger, IntegerToDecimal,
        PrecommitObservation,
    },
    pool::PoolType,
    token::PairWithAmount,
};
use itertools::Itertools;
/// Minimum initial LP share
pub const MINIMUM_LIQUIDITY_AMOUNT: Uint128 = Uint128::new(1_000);
use crate::{
    math::{calc_d, get_xcp},
    state::{
        accumulate_prices, mint_liquidity_token_message, query_pools, AssetExt, AssetInfoExt,
        Precisions, CONCENTRATED_BALANCES, CONFIG,
    },
    utils::{
        accumulate_swap_sizes, assert_max_spread, before_swap_check, calc_last_prices,
        calculate_shares, compute_swap, get_assets_with_precision, get_share_in_assets,
    },
};
/// An LP token's precision.
pub(crate) const LP_TOKEN_PRECISION: u8 = 6;
/// Min safe trading size (0.00001) to calculate a price. This value considers
/// amount in decimal form with respective token precision.
pub const MIN_TRADE_SIZE: Decimal256 = Decimal256::raw(10000000000000);

/// Provides liquidity in the pair with the specified input parameters.
///
/// * **assets** is an array with assets available in the pool.
///
/// * **slippage_tolerance** is an optional parameter which is used to specify how much
/// the pool price can move until the provide liquidity transaction goes through.
///
/// * **auto_stake** is an optional parameter which determines whether the LP tokens minted after
/// liquidity provision are automatically staked in the Incentives contract on behalf of the LP token receiver.
///
/// * **receiver** is an optional parameter which defines the receiver of the LP tokens.
/// If no custom receiver is specified, the pair will mint LP tokens for the function caller.
///
/// NOTE - the address that wants to provide liquidity should approve the pair contract to pull its relevant tokens.
#[allow(clippy::too_many_arguments)]
pub fn provide_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    mut assets: Vec<Asset>,
    slippage_tolerance: Option<Decimal>,
    auto_stake: Option<bool>,
    receiver: Option<String>,
    min_lp_to_receive: Option<Uint128>,
    sender: CrossChainUser,
    tx_id: String,
    liquidity: PairWithAmount,
    slippage_tolerance_bps: u64,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    println!("provide liquidity");
    let total_share = Decimal256::new(Uint256::from(query_native_supply(
        &deps.querier,
        &config.pair_info.liquidity_token,
    )?));
    println!("provide liquidity2");
    let precisions = Precisions::new(deps.storage)?;

    println!("precisions: {:?}", precisions);
    let mut pools = query_pools(deps.querier, &env.contract.address, &config, &precisions)?;
    println!("provide liquidity2.5");

    let old_real_price = config.pool_state.price_state.last_price;

    let deposits = get_assets_with_precision(
        deps.as_ref(),
        &config,
        &mut assets,
        pools.clone(),
        &precisions,
    )?;
    println!("provide liquidity3");
    // info.funds
    //     .assert_coins_properly_sent(&assets, &config.pair_info.asset_infos)?;

    let mut messages = vec![];
    for (i, pool) in pools.iter_mut().enumerate() {
        // If the asset is a token contract, then we need to execute a TransferFrom msg to receive assets
        match &pool.info {
            AssetInfo::Cw20(contract_addr) => {
                if !deposits[i].is_zero() {
                    messages.push(CosmosMsg::Wasm(wasm_execute(
                        contract_addr,
                        &Cw20ExecuteMsg::TransferFrom {
                            owner: info.sender.to_string(),
                            recipient: env.contract.address.to_string(),
                            amount: deposits[i]
                                .to_uint(precisions.get_precision(&assets[i].info)?)
                                .map_err(|_| ContractError::Generic {
                                    err: "Conversion overflow".to_string(),
                                })?,
                        },
                        vec![],
                    )?))
                }
            }
            AssetInfo::Native { .. } => {
                println!("pool amount: {:?}", pool.amount);
                println!("deposit: {:?}", deposits[i]);
                // If the asset is native token, the pool balance is already increased
                // To calculate the total amount of deposits properly, we should subtract the user deposit from the pool
                // pool.amount = pool.amount.checked_sub(deposits[i])?;
            }
            _ => {}
        }
    }
    println!("provide liquidity4");
    let (share_uint128, slippage) = calculate_shares(
        &env,
        &mut config,
        &mut pools,
        total_share,
        deposits.clone(),
        slippage_tolerance,
    )?;

    // if total_share.is_zero() {
    //     messages.extend(mint_liquidity_token_message(
    //         deps.querier,
    //         &config,
    //         &env.contract.address,
    //         &env.contract.address,
    //         MINIMUM_LIQUIDITY_AMOUNT,
    //         false,
    //     )?);
    // }

    let min_amount_lp = min_lp_to_receive.unwrap_or_default();
    ensure!(
        share_uint128 >= min_amount_lp,
        ContractError::Generic {
            err: "Provide slippage violation".to_string(),
        }
    );

    // Mint LP tokens for the sender or for the receiver (if set)
    let receiver = receiver.unwrap_or_else(|| info.sender.clone().into_string());
    let auto_stake = auto_stake.unwrap_or(false);
    // messages.extend(mint_liquidity_token_message(
    //     deps.querier,
    //     &config,
    //     &env.contract.address,
    //     &Addr::unchecked(receiver.clone()),
    //     share_uint128,
    //     auto_stake,
    // )?);

    if config.track_asset_balances {
        for (i, pool) in pools.iter().enumerate() {
            CONCENTRATED_BALANCES.save(
                deps.storage,
                &pool.info,
                &pool
                    .amount
                    .checked_add(deposits[i])?
                    .to_uint(precisions.get_precision(&pool.info)?)
                    .map_err(|_| ContractError::Generic {
                        err: "Conversion overflow".to_string(),
                    })?,
                env.block.height,
            )?;
        }
    }

    accumulate_prices(&env, &mut config, old_real_price);

    CONFIG.save(deps.storage, &config)?;

    let mut mint_lp_tokens: Uint128 = Uint128::zero();
    for (i, pool) in pools.iter().enumerate() {
        mint_lp_tokens = mint_lp_tokens.checked_add(
            pool.amount
                .checked_add(deposits[i])?
                .to_uint(precisions.get_precision(&pool.info)?)
                .map_err(|_| ContractError::Generic {
                    err: "Conversion overflow".to_string(),
                })?,
        )?;
    }

    // Prepare Liquidity Response
    let liquidity_response = AddLiquidityResponse {
        mint_lp_tokens,
        vlp_address: env.contract.address.to_string(),
        tx_id: tx_id.clone(),
        sender: sender.clone(),
        pool_type: PoolType::Concentrated,
    };

    // Prepare acknowledgement
    let acknowledgement = to_json_binary(&liquidity_response)?;

    let attrs = vec![
        attr("action", "provide_liquidity"),
        attr("sender", info.sender),
        attr("receiver", receiver),
        attr("assets", format!("{}, {}", &assets[0], &assets[1])),
        attr("share", share_uint128),
        attr("slippage", slippage.to_string()),
    ];

    Ok(Response::new()
        .add_messages(messages)
        .add_attributes(attrs)
        .set_data(acknowledgement))
}

/// Performs an swap operation with the specified parameters. The trader must approve the
/// pool contract to transfer offer assets from their wallet.
///
/// * **sender** is the sender of the swap operation.
///
/// * **offer_asset** proposed asset for swapping.
///
/// * **belief_price** is used to calculate the maximum swap spread.
///
/// * **max_spread** sets the maximum spread of the swap operation.
///
/// * **to** sets the recipient of the swap operation.
pub fn swap(
    deps: DepsMut,
    env: Env,
    sender: Addr,
    offer_asset: Asset,
    belief_price: Option<Decimal>,
    max_spread: Option<Decimal>,
    to: Option<Addr>,
) -> Result<Response, ContractError> {
    let precisions = Precisions::new(deps.storage)?;
    let offer_asset_prec = precisions.get_precision(&offer_asset.info)?;
    let offer_asset_dec = offer_asset.to_decimal_asset(offer_asset_prec)?;
    let mut config = CONFIG.load(deps.storage)?;

    let mut pools = query_pools(deps.querier, &env.contract.address, &config, &precisions)?;

    let (offer_ind, _) = pools
        .iter()
        .find_position(|asset| asset.info == offer_asset_dec.info)
        .ok_or(ContractError::Generic {
            err: "Invalid asset".to_string(),
        })?;
    let ask_ind = 1 ^ offer_ind;
    let ask_asset_prec = precisions.get_precision(&pools[ask_ind].info)?;

    pools[offer_ind].amount -= offer_asset_dec.amount;

    before_swap_check(&pools, offer_asset_dec.amount)?;

    let mut xs = pools.iter().map(|asset| asset.amount).collect_vec();
    let old_real_price = calc_last_prices(&xs, &config, &env)?;

    // Get fee info from the factory
    let fee_info = query_fee_info(
        &deps.querier,
        &config.factory_addr,
        config.pair_info.pair_type.clone(),
    )?;
    let mut maker_fee_share = Decimal256::zero();
    if fee_info.fee_address.is_some() {
        maker_fee_share = fee_info.maker_fee_rate.into();
    }
    // If this pool is configured to share fees
    let mut share_fee_share = Decimal256::zero();
    if let Some(fee_share) = config.fee_share.clone() {
        share_fee_share = Decimal256::from_ratio(fee_share.bps, 10000u16);
    }

    let swap_result = compute_swap(
        &xs,
        offer_asset_dec.amount,
        ask_ind,
        &config,
        &env,
        maker_fee_share,
        share_fee_share,
    )?;
    xs[offer_ind] += offer_asset_dec.amount;
    xs[ask_ind] -= swap_result.dy + swap_result.maker_fee + swap_result.share_fee;

    let return_amount =
        swap_result
            .dy
            .to_uint(ask_asset_prec)
            .map_err(|_| ContractError::Generic {
                err: "Conversion overflow".to_string(),
            })?;
    let spread_amount = swap_result
        .spread_fee
        .to_uint(ask_asset_prec)
        .map_err(|_| ContractError::Generic {
            err: "Conversion overflow".to_string(),
        })?;
    assert_max_spread(
        belief_price,
        max_spread,
        offer_asset.amount,
        return_amount,
        spread_amount,
    )?;

    let total_share = query_native_supply(&deps.querier, &config.pair_info.liquidity_token)?
        .to_decimal256(LP_TOKEN_PRECISION)?;

    // Skip very small trade sizes which could significantly mess up the price due to rounding errors,
    // especially if token precisions are 18.
    if (swap_result.dy + swap_result.maker_fee + swap_result.share_fee) >= MIN_TRADE_SIZE
        && offer_asset_dec.amount >= MIN_TRADE_SIZE
    {
        let last_price = swap_result.calc_last_price(offer_asset_dec.amount, offer_ind);

        // update_price() works only with internal representation
        xs[1] *= config.pool_state.price_state.price_scale;
        config
            .pool_state
            .update_price(&config.pool_params, &env, total_share, &xs, last_price)?;
    }

    let receiver = to.unwrap_or_else(|| sender.clone());

    let mut messages = vec![Asset {
        info: pools[ask_ind].info.clone(),
        amount: return_amount,
    }
    .into_msg(&receiver)?];

    // Send the shared fee
    let mut fee_share_amount = Uint128::zero();
    if let Some(fee_share) = config.fee_share.clone() {
        fee_share_amount =
            swap_result
                .share_fee
                .to_uint(ask_asset_prec)
                .map_err(|_| ContractError::Generic {
                    err: "Conversion overflow".to_string(),
                })?;
        if !fee_share_amount.is_zero() {
            let fee = pools[ask_ind].info.with_balance(fee_share_amount);
            messages.push(fee.into_msg(fee_share.recipient)?);
        }
    }

    // Send the maker fee
    let mut maker_fee = Uint128::zero();
    if let Some(fee_address) = fee_info.fee_address {
        maker_fee =
            swap_result
                .maker_fee
                .to_uint(ask_asset_prec)
                .map_err(|_| ContractError::Generic {
                    err: "Conversion overflow".to_string(),
                })?;
        if !maker_fee.is_zero() {
            let fee = pools[ask_ind].info.with_balance(maker_fee);
            messages.push(fee.into_msg(fee_address)?);
        }
    }

    accumulate_prices(&env, &mut config, old_real_price);

    // Store observation from precommit data
    accumulate_swap_sizes(deps.storage, &env)?;

    // Store time series data in precommit observation.
    // Skipping small unsafe values which can seriously mess oracle price due to rounding errors.
    // This data will be reflected in observations in the next action.
    if offer_asset_dec.amount >= MIN_TRADE_SIZE && swap_result.dy >= MIN_TRADE_SIZE {
        let (base_amount, quote_amount) = if offer_ind == 0 {
            (offer_asset.amount, return_amount)
        } else {
            (return_amount, offer_asset.amount)
        };
        PrecommitObservation::save(deps.storage, &env, base_amount, quote_amount)?;
    }

    CONFIG.save(deps.storage, &config)?;

    if config.track_asset_balances {
        CONCENTRATED_BALANCES.save(
            deps.storage,
            &pools[offer_ind].info,
            &(pools[offer_ind].amount + offer_asset_dec.amount)
                .to_uint(offer_asset_prec)
                .map_err(|_| ContractError::Generic {
                    err: "Conversion overflow".to_string(),
                })?,
            env.block.height,
        )?;
        CONCENTRATED_BALANCES.save(
            deps.storage,
            &pools[ask_ind].info,
            &(pools[ask_ind].amount.to_uint(ask_asset_prec).map_err(|_| {
                ContractError::Generic {
                    err: "Conversion overflow".to_string(),
                }
            })? - return_amount
                - maker_fee
                - fee_share_amount),
            env.block.height,
        )?;
    }

    Ok(Response::new().add_messages(messages).add_attributes(vec![
        attr("action", "swap"),
        attr("sender", sender),
        attr("receiver", receiver),
        attr("offer_asset", offer_asset_dec.info.to_string()),
        attr("ask_asset", pools[ask_ind].info.to_string()),
        attr("offer_amount", offer_asset.amount),
        attr("return_amount", return_amount),
        attr("spread_amount", spread_amount),
        attr(
            "commission_amount",
            swap_result
                .total_fee
                .to_uint(ask_asset_prec)
                .map_err(|_| ContractError::Generic {
                    err: "Conversion overflow".to_string(),
                })?,
        ),
        attr("maker_fee_amount", maker_fee),
        attr("fee_share_amount", fee_share_amount),
    ]))
}

/// Withdraw liquidity from the pool.
///
/// * **sender** address that will receive assets back from the pair contract
///
/// * **assets** defines number of coins a user wants to withdraw per each asset.
pub fn withdraw_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    assets: Vec<Asset>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    let Coin { amount, denom } = one_coin(&info).map_err(|_| ContractError::Generic {
        err: "Missing denom".to_string(),
    })?;
    println!("liquidity token: {}", config.pair_info.liquidity_token);
    ensure_eq!(
        denom,
        config.pair_info.liquidity_token,
        ContractError::Generic {
            err: "Missing denom".to_string(),
        }
    );

    let precisions = Precisions::new(deps.storage)?;
    let pools = query_pools(
        deps.querier,
        &config.pair_info.contract_addr,
        &config,
        &precisions,
    )?;

    let total_share = query_native_supply(&deps.querier, &config.pair_info.liquidity_token)?;
    let mut messages = vec![];

    let refund_assets = if assets.is_empty() {
        // Usual withdraw (balanced)
        get_share_in_assets(&pools, amount.saturating_sub(Uint128::one()), total_share)
    } else {
        return Err(ContractError::Generic {
            err: "Imbalanced withdraw is currently disabled".to_string(),
        });
    };

    // decrease XCP
    let mut xs = pools.iter().map(|a| a.amount).collect_vec();

    xs[0] -= refund_assets[0].amount;
    xs[1] -= refund_assets[1].amount;
    xs[1] *= config.pool_state.price_state.price_scale;
    let amp_gamma = config.pool_state.get_amp_gamma(&env);
    let d = calc_d(&xs, &amp_gamma)?;
    config.pool_state.price_state.xcp_profit_real =
        get_xcp(d, config.pool_state.price_state.price_scale)
            / (total_share - amount).to_decimal256(LP_TOKEN_PRECISION)?;

    let refund_assets = refund_assets
        .into_iter()
        .map(|asset| {
            let prec = precisions.get_precision(&asset.info).unwrap();

            Ok(Asset {
                info: asset.info,
                amount: asset.amount.to_uint(prec)?,
            })
        })
        .collect::<StdResult<Vec<_>>>()?;

    messages.extend(
        refund_assets
            .iter()
            .cloned()
            .map(|asset| asset.into_msg(&info.sender))
            .collect::<StdResult<Vec<_>>>()?,
    );
    messages.push(tf_burn_msg(
        env.contract.address,
        coin(amount.u128(), config.pair_info.liquidity_token.to_string()),
    ));

    if config.track_asset_balances {
        for (i, pool) in pools.iter().enumerate() {
            CONCENTRATED_BALANCES.save(
                deps.storage,
                &pool.info,
                &pool
                    .amount
                    .to_uint(precisions.get_precision(&pool.info)?)
                    .map_err(|_| ContractError::Generic {
                        err: "Conversion overflow".to_string(),
                    })?
                    .checked_sub(refund_assets[i].amount)?,
                env.block.height,
            )?;
        }
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_messages(messages).add_attributes(vec![
        attr("action", "withdraw_liquidity"),
        attr("sender", info.sender),
        attr("withdrawn_share", amount),
        attr("refund_assets", refund_assets.iter().join(", ")),
    ]))
}
