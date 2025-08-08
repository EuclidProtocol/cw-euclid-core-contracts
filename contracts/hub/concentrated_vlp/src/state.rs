use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    coin, ensure, from_json, to_json_binary, wasm_execute, Addr, BankMsg, CosmosMsg, CustomQuery,
    Decimal, Decimal256, DepsMut, Env, Fraction, Order, QuerierWrapper, StdError, StdResult,
    Storage, Uint128, Uint64, WasmMsg,
};
use cw20::{Cw20ExecuteMsg, Cw20QueryMsg, TokenInfoResponse};
use cw_asset::{Asset, AssetInfo};
use cw_storage_plus::{Item, Map, SnapshotMap};
use euclid::error::ContractError;
use euclid::msgs::concentrated_vlp::{
    query_balance, query_token_balance, tf_mint_msg, AmpGamma, CircularBuffer, DecimalAsset,
    FeeShareConfig, Observation, PairType, PoolParams, PriceState,
};
use euclid::pool::State;
use euclid::utils::math::Decimal256Ext;
use euclid::{chain::ChainUid, token::Token};

use crate::math::{calc_d, get_xcp, half_float_pow, TWO};
/// TWAP constant for external oracle prices
pub const TWAP_PRECISION_DEC: Decimal256 = Decimal256::raw((1e6 * 1e18) as u128);

pub const STATE: Item<State> = Item::new("state");

pub const CHAIN_LP_TOKENS: Map<ChainUid, Uint128> = Map::new("chain_lp_tokens");

pub const BALANCES: Map<Token, Uint128> = Map::new("balances");

// The amplification factor for the stableswap invariant, default is 1000
pub const AMP_FACTOR: Item<Uint64> = Item::new("amp_factor");

pub const COLLATERAL_LP_TOKENS: Item<Uint128> = Item::new("collateral_lp_tokens");

/// Concentrated VLP Config
/// Stores pool parameters and state.
pub const CONFIG: Item<Config> = Item::new("config");
/// Stores asset balances to query them later at any block height
pub const CONCENTRATED_BALANCES: SnapshotMap<&AssetInfo, Uint128> = SnapshotMap::new(
    "balances",
    "balances_check",
    "balances_change",
    cw_storage_plus::Strategy::EveryBlock,
);
/// Circular buffer to store trade size observations
pub const OBSERVATIONS: CircularBuffer<Observation> =
    CircularBuffer::new("observations_state", "observations_buffer");
/// This structure stores the concentrated pair parameters.
#[cw_serde]
pub struct Config {
    /// The pair information stored in a [`PairInfo`] struct
    pub pair_info: PairInfo,
    /// The factory contract address
    pub factory_addr: Addr,
    /// The last timestamp when the pair contract updated the asset cumulative prices
    pub block_time_last: u64,
    /// The vector contains cumulative prices for each pair of assets in the pool
    pub cumulative_prices: Vec<(AssetInfo, AssetInfo, Uint128)>,
    /// Pool parameters
    pub pool_params: PoolParams,
    /// Pool state
    pub pool_state: PoolState,
    /// Pool's owner
    pub owner: Option<Addr>,
    /// Whether asset balances are tracked over blocks or not.
    pub track_asset_balances: bool,
    /// The config for swap fee sharing
    pub fee_share: Option<FeeShareConfig>,
    /// The tracker contract address
    pub tracker_addr: Option<Addr>,
}

/// The first key is denom, the second key is a precision.
pub const COINS_INFO: Map<String, u8> = Map::new("coins_info");
#[derive(Debug)]
pub struct Precisions(Vec<(String, u8)>);

impl Precisions {
    /// Stores map of AssetInfo (as String) -> precision
    pub const PRECISIONS: Map<&str, u8> = Map::new("precisions");
    pub fn new(storage: &dyn Storage) -> StdResult<Self> {
        let items = Self::PRECISIONS
            .range(storage, None, None, Order::Ascending)
            .collect::<StdResult<Vec<_>>>()?;

        Ok(Self(items))
    }

