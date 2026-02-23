use crate::{
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    fee::{Fee, TotalFees},
    msgs::vlp::base::{
        GetLiquidityQueryResponse, GetSwapQueryResponse, PoolConfig, PoolKey,
        VlpConcentratedAddLiquidityMsg, VlpConcentratedRegisterPoolMsg,
        VlpConcentratedRemoveLiquidityMsg, VlpSimulateSwapMsg, VlpSwapMsg,
    },
    token::Pair,
};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Uint128};

#[cw_serde]
pub struct InstantiateMsg {
    pub router: Addr,
    pub virtual_balance_contract: Addr,
    pub pair: Pair,
    pub fee: Fee,
    pub execute: Option<ExecuteMsg>,
    pub admin: Addr,
    pub fee_tier_bps: u64,
    pub tick_spacing: u64,
}

#[cw_serde]
pub enum ExecuteMsg {
    UpdateState {
        admin: Option<Addr>,
    },
    UpdateFee {
        lp_fee_bps: Option<u64>,
        euclid_fee_bps: Option<u64>,
        recipient: Option<CrossChainUser>,
    },
    RegisterPool(VlpConcentratedRegisterPoolMsg),
    AddLiquidity(VlpConcentratedAddLiquidityMsg),
    RemoveLiquidity(VlpConcentratedRemoveLiquidityMsg),
    Swap(VlpSwapMsg),
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(GetStateResponse)]
    State {},
    #[returns(GetSwapQueryResponse)]
    SimulateSwap(VlpSimulateSwapMsg),
    #[returns(GetLiquidityQueryResponse)]
    Liquidity {},
    #[returns(FeeResponse)]
    Fee {},
    #[returns(TotalFeesResponse)]
    TotalFeesCollected {},
    #[returns(TotalFeesPerDenomResponse)]
    TotalFeesPerDenom { denom: String },
    #[returns(ConcentratedPoolResponse)]
    Pool {
        chain_uid: ChainUid,
        pool_key: PoolKey,
    },
    #[returns(AllConcentratedPoolsResponse)]
    GetAllPools {},
}

#[cw_serde]
pub struct GetStateResponse {
    pub pair: Pair,
    pub router: Addr,
    pub virtual_balance_contract: Addr,
    pub fee: Fee,
    pub total_fees_collected: TotalFees,
    pub last_updated: u64,
    pub total_lp_tokens: Uint128,
    pub admin: Addr,
    pub pool_config: PoolConfig,
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
pub struct ConcentratedPoolResponse {
    pub pool_key: PoolKey,
    pub lp_shares: Uint128,
    pub reserve_1: Uint128,
    pub reserve_2: Uint128,
}

#[cw_serde]
pub struct ConcentratedPoolInfo {
    pub chain_uid: ChainUid,
    pub pool: ConcentratedPoolResponse,
}

#[cw_serde]
pub struct AllConcentratedPoolsResponse {
    pub pools: Vec<ConcentratedPoolInfo>,
}

#[cw_serde]
pub struct MigrateMsg {}
