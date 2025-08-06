use std::{
    collections::HashMap,
    fmt::{Display, Formatter},
    marker::PhantomData,
};

use crate::{
    chain::{ChainUid, CrossChainUser},
    fee::{Fee, TotalFees},
    pool::{GetSwapResponse, PoolConfig},
    swap::NextSwapVlp,
    token::{Pair, PairWithAmount, Token},
};
pub use cosmos_sdk_proto::cosmos::base::v1beta1::Coin as ProtoCoin;
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{
    ensure, Addr, BankMsg, Binary, Coin, ConversionOverflowError, CosmosMsg, CustomMsg,
    CustomQuery, Decimal, Decimal256, Env, Fraction, QuerierWrapper, StdError, StdResult, Storage,
    Uint128, Uint256, Uint64,
};

use cw20::{BalanceResponse as Cw20BalanceResponse, Cw20QueryMsg};
use cw_asset::{Asset, AssetInfo, AssetInfoBase};
use cw_storage_plus::{Item, Map};
use prost::Message;
use serde::{de::DeserializeOwned, Serialize};
// The amplification factor for the stableswap invariant, default is 1000
pub const DEFAULT_AMP_FACTOR: Uint64 = Uint64::new(1000);
pub const TYPE_URL: &'static str = "/osmosis.tokenfactory.v1beta1.MsgMint";
/// Defines fee tolerance. If k coefficient is small enough then k = 0. (0.001)
pub const FEE_TOL: Decimal256 = Decimal256::raw(1000000000000000);
/// N ^ 2
pub const N_POW2: Decimal256 = Decimal256::raw(4000000000000000000);

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
        sender: Addr,
        offer_asset: Asset,
        belief_price: Option<Decimal>,
        max_spread: Option<Decimal>,
        to: Option<Addr>,
    },
    AddLiquidity {
        assets: Vec<Asset>,
        slippage_tolerance: Option<Decimal>,
        auto_stake: Option<bool>,
        receiver: Option<String>,
        min_lp_to_receive: Option<Uint128>,
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

/// Returns a token balance for an account.
///
/// * **contract_addr** token contract for which we return a balance.
///
/// * **account_addr** account address for which we return a balance.
pub fn query_token_balance<C>(
    querier: &QuerierWrapper<C>,
    contract_addr: impl Into<String>,
    account_addr: impl Into<String>,
) -> StdResult<Uint128>
where
    C: CustomQuery,
{
    // load balance from the token contract
    let resp: Cw20BalanceResponse = querier
        .query_wasm_smart(
            contract_addr,
            &Cw20QueryMsg::Balance {
                address: account_addr.into(),
            },
        )
        .unwrap_or_else(|_| Cw20BalanceResponse {
            balance: Uint128::zero(),
        });

    Ok(resp.balance)
}

/// Returns a native token's balance for a specific account.
///
/// * **denom** specifies the denomination used to return the balance (e.g uluna).
pub fn query_balance<C>(
    querier: &QuerierWrapper<C>,
    account_addr: impl Into<String>,
    denom: impl Into<String>,
) -> StdResult<Uint128>
where
    C: CustomQuery,
{
    querier
        .query_balance(account_addr, denom)
        .map(|coin| coin.amount)
}

/// This structure describes the available query messages for the factory contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum FactoryQueryMsg {
    // /// Config returns contract settings specified in the custom [`ConfigResponse`] structure.
    // #[returns(ConfigResponse)]
    // Config {},
    // /// Pair returns information about a specific pair according to the specified assets.
    // #[returns(PairInfo)]
    // Pair {
    //     /// The assets for which we return a pair
    //     asset_infos: Vec<AssetInfo>,
    // },
    // /// Pairs returns an array of pairs and their information according to the specified parameters in `start_after` and `limit` variables.
    // #[returns(PairsResponse)]
    // Pairs {
    //     /// The pair item to start reading from. It is an [`Option`] type that accepts [`AssetInfo`] elements.
    //     start_after: Option<Vec<AssetInfo>>,
    //     /// The number of pairs to read and return. It is an [`Option`] type.
    //     limit: Option<u32>,
    // },
    /// FeeInfo returns fee parameters for a specific pair. The response is returned using a [`FeeInfoResponse`] structure
    #[returns(FeeInfoResponse)]
    FeeInfo {
        /// The pair type for which we return fee information. Pair type is a [`PairType`] struct
        pair_type: PairType,
    },
    // /// Returns a vector that contains blacklisted pair types
    // #[returns(Vec<PairType>)]
    // BlacklistedPairTypes {},
    // #[returns(TrackerConfig)]
    // TrackerConfig {},
}