    /// Store all token precisions
    pub fn store_precisions(
        deps: DepsMut,
        asset_infos: &[AssetInfo],
        factory_addr: &Addr,
    ) -> StdResult<()> {
        for asset_info in asset_infos {
            let precision = query_token_precision(&deps.querier, asset_info, factory_addr)?;
            Self::PRECISIONS.save(deps.storage, asset_info.to_string().as_str(), &precision)?;
        }

        Ok(())
    }

    pub fn get_precision(&self, asset_info: &AssetInfo) -> Result<u8, ContractError> {
        println!("asset info: {:?}", asset_info);
        println!("self: {:?}", self);
        self.0
            .iter()
            .find_map(|(info, prec)| {
                if info == &asset_info.to_string() {
                    Some(*prec)
                } else {
                    None
                }
            })
            .ok_or_else(|| ContractError::Generic {
                err: "Invalid asset".to_string(),
            })
    }
}
/// This structure stores the main parameters for an Astroport pair
#[cw_serde]
pub struct PairInfo {
    /// Asset information for the assets in the pool
    pub asset_infos: Vec<AssetInfo>,
    /// Pair contract address
    pub contract_addr: Addr,
    /// Pair LP token denom
    pub liquidity_token: String,
    /// The pool type (xyk, stableswap etc) available in [`PairType`]
    pub pair_type: PairType,
}
impl PairInfo {
    /// Returns the balance for each asset in the pool.
    ///
    /// * **contract_addr** is pair's pool address.
    pub fn query_pools(
        &self,
        querier: &QuerierWrapper,
        contract_addr: impl Into<String>,
    ) -> StdResult<Vec<Asset>> {
        let contract_addr = contract_addr.into();
        self.asset_infos
            .iter()
            .map(|asset_info| {
                Ok(Asset {
                    info: asset_info.clone(),
                    amount: asset_info.query_balance(querier, &contract_addr).unwrap(),
                })
            })
            .collect()
    }

    /// Returns the balance for each asset in the pool in decimal.
    ///
    /// * **contract_addr** is pair's pool address.
    pub fn query_pools_decimal(
        &self,
        querier: &QuerierWrapper,
        contract_addr: impl Into<String>,
        factory_addr: &Addr,
    ) -> StdResult<Vec<DecimalAsset>> {
        let contract_addr = contract_addr.into();
        self.asset_infos
            .iter()
            .map(|asset_info| {
                Ok(DecimalAsset {
                    info: asset_info.clone(),
                    amount: Decimal256::from_atomics(
                        asset_info.query_pool(querier, &contract_addr)?,
                        query_token_precision(querier, asset_info, factory_addr)?.into(),
                    )
                    .map_err(|_| StdError::generic_err("Decimal256RangeExceeded"))?,
                })
            })
            .collect()
    }
}
pub trait AssetExt {
    fn to_decimal_asset(&self, precision: impl Into<u32>) -> StdResult<DecimalAsset>;
    fn into_msg<T>(self, recipient: impl Into<String>) -> StdResult<CosmosMsg<T>>;
}

impl AssetExt for Asset {
    fn to_decimal_asset(&self, precision: impl Into<u32>) -> StdResult<DecimalAsset> {
        Ok(DecimalAsset {
            info: self.info.clone(),
            amount: Decimal256::with_precision(self.amount, precision.into())?,
        })
    }
    /// For native tokens of type [`AssetInfo`] uses the default method [`BankMsg::Send`] to send a
    /// token amount to a recipient.
    /// For a token of type [`AssetInfo`] we use the default method [`Cw20ExecuteMsg::Transfer`].
    fn into_msg<T>(self, recipient: impl Into<String>) -> StdResult<CosmosMsg<T>> {
        let recipient = recipient.into();
        match &self.info {
            AssetInfo::Cw20(contract_addr) => Ok(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: contract_addr.to_string(),
                msg: to_json_binary(&Cw20ExecuteMsg::Transfer {
                    recipient,
                    amount: self.amount,
                })?,
                funds: vec![],
            })),
            AssetInfo::Native(denom) => Ok(CosmosMsg::Bank(BankMsg::Send {
                to_address: recipient,
                amount: vec![coin(self.amount.u128(), denom.to_string())],
            })),
            _ => Err(StdError::generic_err("Invalid asset info")),
        }
    }
}

