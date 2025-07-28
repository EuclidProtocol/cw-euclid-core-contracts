use std::fmt::{Display, Formatter};

use crate::{
    chain::{ChainUid, CrossChainUser},
    fee::{Fee, TotalFees},
    pool::{GetSwapResponse, PoolConfig},
    swap::NextSwapVlp,
    token::{Pair, PairWithAmount, Token},
};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary, Decimal, Decimal256, Uint128, Uint64};
use cw_asset::AssetInfo;
// The amplification factor for the stableswap invariant, default is 1000
pub const DEFAULT_AMP_FACTOR: Uint64 = Uint64::new(1000);
#[cw_serde]
pub struct InstantiateMsg {
    pub router: String,
    pub virtual_balance: String,
    pub pair: Pair,
    pub fee: Fee,
    pub execute: Option<ExecuteMsg>,
    pub admin: String,
    pub amp_factor: Option<Uint64>,
    // Concentrated VLP
    /// The pair type
    pub pair_type: PairType,
    /// Asset information for the assets in the pool
    pub asset_infos: Vec<AssetInfo>,
    /// The token contract code ID used for the tokens in the pool
    pub token_code_id: u64,
    /// The factory contract address
    pub factory_addr: String,
    /// Optional binary serialised parameters for custom pool types
    pub init_params: Option<Binary>,
}

#[cw_serde]
pub enum ExecuteMsg {
    // Registers a new pool from a new chain to an already existing VLP
    RegisterPool {
        sender: CrossChainUser,
        pair: Pair,
        tx_id: String,
    },

    UpdateFee {
        lp_fee_bps: Option<u64>,
        euclid_fee_bps: Option<u64>,
        recipient: Option<CrossChainUser>,
    },

    Swap {
        sender: CrossChainUser,
        tx_id: String,
        asset_in: Token,
        amount_in: Uint128,
        min_token_out: Uint128,
        next_swaps: Vec<NextSwapVlp>,
        test_fail: Option<bool>,
    },
    AddLiquidity {
        sender: CrossChainUser,
        tx_id: String,
        liquidity: PairWithAmount,
        slippage_tolerance_bps: u64,
    },
    RemoveLiquidity {
        sender: CrossChainUser,
        tx_id: String,
        lp_allocation: Uint128,
    },
    UpdateState {
        // Router Contract
        router: Option<String>,
        // Virtual Coin Contract
        virtual_balance: Option<String>,
        // Fee per swap for each transaction
        fee: Option<Fee>,
        // The last timestamp where the balances for each token have been updated
        last_updated: Option<u64>,
        admin: Option<String>,
        amp_factor: Option<Uint64>,
    },
}

#[cw_serde]
#[derive(QueryResponses)]

pub enum QueryMsg {
    #[returns(GetStateResponse)]
    State {},
    // Query to simulate a swap for the asset
    #[returns(GetSwapResponse)]
    SimulateSwap {
        asset: Token,
        asset_amount: Uint128,
        swaps: Vec<NextSwapVlp>,
    },
    // Queries the total reserve of the pair in the VLP
    #[returns(GetLiquidityResponse)]
    Liquidity {},

    // Queries the fee of this specific pool
    #[returns(FeeResponse)]
    Fee {},

    #[returns(TotalFeesResponse)]
    TotalFeesCollected {},

    #[returns(TotalFeesPerDenomResponse)]
    TotalFeesPerDenom { denom: String },

    // Queries the pool information for a chain id
    #[returns(StablePoolResponse)]
    Pool { chain_uid: ChainUid },
    // Query to get all pools
    #[returns(AllStablePoolsResponse)]
    GetAllPools {},
}

#[cw_serde]
pub struct GetStateResponse {
    pub pair: Pair,
    pub router: String,
    pub virtual_balance: String,
    pub fee: Fee,
    pub total_fees_collected: TotalFees,
    pub last_updated: u64,
    pub total_lp_tokens: Uint128,
    pub admin: String,
    pub pool_config: PoolConfig,
}