/// This structure holds parameters that describe the fee structure for a pool.
#[derive(Clone)]
pub struct FeeInfo {
    /// The fee address
    pub fee_address: Option<Addr>,
    /// The total amount of fees charged per swap
    pub total_fee_rate: Decimal,
    /// The amount of fees sent to the Maker contract
    pub maker_fee_rate: Decimal,
}

/// Returns the fee information for a specific pair type.
///
/// * **pair_type** pair type we query information for.
pub fn query_fee_info<C>(
    querier: &QuerierWrapper<C>,
    factory_contract: impl Into<String>,
    pair_type: PairType,
) -> StdResult<FeeInfo>
where
    C: CustomQuery,
{
    let res: FeeInfoResponse =
        querier.query_wasm_smart(factory_contract, &FactoryQueryMsg::FeeInfo { pair_type })?;

    Ok(FeeInfo {
        fee_address: res.fee_address,
        total_fee_rate: Decimal::from_ratio(res.total_fee_bps, 10000u16),
        maker_fee_rate: Decimal::from_ratio(res.maker_fee_bps, 10000u16),
    })
}

/// A custom struct for each query response that returns an object of type [`FeeInfoResponse`].
#[cw_serde]
pub struct FeeInfoResponse {
    /// Contract address to send governance fees to
    pub fee_address: Option<Addr>,
    /// Total amount of fees (in bps) charged on a swap
    pub total_fee_bps: u16,
    /// Amount of fees (in bps) sent to the Maker contract
    pub maker_fee_bps: u16,
}

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

impl PoolParams {
    pub fn fee(&self, xp: &[Decimal256]) -> Decimal256 {
        let fee_gamma: Decimal256 = self.fee_gamma.into();
        let sum = xp[0] + xp[1];
        let mut k = xp[0] * xp[1] * N_POW2 / sum.pow(2);
        k = fee_gamma / (fee_gamma + Decimal256::one() - k);

        if k <= FEE_TOL {
            k = Decimal256::zero()
        }

        k * Decimal256::from(self.mid_fee)
            + (Decimal256::one() - k) * Decimal256::from(self.out_fee)
    }
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

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgCreateDenom {
    #[prost(string, tag = "1")]
    pub sender: ::prost::alloc::string::String,
    /// subdenom can be up to 44 "alphanumeric" characters long.
    #[prost(string, tag = "2")]
    pub subdenom: ::prost::alloc::string::String,
}

impl MsgCreateDenom {
    #[cfg(not(feature = "injective"))]
    pub const TYPE_URL: &'static str = "/osmosis.tokenfactory.v1beta1.MsgCreateDenom";
    #[cfg(feature = "injective")]
    pub const TYPE_URL: &'static str = "/injective.tokenfactory.v1beta1.MsgCreateDenom";
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgCreateDenomResponse {
    #[prost(string, tag = "1")]
    pub new_token_denom: ::prost::alloc::string::String,
}

impl MsgCreateDenomResponse {
    pub fn to_proto_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        self.encode(&mut buf).unwrap();
        buf
    }
}

impl From<MsgCreateDenomResponse> for Binary {
    fn from(msg: MsgCreateDenomResponse) -> Self {
        Binary::from(msg.to_proto_bytes())
    }
}

impl TryFrom<Binary> for MsgCreateDenomResponse {
    type Error = StdError;
    fn try_from(binary: Binary) -> Result<Self, Self::Error> {
        Self::decode(binary.as_slice()).map_err(|e| {
            StdError::generic_err(
                format!(
                    "MsgCreateDenomResponse Unable to decode binary: \n  - base64: {}\n  - bytes array: {:?}\n\n{:?}",
                    binary,
                    binary.to_vec(),
                    e
                ),
            )
        })
    }
}

// impl TryFrom<Binary> for MsgCreateDenom {
//     type Error = StdError;
//     fn try_from(binary: Binary) -> Result<Self, Self::Error> {
//         Self::decode(binary.as_slice()).map_err(|e| {
//             StdError::generic_err(format!(
//                 "MsgCreateDenom Unable to decode binary: \n  - base64: {}\n  - bytes array: {:?}\n\n{:?}",
//                 binary,
//                 binary.to_vec(),
//                 e
//             ))
//         })
//     }
// }

pub fn tf_create_denom_msg<T>(sender: impl Into<String>, denom: impl Into<String>) -> CosmosMsg<T>
where
    T: CustomMsg,
{
    let create_denom_msg = MsgCreateDenom {
        sender: sender.into(),
        subdenom: denom.into(),
    };

    CosmosMsg::Stargate {
        type_url: MsgCreateDenom::TYPE_URL.to_string(),
        value: Binary::from(create_denom_msg.encode_to_vec()),
    }
}

/// Returns the total supply of a native token.
///
/// * **denom** specifies the denomination used to return the supply (e.g uatom).
pub fn query_native_supply<C>(
    querier: &QuerierWrapper<C>,
    denom: impl Into<String>,
) -> StdResult<Uint128>
where
    C: CustomQuery,
{
    querier.query_supply(denom).map(|res| res.amount)
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
                StdError::generic_err(format!("Error converting "))
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

#[cfg(not(feature = "injective"))]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgMint {
    #[prost(string, tag = "1")]
    pub sender: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "2")]
    pub amount: ::core::option::Option<cosmos_sdk_proto::cosmos::base::v1beta1::Coin>,
    #[prost(string, tag = "3")]
    pub mint_to_address: ::prost::alloc::string::String,
}

