use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    coin, from_json, wasm_execute, Addr, CosmosMsg, Decimal256, DepsMut, Env, Fraction, Order,
    QuerierWrapper, StdError, StdResult, Storage, Uint128, Uint64,
};
use cw_asset::AssetInfo;
use cw_storage_plus::{Item, Map, SnapshotMap};
use euclid::error::ContractError;
use euclid::msgs::concentrated_vlp::{
    tf_mint_msg, DecimalAsset, FeeShareConfig, PairInfo, PoolParams, PoolState,
};
use euclid::pool::State;
use euclid::utils::math::Decimal256Ext;
use euclid::{chain::ChainUid, token::Token};
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
            let precision = asset_info.decimals(&deps.querier, factory_addr)?;
            Self::PRECISIONS.save(deps.storage, asset_info.to_string().as_str(), &precision)?;
        }

        Ok(())
    }

    pub fn get_precision(&self, asset_info: &AssetInfo) -> Result<u8, ContractError> {
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
            asset
                .to_decimal_asset(precisions.get_precision(&asset.info)?)
                .map_err(Into::into)
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

/// Returns the configuration for the factory contract.
pub fn query_factory_config(
    querier: &QuerierWrapper,
    factory_contract: impl Into<String>,
) -> StdResult<Config> {
    if let Some(res) = querier.query_wasm_raw(factory_contract, b"config".as_slice())? {
        let res = from_json(res)?;
        Ok(res)
    } else {
        Err(StdError::generic_err("The factory config not found!"))
    }
}
