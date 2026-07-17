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
use cosmwasm_std::{Addr, Decimal256, Uint256, Uint64};
use cw_asset::AssetInfo;
// The amplification factor for the stableswap invariant, default is 1000
pub const DEFAULT_AMP_FACTOR: Uint64 = Uint64::new(1000);
#[cw_serde]
pub struct InstantiateMsg {
    pub router: Addr,
    pub virtual_balance_contract: Addr,
    pub pair: Pair,
    pub fee: Fee,
    pub execute: Option<ExecuteMsg>,
    pub admin: EuclidAdmin,
    pub amp_factor: Option<Uint64>,
}

#[cw_serde]
#[cfg_attr(feature = "cross-vm", derive(cross_vm_macros::CwExecuteFns))]
#[cfg_attr(feature = "cross-vm", cross_vm(trait_name = "StableVlpExecuteFns"))]
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
    UpdateAmpFactor {
        amp_factor: Uint64,
    },

    RegisterPool(VlpRegisterPoolMsg),
    Swap(VlpSwapMsg),
    AddLiquidity(VlpAddLiquidityMsg),
    RemoveLiquidity(VlpRemoveLiquidityMsg),
}

#[cw_serde]
#[derive(QueryResponses)]
#[cfg_attr(feature = "cross-vm", derive(cross_vm_macros::CwQueryFns))]
#[cfg_attr(feature = "cross-vm", cross_vm(trait_name = "StableVlpQueryFns"))]
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
    #[returns(StablePoolResponse)]
    Pool { chain_uid: ChainUid },
    // Query to get all pools
    #[returns(AllStablePoolsResponse)]
    GetAllPools {},

    #[returns(crate::build_info::BuildInfoResponse)]
    GetBuildInfo {},
}

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
pub struct StablePoolResponse {
    pub lp_shares: Uint256,
    pub reserve_1: Uint256,
    pub reserve_2: Uint256,
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