impl MsgMint {
    #[cfg(not(feature = "injective"))]
    pub const TYPE_URL: &'static str = "/osmosis.tokenfactory.v1beta1.MsgMint";
    // #[cfg(feature = "injective")]
    // pub const TYPE_URL: &'static str = "/injective.tokenfactory.v1beta1.MsgMint";
}

pub fn tf_mint_msg(
    sender: impl Into<String>,
    coin: Coin,
    receiver: impl Into<String>,
) -> Vec<CosmosMsg> {
    let sender_addr: String = sender.into();
    let receiver_addr: String = receiver.into();

    // #[cfg(not(feature = "injective"))]
    let mint_msg = MsgMint {
        sender: sender_addr.clone(),
        amount: Some(ProtoCoin {
            denom: coin.denom.to_string(),
            amount: coin.amount.to_string(),
        }),
        mint_to_address: receiver_addr.clone(),
    };

    // #[cfg(feature = "injective")]
    // let mint_msg = MsgMint {
    //     sender: sender_addr.clone(),
    //     amount: Some(ProtoCoin {
    //         denom: coin.denom.to_string(),
    //         amount: coin.amount.to_string(),
    //     }),
    // };

    // #[cfg(not(feature = "injective"))]
    // return vec![CosmosMsg::Stargate {
    //     type_url: MsgMint::TYPE_URL.to_string(),
    //     value: Binary::from(mint_msg.encode_to_vec()),
    // }];

    // #[cfg(feature = "injective")]
    if sender_addr == receiver_addr {
        vec![CosmosMsg::Stargate {
            type_url: MsgMint::TYPE_URL.to_string(),
            value: Binary::from(mint_msg.encode_to_vec()),
        }]
    } else {
        vec![
            CosmosMsg::Stargate {
                type_url: MsgMint::TYPE_URL.to_string(),
                value: Binary::from(mint_msg.encode_to_vec()),
            },
            BankMsg::Send {
                to_address: receiver_addr,
                amount: vec![coin],
            }
            .into(),
        ]
    }
}

pub trait DecimalToInteger<T> {
    fn to_uint(self, precision: impl Into<u32>) -> Result<T, ConversionOverflowError>;
}

impl DecimalToInteger<Uint128> for Decimal256 {
    fn to_uint(self, precision: impl Into<u32>) -> Result<Uint128, ConversionOverflowError> {
        let multiplier = Uint256::from(10u8).pow(precision.into());
        (multiplier * self.numerator() / self.denominator()).try_into()
    }
}
pub trait IntegerToDecimal
where
    Self: Copy + Into<Uint128> + Into<Uint256>,
{
    fn to_decimal(self) -> Decimal {
        Decimal::from_ratio(self, 1u8)
    }

    fn to_decimal256(self, precision: impl Into<u32>) -> StdResult<Decimal256> {
        Decimal256::with_precision(self, precision)
    }
}
impl IntegerToDecimal for u64 {}
impl IntegerToDecimal for Uint128 {}

