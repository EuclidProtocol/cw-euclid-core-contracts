use crate::{
    chain::{ChainUid, CrossChainUser},
    fee::{Fee, TotalFees},
    swap::NextSwapVlp,
    token::{Pair, PairWithAmount, Token, TokenType},
};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;

#[cw_serde]
pub struct InstantiateMsg {
    pub router: String,
    pub virtual_balance: String,
    pub pair: Pair,
    pub fee: Fee,
    pub execute: Option<ExecuteMsg>,
    pub admin: String,
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
    },
    MigrateVLP {
        migrate_msg: VlpMigrateMsg,
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
    #[returns(PoolResponse)]
    Pool { chain_uid: ChainUid },
    // Query to get all pools
    #[returns(AllPoolsResponse)]
    GetAllPools {},
    #[returns(AllChainLpTokensResponse)]
    GetAllChainLpTokens {},
    #[returns(VlpMigrateMsg)]
    GetMigrateData {},
}

#[cw_serde]
pub struct VlpMigrateMsg {
    pub state: GetStateResponse,
    pub chain_lp_tokens: AllChainLpTokensResponse,
    pub balances: AllBalancesResponse,
}

#[cw_serde]
pub struct AllChainLpTokensResponse {
    pub chain_lp_tokens: Vec<(ChainUid, Uint128)>,
}

// We define a custom struct for each query response
#[cw_serde]
pub struct GetSwapResponse {
    pub amount_out: Uint128,
    pub asset_out: Token,
    pub spread_amount: Uint128,
}

#[cw_serde]
pub struct State {
    // Token Pair Info
    pub pair: Pair,
    // Router Contract
    pub router: String,
    // Virtual Coin Contract
    pub virtual_balance: String,
    // Fee per swap for each transaction
    pub fee: Fee,
    // Total lp and euclid fees collected
    pub total_fees_collected: TotalFees,
    // The last timestamp where the balances for each token have been updated
    pub last_updated: u64,
    // total number of LP tokens issued
    pub total_lp_tokens: Uint128,
    pub admin: String,
}

#[cw_serde]
pub struct GetStateResponse {
    pub state: State,
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
pub struct PoolResponse {
    pub lp_shares: Uint128,
    pub reserve_1: Uint128,
    pub reserve_2: Uint128,
}

#[cw_serde]
pub struct PoolInfo {
    pub chain_uid: ChainUid,
    pub pool: PoolResponse,
}
#[cw_serde]
pub struct AllPoolsResponse {
    pub pools: Vec<PoolInfo>,
}

#[cw_serde]
pub struct AllBalancesResponse {
    pub balances: Vec<(Token, Uint128)>,
}

#[cw_serde]
pub struct MigrateMsg {}

#[cw_serde]
pub struct VlpRemoveLiquidityResponse {
    pub liquidity_released: PairWithAmount,
    pub preferred_denom: Option<TokenType>,
    pub burn_lp_tokens: Uint128,
    pub tx_id: String,
    pub sender: CrossChainUser,
    pub vlp_address: String,
}

#[cw_serde]
pub struct VlpSwapResponse {
    pub sender: CrossChainUser,
    pub tx_id: String,
    pub asset_out: Token,
    pub amount_out: Uint128,
}