#[cw_serde]
pub struct GetLiquidityResponse {
    pub pair: Pair,
    pub token_1_reserve: Uint128,
    pub token_2_reserve: Uint128,
    pub total_lp_tokens: Uint128,
}

#[cw_serde]
pub struct FeeResponse {
    pub fee: Fee,
}

#[cw_serde]
pub struct TotalFeesResponse {
    pub total_fees: TotalFees,
}

#[cw_serde]
pub struct TotalFeesPerDenomResponse {
    pub lp_fees: Uint128,
    pub euclid_fees: Uint128,
}

#[cw_serde]
pub struct StablePoolResponse {
    pub lp_shares: Uint128,
    pub reserve_1: Uint128,
    pub reserve_2: Uint128,
}

#[cw_serde]
pub struct StablePoolInfo {
    pub chain_uid: ChainUid,
    pub pool: StablePoolResponse,
}
#[cw_serde]
pub struct AllStablePoolsResponse {
    pub pools: Vec<StablePoolInfo>,
}

#[cw_serde]
pub struct MigrateMsg {}

/// This struct describes a Terra asset as decimal.
#[cw_serde]
pub struct DecimalAsset {
    pub info: AssetInfo,
    pub amount: Decimal256,
}

/// Concentrated VLP
/// This enum describes available pair types.
/// ## Available pool types
/// ```
/// # use astroport::factory::PairType::{Custom, Stable, Xyk};
/// Xyk {};
/// Stable {};
/// Custom(String::from("Custom"));
/// ```
#[derive(Eq)]
#[cw_serde]
pub enum PairType {
    /// XYK pair type
    Xyk {},
    /// Stable pair type
    Stable {},
    /// Custom pair type
    Custom(String),
}

/// Returns a raw encoded string representing the name of each pool type
impl Display for PairType {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            PairType::Xyk {} => fmt.write_str("xyk"),
            PairType::Stable {} => fmt.write_str("stable"),
            PairType::Custom(pair_type) => fmt.write_str(format!("custom-{}", pair_type).as_str()),
        }
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

// impl PairInfo {
//     /// Returns the balance for each asset in the pool.
//     ///
//     /// * **contract_addr** is pair's pool address.
//     pub fn query_pools<C>(
//         &self,
//         querier: &QuerierWrapper<C>,
//         contract_addr: impl Into<String>,
//     ) -> StdResult<Vec<Asset>>
//     where
//         C: CustomQuery,
//     {
//         let contract_addr = contract_addr.into();
//         self.asset_infos
//             .iter()
//             .map(|asset_info| {
//                 Ok(Asset {
//                     info: asset_info.clone(),
//                     amount: asset_info.query_pool(querier, &contract_addr)?,
//                 })
//             })
//             .collect()
//     }
//     /// Returns the balance for each asset in the pool in decimal.
//     ///
//     /// * **contract_addr** is pair's pool address.
//     pub fn query_pools_decimal(
//         &self,
//         querier: &QuerierWrapper,
//         contract_addr: impl Into<String>,
//         factory_addr: &Addr,
//     ) -> StdResult<Vec<DecimalAsset>> {
//         let contract_addr = contract_addr.into();
//         self.asset_infos
//             .iter()
//             .map(|asset_info| {
//                 Ok(DecimalAsset {
//                     info: asset_info.clone(),
//                     amount: Decimal256::from_atomics(
//                         asset_info.query_pool(querier, &contract_addr)?,
//                         asset_info.decimals(querier, factory_addr)?.into(),
//                     )
//                     .map_err(|_| StdError::generic_err("Decimal256RangeExceeded"))?,
//                 })
//             })
//             .collect()
//     }
// }