#[cw_serde]
pub struct PrecommitObservation {
    pub base_amount: Uint128,
    pub quote_amount: Uint128,
    pub precommit_ts: u64,
}

impl PrecommitObservation {
    /// Temporal storage for observation which should be committed in the next block
    const PRECOMMIT_OBSERVATION: Item<PrecommitObservation> = Item::new("precommit_observation");

    pub fn save(
        storage: &mut dyn Storage,
        env: &Env,
        base_amount: Uint128,
        quote_amount: Uint128,
    ) -> StdResult<()> {
        let next_obs = match Self::may_load(storage)? {
            // Accumulating observations at the same block
            Some(mut prev_obs) if env.block.time.seconds() == prev_obs.precommit_ts => {
                prev_obs.base_amount += base_amount;
                prev_obs.quote_amount += quote_amount;
                prev_obs
            }
            _ => PrecommitObservation {
                base_amount,
                quote_amount,
                precommit_ts: env.block.time.seconds(),
            },
        };

        Self::PRECOMMIT_OBSERVATION.save(storage, &next_obs)
    }

    #[inline]
    pub fn may_load(storage: &dyn Storage) -> StdResult<Option<Self>> {
        Self::PRECOMMIT_OBSERVATION.may_load(storage)
    }
}

#[cw_serde]
pub struct BufferState {
    capacity: u32,
    head: u32,
}

pub struct CircularBuffer<V> {
    state_key: &'static str,
    array_namespace: &'static str,
    data_type: PhantomData<V>,
}

impl<V> CircularBuffer<V> {
    pub const fn new(state_key: &'static str, array_namespace: &'static str) -> Self {
        Self {
            state_key,
            array_namespace,
            data_type: PhantomData,
        }
    }

    pub const fn state(&self) -> Item<BufferState> {
        Item::new(self.state_key)
    }

    pub const fn array(&self) -> Map<u32, V> {
        Map::new(self.array_namespace)
    }
}
use std::fmt::Debug;

pub struct BufferManager<V> {
    state: BufferState,
    store_iface: CircularBuffer<V>,
    precommit_buffer: HashMap<u32, V>,
}

