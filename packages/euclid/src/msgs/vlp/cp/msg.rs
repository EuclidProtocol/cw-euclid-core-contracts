use crate::{
    admin::{AdminType, EuclidAdmin},
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    fee::{Fee, TotalFees},
    msgs::vlp::base::{
        GetLiquidityQueryResponse, GetSwapQueryResponse, PoolConfig, VlpAddLiquidityMsg,
        VlpRegisterPoolMsg, VlpRemoveLiquidityMsg, VlpSimulateSwapMsg, VlpSwapMsg,
    },
    token::Pair,
};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Uint256};

#[cw_serde]
pub struct InstantiateMsg {
    pub router: Addr,
    pub virtual_balance_contract: Addr,
    pub pair: Pair,
    pub fee: Fee,
    pub execute: Option<ExecuteMsg>,
    pub admin: EuclidAdmin,
}

#[cw_serde]
#[cfg_attr(feature = "cross-vm", derive(cross_vm_macros::CwExecuteFns))]
#[cfg_attr(feature = "cross-vm", cross_vm(trait_name = "CpVlpExecuteFns"))]
pub enum ExecuteMsg {
    UpdateAdmin {
        admin: String,
        admin_type: AdminType,
    },
    UpdateFee {
        lp_fee_bps: Option<u64>,
        euclid_fee_bps: Option<u64>,
        recipient: Option<CrossChainUser>,
    },
    RegisterPool(VlpRegisterPoolMsg),
    AddLiquidity(VlpAddLiquidityMsg),
    RemoveLiquidity(VlpRemoveLiquidityMsg),
    Swap(VlpSwapMsg),
}

#[cw_serde]
#[derive(QueryResponses)]
#[cfg_attr(feature = "cross-vm", derive(cross_vm_macros::CwQueryFns))]
#[cfg_attr(feature = "cross-vm", cross_vm(trait_name = "CpVlpQueryFns"))]
pub enum QueryMsg {
    #[returns(GetStateResponse)]
    State {},

    #[returns(EuclidAdmin)]
    GetAdmin {},

    // Query to simulate a swap for the asset
    #[returns(GetSwapQueryResponse)]
    SimulateSwap(VlpSimulateSwapMsg),
    // Queries the total reserve of the pair in the VLP
    #[returns(GetLiquidityQueryResponse)]
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

    #[returns(crate::build_info::BuildInfoResponse)]
    GetBuildInfo {},
}

// We define a custom struct for each query response
#[cw_serde]
pub struct GetStateResponse {
    pub pair: Pair,
    pub router: Addr,
    pub virtual_balance_contract: Addr,
    pub fee: Fee,
    pub total_fees_collected: TotalFees,
    pub last_updated: u64,
    pub total_lp_tokens: Uint256,
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
    pub lp_fees: Uint256,
    pub euclid_fees: Uint256,
}

#[cw_serde]
pub struct PoolResponse {
    pub lp_shares: Uint256,
    pub reserve_1: Uint256,
    pub reserve_2: Uint256,
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
pub struct MigrateMsg {}