/// This structure stores the pool parameters which may be adjusted via the `update_pool_params`.
#[cw_serde]
#[derive(Default)]
pub struct PoolParams {
    /// The minimum fee, charged when pool is fully balanced
    pub mid_fee: Decimal,
    /// The maximum fee, charged when pool is imbalanced
    pub out_fee: Decimal,
    /// Parameter that defines how gradual the fee changes from fee_mid to fee_out based on
    /// distance from price_scale
    pub fee_gamma: Decimal,
    /// Minimum profit before initiating a new repeg
    pub repeg_profit_threshold: Decimal,
    /// Minimum amount to change price_scale when repegging
    pub min_price_scale_delta: Decimal,
    /// Half-time used for calculating the price oracle
    pub ma_half_time: u64,
    #[serde(default)]
    /// Allowed xCP profit real drop per each .update_price() call
    pub allowed_xcp_profit_drop: Decimal,
    #[serde(default)]
    /// Total allowed xCP profit drop i.e. cap for `price_state.xcp_profit_losses`
    pub xcp_profit_losses_threshold: Decimal,
}

/// Structure which stores Amp and Gamma.
#[cw_serde]
#[derive(Default, Copy)]
pub struct AmpGamma {
    pub amp: Decimal,
    pub gamma: Decimal,
}

// impl AmpGamma {
//     /// Validates the parameters and creates a new object of the [`AmpGamma`] structure.
//     pub fn new(amp: Decimal, gamma: Decimal) -> Result<Self, PclError> {
//         validate_param("amp", amp, AMP_MIN, AMP_MAX)?;
//         validate_param("gamma", gamma, GAMMA_MIN, GAMMA_MAX)?;

//         Ok(AmpGamma { amp, gamma })
//     }
// }

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

/// Internal structure which stores the price state.
/// This structure cannot be updated via update_config.
#[cw_serde]
#[derive(Default)]
pub struct PriceState {
    /// Internal oracle price
    pub oracle_price: Decimal256,
    /// The last saved price
    pub last_price: Decimal256,
    /// Current price scale between 1st and 2nd assets.
    /// I.e. such C that x = C * y where x - 1st asset, y - 2nd asset.
    pub price_scale: Decimal256,
    /// Last timestamp when the price_oracle was updated.
    pub last_price_update: u64,
    /// Keeps track of positive change in xcp due to fees accruing
    pub xcp_profit: Decimal256,
    /// Profits due to fees inclusive of realized losses from rebalancing
    pub xcp_profit_real: Decimal256,
    #[serde(default)]
    /// Accounts for xCP profit real losses
    pub xcp_profit_losses: Decimal256,
}

/// This structure holds concentrated pool parameters.
#[cw_serde]
pub struct ConcentratedPoolParams {
    /// Amplification coefficient affects trades close to price_scale
    pub amp: Decimal,
    /// Affects how gradual the curve changes from constant sum to constant product
    /// as price moves away from price scale. Low values mean more gradual.
    pub gamma: Decimal,
    /// The minimum fee, charged when pool is fully balanced
    pub mid_fee: Decimal,
    /// The maximum fee, charged when pool is imbalanced
    pub out_fee: Decimal,
    /// Parameter that defines how gradual the fee changes from fee_mid to fee_out
    /// based on distance from price_scale.
    pub fee_gamma: Decimal,
    /// Minimum profit before initiating a new repeg
    pub repeg_profit_threshold: Decimal,
    /// Minimum amount to change price_scale when repegging.
    pub min_price_scale_delta: Decimal,
    /// 1 x\[0] = price_scale * x\[1].
    pub price_scale: Decimal,
    /// Half-time used for calculating the price oracle.
    pub ma_half_time: u64,
    /// Whether asset balances are tracked over blocks or not.
    /// They will not be tracked if the parameter is ignored.
    /// It can not be disabled later once enabled.
    pub track_asset_balances: Option<bool>,
    /// The config for swap fee sharing
    pub fee_share: Option<FeeShareConfig>,
    /// Allowed xCP profit real drop per each PCL repeg try
    pub allowed_xcp_profit_drop: Option<Decimal>,
    /// Total allowed xCP profit loss i.e. cap for `price_state.xcp_profit_losses`
    pub xcp_profit_losses_threshold: Option<Decimal>,
}

/// Holds the configuration for fee sharing
#[cw_serde]
pub struct FeeShareConfig {
    /// The fee shared with the address
    pub bps: u16,
    /// The share is sent to this address on every swap
    pub recipient: Addr,
}
