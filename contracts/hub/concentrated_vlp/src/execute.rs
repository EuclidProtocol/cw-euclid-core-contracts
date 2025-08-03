// Concentrated VLP

use cosmwasm_std::{
    attr, ensure, wasm_execute, CosmosMsg, Decimal, Decimal256, DepsMut, Env, MessageInfo,
    Response, Uint128, Uint256,
};
use cw_asset::{Asset, AssetInfo};
use euclid::{error::ContractError, msgs::concentrated_vlp::query_native_supply};
/// Minimum initial LP share
pub const MINIMUM_LIQUIDITY_AMOUNT: Uint128 = Uint128::new(1_000);
use crate::{
    state::{
        accumulate_prices, mint_liquidity_token_message, query_pools, CONCENTRATED_BALANCES, CONFIG,
    },
    utils::calculate_shares,
};

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
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    let total_share = Decimal256::new(Uint256::from(query_native_supply(
        &deps.querier,
        &config.pair_info.liquidity_token,
    )?));

    // let precisions = Precisions::new(deps.storage)?;

    let mut pools = query_pools(deps.querier, &env.contract.address, &config, &precisions)?;

    let old_real_price = config.pool_state.price_state.last_price;

    let deposits = get_assets_with_precision(
        deps.as_ref(),
        &config,
        &mut assets,
        pools.clone(),
        &precisions,
    )?;

    info.funds
        .assert_coins_properly_sent(&assets, &config.pair_info.asset_infos)?;

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
                                .to_uint(precisions.get_precision(&assets[i].info)?)?,
                        },
                        vec![],
                    )?))
                }
            }
            AssetInfo::Native { .. } => {
                // If the asset is native token, the pool balance is already increased
                // To calculate the total amount of deposits properly, we should subtract the user deposit from the pool
                pool.amount = pool.amount.checked_sub(deposits[i])?;
            }
        }
    }

    let (share_uint128, slippage) = calculate_shares(
        &env,
        &mut config,
        &mut pools,
        total_share,
        deposits.clone(),
        slippage_tolerance,
    )?;

    if total_share.is_zero() {
        messages.extend(mint_liquidity_token_message(
            deps.querier,
            &config,
            &env.contract.address,
            &env.contract.address,
            MINIMUM_LIQUIDITY_AMOUNT,
            false,
        )?);
    }

    let min_amount_lp = min_lp_to_receive.unwrap_or_default();
    ensure!(
        share_uint128 >= min_amount_lp,
        ContractError::ProvideSlippageViolation(share_uint128, min_amount_lp,)
    );

    // Mint LP tokens for the sender or for the receiver (if set)
    let receiver = addr_opt_validate(deps.api, &receiver)?.unwrap_or_else(|| info.sender.clone());
    let auto_stake = auto_stake.unwrap_or(false);
    messages.extend(mint_liquidity_token_message(
        deps.querier,
        &config,
        &env.contract.address,
        &receiver,
        share_uint128,
        auto_stake,
    )?);

    if config.track_asset_balances {
        for (i, pool) in pools.iter().enumerate() {
            CONCENTRATED_BALANCES.save(
                deps.storage,
                &pool.info,
                &pool
                    .amount
                    .checked_add(deposits[i])?
                    .to_uint(precisions.get_precision(&pool.info)?)?,
                env.block.height,
            )?;
        }
    }

    accumulate_prices(&env, &mut config, old_real_price);

    CONFIG.save(deps.storage, &config)?;

    let attrs = vec![
        attr("action", "provide_liquidity"),
        attr("sender", info.sender),
        attr("receiver", receiver),
        attr("assets", format!("{}, {}", &assets[0], &assets[1])),
        attr("share", share_uint128),
        attr("slippage", slippage.to_string()),
    ];

    Ok(Response::new().add_messages(messages).add_attributes(attrs))
}