pub trait AssetInfoExt {
    fn query_pool(
        &self,
        querier: &QuerierWrapper,
        pool_addr: impl Into<String>,
    ) -> StdResult<Uint128>;
    fn with_balance(&self, balance: impl Into<Uint128>) -> Asset;
}

impl AssetInfoExt for AssetInfo {
    fn query_pool(
        &self,
        querier: &QuerierWrapper,
        pool_addr: impl Into<String>,
    ) -> StdResult<Uint128> {
        let pool_addr = pool_addr.into();

        match self {
            AssetInfo::Cw20(contract_addr) => {
                query_token_balance(querier, contract_addr, &pool_addr)
            }
            AssetInfo::Native(denom) => query_balance(querier, &pool_addr, denom),
            _ => Err(StdError::generic_err("Invalid asset info")),
        }
    }
    fn with_balance(&self, balance: impl Into<Uint128>) -> Asset {
        Asset {
            info: self.clone(),
            amount: balance.into(),
        }
    }
}

/// Returns the number of decimals that a token has.
///
/// * **asset_info** is an object of type [`AssetInfo`] and contains the asset details for a specific token.
pub fn query_token_precision<C>(
    querier: &QuerierWrapper<C>,
    asset_info: &AssetInfo,
    factory_addr: &Addr,
) -> StdResult<u8>
where
    C: CustomQuery,
{
    Ok(match asset_info {
        AssetInfo::Native(denom) => {
            // For native tokens, we need to query the factory config to get coin registry
            // For now, return a default precision of 6 for native tokens
            6u8
        }
        AssetInfo::Cw20(contract_addr) => {
            let res: TokenInfoResponse =
                querier.query_wasm_smart(contract_addr, &Cw20QueryMsg::TokenInfo {})?;

            res.decimals
        }
        _ => {
            return Err(StdError::generic_err("Unsupported asset info type"));
        }
    })
}

/// Returns the configuration for the factory contract.
pub fn query_factory_config<C>(
    querier: &QuerierWrapper<C>,
    factory_contract: impl Into<String>,
) -> StdResult<Config>
where
    C: CustomQuery,
{
    if let Some(res) = querier.query_wasm_raw(factory_contract, b"config".as_slice())? {
        let res = from_json(res)?;
        Ok(res)
    } else {
        Err(StdError::generic_err("The factory config not found!"))
    }
}

/// Returns current pool's volumes where amount is in [`Decimal256`] form.
pub(crate) fn query_pools(
    querier: QuerierWrapper,
    addr: &Addr,
    config: &Config,
    precisions: &Precisions,
) -> Result<Vec<DecimalAsset>, ContractError> {
    config
        .pair_info
        .query_pools(&querier, addr)?
        .into_iter()
        .map(|asset| {
            let precision = precisions.get_precision(&asset.info)?;
            Ok(DecimalAsset {
                info: asset.info,
                amount: Decimal256::from_atomics(asset.amount, precision.into())
                    .map_err(|_| StdError::generic_err("Decimal256RangeExceeded"))?,
            })
        })
        .collect()
}

