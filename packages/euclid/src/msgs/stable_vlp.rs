use crate::{
    chain::{ChainUid, CrossChainUser},
    fee::{Fee, TotalFees},
    pool::{GetSwapResponse, PoolConfig},
    swap::NextSwapVlp,
    token::{Pair, PairWithAmount, Token},
};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Decimal256, Uint128, Uint64};
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
        paused: Option<bool>,
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