impl<V> BufferManager<V>
where
    V: Serialize + DeserializeOwned + Clone,
{
    /// Static function to initialize buffer in storage.
    /// Intended to be called during contract initialization.
    pub fn init(
        store: &mut dyn Storage,
        store_iface: CircularBuffer<V>,
        capacity: u32,
    ) -> Result<(), StdError> {
        let state_iface = store_iface.state();

        if state_iface.may_load(store)?.is_some() {
            return Err(StdError::generic_err("Buffer already initialized"));
        }

        state_iface.save(store, &BufferState { capacity, head: 0 })?;

        Ok(())
    }

    /// Initialize buffer manager.
    /// In case buffer is not initialized it throws [`BufferError::BufferNotInitialized`] error.
    pub fn new(store: &dyn Storage, store_iface: CircularBuffer<V>) -> StdResult<Self> {
        Ok(Self {
            state: store_iface.state().load(store).map_err(|err| {
                if let StdError::NotFound { .. } = err {
                    StdError::generic_err("Buffer not initialized")
                } else {
                    err.into()
                }
            })?,
            store_iface,
            precommit_buffer: HashMap::new(),
        })
    }

    /// Returns current buffer capacity.
    pub fn capacity(&self) -> u32 {
        self.state.capacity
    }

    /// Returns current buffer head.
    pub fn head(&self) -> u32 {
        self.state.head
    }

    /// Push value to precommit buffer.
    pub fn push(&mut self, value: &V) {
        self.precommit_buffer.insert(self.state.head, value.clone());
        self.state.head = (self.state.head + 1) % self.state.capacity;
    }

    /// Push multiple values to precommit buffer.
    pub fn push_many(&mut self, values: &[V]) {
        for value in values {
            self.push(value);
        }
    }

    /// Push value to precommit buffer and commit it to storage.
    pub fn instant_push(&mut self, store: &mut dyn Storage, value: &V) -> Result<(), StdError> {
        self.push(value);
        self.commit(store)
    }

    /// Commit in storage current state and precommit buffer. Buffer is erased after commit.
    pub fn commit(&mut self, store: &mut dyn Storage) -> Result<(), StdError> {
        let array_key = self.store_iface.array();
        for (&key, value) in &self.precommit_buffer {
            if key >= self.state.capacity {
                return Err(StdError::generic_err("Save value error"));
            }
            array_key.save(store, key, value)?;
        }
        self.precommit_buffer.clear();
        self.store_iface.state().save(store, &self.state)?;

        Ok(())
    }

    /// Read values from storage by indexes. If `stop_if_empty` is true,
    /// reading will stop when first empty value is encountered.
    /// Otherwise, [`BufferError::IndexNotFound`] error will be thrown.
    ///
    /// ## Examples:
    /// ```
    /// # use cosmwasm_std::{testing::MockStorage};
    /// # use astroport_circular_buffer::{BufferManager, CircularBuffer};
    /// # let mut store = MockStorage::new();
    /// # const CIRCULAR_BUFFER: CircularBuffer<u128> = CircularBuffer::new("buffer_state", "buffer");
    /// # BufferManager::init(&mut store, CIRCULAR_BUFFER, 10).unwrap();
    /// # let mut buffer = BufferManager::new(&store, CIRCULAR_BUFFER).unwrap();
    /// # let data = (1..=10u128).collect::<Vec<_>>();
    /// # buffer.push_many(&data);
    /// # buffer.commit(&mut store).unwrap();
    ///
    /// let values = buffer.read(&store, 0u32..=9, false).unwrap();
    /// let values = buffer.read(&store, vec![0u32, 5, 7], false).unwrap();
    /// let values = buffer.read(&store, (0u32..buffer.capacity()).step_by(2), false).unwrap();
    /// ```
    pub fn read(
        &self,
        store: &dyn Storage,
        indexes: impl IntoIterator<Item = impl Into<u32> + Display>,
        stop_if_empty: bool,
    ) -> Result<Vec<V>, StdError> {
        let array_key = self.store_iface.array();
        let mut values = vec![];
        for index in indexes {
            let ind = index.into();
            if ind > self.state.capacity - 1 {
                return Err(StdError::generic_err("Read ahead error"));
            } else {
                let value = array_key.load(store, ind).map_err(|err| {
                    if let StdError::NotFound { .. } = err {
                        StdError::generic_err("Index not found")
                    } else {
                        err.into()
                    }
                });
                match value {
                    Ok(value) => values.push(value),
                    Err(StdError::NotFound { .. }) if stop_if_empty => return Ok(values),
                    Err(err) => return Err(err),
                }
            }
        }

        Ok(values)
    }

    /// Read all available values from storage.
    pub fn read_all(&self, store: &dyn Storage) -> Result<Vec<V>, StdError> {
        self.read(store, 0..self.state.capacity, true)
    }

    /// Read last saved value from storage. Returns None if buffer is empty.
    pub fn read_last(&self, store: &dyn Storage) -> Result<Option<V>, StdError> {
        self.read_single(
            store,
            (self.state.capacity + self.state.head - 1) % self.state.capacity,
        )
    }

    /// Looped read. Returns None if value in buffer does not exist.
    pub fn read_single(
        &self,
        store: &dyn Storage,
        index: impl Into<u32>,
    ) -> Result<Option<V>, StdError> {
        let ind = index.into() % self.state.capacity;
        let res = self.store_iface.array().load(store, ind);
        if let Err(StdError::NotFound { .. }) = res {
            Ok(None)
        } else {
            res.map(Some).map_err(Into::into)
        }
    }

    /// This operation is gas consuming. However, it might be helpful in rare cases.
    pub fn clear_buffer(&self, store: &mut dyn Storage) {
        let array_key = self.store_iface.array();
        (0..self.state.capacity).for_each(|i| array_key.remove(store, i))
    }

    /// Whether index exists in buffer.
    pub fn exists(&self, store: &dyn Storage, index: u32) -> bool {
        self.store_iface
            .array()
            .has(store, index % self.state.capacity)
    }
}

impl<V: Debug> Debug for BufferManager<V> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BufferManager")
            .field("state", &self.state)
            .field("precommit_buffer", &self.precommit_buffer)
            .finish()
    }
}

/// Stores trade size observations. We use it in orderbook integration
/// and derive prices for external contracts/users.
#[cw_serde]
#[derive(Copy, Default)]
pub struct Observation {
    /// Timestamp of the observation
    pub ts: u64,
    /// Observed price at this point
    pub price: Decimal,
    /// Price simple moving average (mean)
    pub price_sma: Decimal,
}