/// Accumulate token prices for the assets in the pool.
pub fn accumulate_prices(env: &Env, config: &mut Config, last_real_price: Decimal256) {
    let block_time = env.block.time.seconds();
    if block_time <= config.block_time_last {
        return;
    }

    let time_elapsed = Uint128::from(block_time - config.block_time_last);

    for (from, _, value) in config.cumulative_prices.iter_mut() {
        let price = if &config.pair_info.asset_infos[0] == from {
            last_real_price.inv().unwrap()
        } else {
            last_real_price
        };
        // Price max value = 1e18 bc smallest value in Decimal is 1e-18.
        // Thus highest inverted price is 1/1e-18.
        // (price * twap) max value = 1e24 which fits into Uint128 thus we use unwrap here
        let price: Uint128 = (price * TWAP_PRECISION_DEC)
            .to_uint128_with_precision(0u8)
            .unwrap();
        // time_elapsed * price does not need checked_mul.
        // price max value = 1e24, u128 max value = 340282366920938463463374607431768211455
        // overflow is possible if time_elapsed > 340282366920939 seconds ~ 10790283 years
        *value = value.wrapping_add(time_elapsed * price);
    }

    config.block_time_last = block_time;
}

/// Mint LP tokens for a beneficiary and auto stake the tokens in the Incentive contract (if auto staking is specified).
///
/// * **recipient** LP token recipient.
///
/// * **coin** denom and amount of LP tokens that will be minted for the recipient.
///
/// * **auto_stake** determines whether the newly minted LP tokens will
/// be automatically staked in the Incentives Contract on behalf of the recipient.
pub fn mint_liquidity_token_message(
    querier: QuerierWrapper,
    config: &Config,
    contract_address: &Addr,
    recipient: &Addr,
    amount: Uint128,
    auto_stake: bool, // set this to false for now
) -> Result<Vec<CosmosMsg>, ContractError> {
    let coin = coin(amount.into(), config.pair_info.liquidity_token.to_string());

    // If no auto-stake - just mint to recipient
    if !auto_stake {
        return Ok(tf_mint_msg(contract_address, coin, recipient));
    } else {
        Err(ContractError::Generic {
            err: "Auto staking is not supported".to_string(),
        })
    }

    // // Mint for the pair contract and stake into the Incentives contract
    // let incentives_addr = query_factory_config(&querier, &config.factory_addr)?.generator_address;

    // if let Some(address) = incentives_addr {
    //     let mut msgs = tf_mint_msg(contract_address, coin.clone(), contract_address);
    //     msgs.push(
    //         wasm_execute(
    //             address,
    //             &IncentiveExecuteMsg::Deposit {
    //                 recipient: Some(recipient.to_string()),
    //             },
    //             vec![coin],
    //         )?
    //         .into(),
    //     );
    //     Ok(msgs)
    // } else {
    //     Err(ContractError::Generic {
    //         err: "Auto staking is not supported".to_string(),
    //     })
    // }
}

/// Internal structure which stores the pool's state.
#[cw_serde]
pub struct PoolState {
    /// Initial Amp and Gamma
    pub initial: AmpGamma,
    /// Future Amp and Gamma
    pub future: AmpGamma,
    /// Timestamp when Amp and Gamma should become equal to self.future
    pub future_time: u64,
    /// Timestamp when Amp and Gamma started being changed
    pub initial_time: u64,
    /// Current price state
    pub price_state: PriceState,
}

impl PoolState {
    /// Calculates current amp and gamma.
    /// This function handles parameters upgrade as well as downgrade.
    /// If block time >= self.future_time then it returns self.future parameters.
    pub fn get_amp_gamma(&self, env: &Env) -> AmpGamma {
        let block_time = env.block.time.seconds();
        if block_time < self.future_time {
            let total = Decimal::new((self.future_time - self.initial_time).into());
            let passed = Decimal::new((block_time - self.initial_time).into());
            let left = total - passed;

            // A1 = A0 + (A1 - A0) * (block_time - t_init) / (t_end - t_init) -> simplified to:
            // A1 = ( A0 * (t_end - block_time) + A1 * (block_time - t_init) ) / (t_end - t_init)
            let amp = (self.initial.amp * left + self.future.amp * passed) / total;
            let gamma = (self.initial.gamma * left + self.future.gamma * passed) / total;

            AmpGamma { amp, gamma }
        } else {
            AmpGamma {
                amp: self.future.amp,
                gamma: self.future.gamma,
            }
        }
    }
    /// The function is responsible for repegging mechanism.
    /// It updates internal oracle price and adjusts price scale.
    ///
    /// * **total_lp** total LP tokens were minted
    /// * **cur_xs** - internal representation of pool volumes
    /// * **cur_price** - last price happened in the previous action (swap, provide or withdraw)
    pub fn update_price(
        &mut self,
        pool_params: &PoolParams,
        env: &Env,
        total_lp: Decimal256,
        cur_xs: &[Decimal256],
        cur_price: Decimal256,
    ) -> StdResult<()> {
        let amp_gamma = self.get_amp_gamma(env);
        let block_time = env.block.time.seconds();
        let price_state = &mut self.price_state;

        if price_state.last_price_update < block_time {
            let arg = Decimal256::from_ratio(
                block_time - price_state.last_price_update,
                pool_params.ma_half_time,
            );
            let alpha = half_float_pow(arg)?;
            price_state.oracle_price = price_state.last_price * (Decimal256::one() - alpha)
                + price_state.oracle_price * alpha;
            price_state.last_price_update = block_time;
        }
        price_state.last_price = cur_price;

        let cur_d = calc_d(cur_xs, &amp_gamma)?;
        let xcp = get_xcp(cur_d, price_state.price_scale);

        if !price_state.xcp_profit_real.is_zero() {
            let xcp_profit_real = xcp / total_lp;

            // If xcp dropped and no ramping happens,
            // then the pool either now or previously lost a fraction of its liquidity
            // which somehow bypassed the PCL curve.
            if block_time >= self.future_time {
                if xcp_profit_real < price_state.xcp_profit_real {
                    let losses = price_state.xcp_profit_real - xcp_profit_real;
                    if losses > Decimal256::from(pool_params.allowed_xcp_profit_drop) {
                        return Err(StdError::generic_err(
                            "XCP profit real value dropped. This action makes loss",
                        ));
                    } else {
                        price_state.xcp_profit_losses += losses;
                    }
                } else {
                    let gain = xcp_profit_real - price_state.xcp_profit_real;
                    price_state.xcp_profit_losses =
                        price_state.xcp_profit_losses.saturating_sub(gain);
                }

                ensure!(
                    price_state.xcp_profit_losses
                        <= Decimal256::from(pool_params.xcp_profit_losses_threshold),
                    StdError::generic_err("PCL has reached the limit of losses")
                );
            }

            price_state.xcp_profit =
                price_state.xcp_profit * xcp_profit_real / price_state.xcp_profit_real;
            price_state.xcp_profit_real = xcp_profit_real;
        }

        let xcp_profit = price_state.xcp_profit;

        let norm = (price_state.oracle_price / price_state.price_scale).abs_diff(Decimal256::one());
        let scale_delta = Decimal256::from(pool_params.min_price_scale_delta)
            .max(norm * Decimal256::from_ratio(1u8, 10u8));

        if norm >= scale_delta
            && price_state
                .xcp_profit_real
                .saturating_sub(Decimal256::one())
                > xcp_profit.saturating_sub(Decimal256::one()) / TWO
                    + Decimal256::from(pool_params.repeg_profit_threshold)
        {
            let numerator = price_state.price_scale * (norm - scale_delta)
                + scale_delta * price_state.oracle_price;
            let price_scale_new = numerator / norm;

            let xs = [
                cur_xs[0],
                cur_xs[1] * price_scale_new / price_state.price_scale,
            ];
            let new_d = calc_d(&xs, &amp_gamma)?;

            let new_xcp = get_xcp(new_d, price_scale_new);
            let new_xcp_profit_real = new_xcp / total_lp;

            if TWO * new_xcp_profit_real > xcp_profit + Decimal256::one() {
                price_state.price_scale = price_scale_new;
                price_state.xcp_profit_real = new_xcp_profit_real;
            };
        }

        Ok(())
    }
}
