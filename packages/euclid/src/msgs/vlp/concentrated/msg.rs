use crate::{
    admin::AdminType,
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    fee::{Fee, TotalFees},
    msgs::vlp::base::{
        GetLiquidityQueryResponse, GetSwapQueryResponse, PoolConfig, PoolKey,
        VlpConcentratedAddLiquidityMsg, VlpConcentratedCollectFeesMsg,
        VlpConcentratedCollectProtocolFeesMsg, VlpConcentratedRegisterPoolMsg,
        VlpConcentratedRemoveLiquidityMsg, VlpSimulateSwapMsg, VlpSwapMsg,
    },
    token::Pair,
};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Uint128, Uint256};

#[cw_serde]
pub struct InstantiateMsg {
    pub virtual_balance_contract: Addr,
    pub pair: Pair,
    pub fee: Fee,
    pub execute: Option<ExecuteMsg>,
    pub admin: Addr,
    pub fee_tier_bps: u64,
    pub tick_spacing: u64,
    /// Initial tick for pool price. `None` defaults to tick 0 (1:1 price).
    pub initial_tick: Option<i64>,
}

#[cw_serde]
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
    RegisterPool(VlpConcentratedRegisterPoolMsg),
    AddLiquidity(VlpConcentratedAddLiquidityMsg),
    RemoveLiquidity(VlpConcentratedRemoveLiquidityMsg),
    CollectFees(VlpConcentratedCollectFeesMsg),
    CollectProtocolFees(VlpConcentratedCollectProtocolFeesMsg),
    IncreaseObservationCardinalityNext {
        observation_cardinality_next: u16,
    },
    Swap(VlpSwapMsg),
}

#[cw_serde]
pub enum LegacyLiquidityMode {
    AlreadyV3Liquidity,
    LegacyShareProRata,
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
    #[returns(Slot0Response)]
    Slot0 {},
    #[returns(PositionResponse)]
    Position { position_id: Uint128 },
    #[returns(TickResponse)]
    Tick { index: i64 },
    #[returns(TicksResponse)]
    Ticks {
        start_after: Option<i64>,
        limit: Option<u32>,
    },
    #[returns(ObserveResponse)]
    Observe { seconds_agos: Vec<u64> },
    #[returns(ProtocolFeesResponse)]
    ProtocolFees {},
    #[returns(MigrationStatusResponse)]
    MigrationStatus {},
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
pub struct Slot0Response {
    pub sqrt_price_x96: Uint256,
    pub tick: i64,
    pub observation_index: u64,
    pub observation_cardinality: u16,
    pub observation_cardinality_next: u16,
    pub liquidity: Uint128,
    pub fee_growth_global_0_x128: Uint256,
    pub fee_growth_global_1_x128: Uint256,
}

#[cw_serde]
pub struct PositionResponse {
    pub position_id: Uint128,
    pub chain_uid: ChainUid,
    pub lower_tick_index: i64,
    pub upper_tick_index: i64,
    pub liquidity: Uint128,
    pub fee_growth_inside_0_last_x128: Uint256,
    pub fee_growth_inside_1_last_x128: Uint256,
    pub tokens_owed_0: Uint128,
    pub tokens_owed_1: Uint128,
}

#[cw_serde]
pub struct TickResponse {
    pub index: i64,
    pub initialized: bool,
    pub liquidity_gross: Uint128,
    pub liquidity_net: i128,
    pub fee_growth_outside_0_x128: Uint256,
    pub fee_growth_outside_1_x128: Uint256,
}

#[cw_serde]
pub struct TicksResponse {
    pub ticks: Vec<TickResponse>,
}

#[cw_serde]
pub struct ObserveResponse {
    pub tick_cumulatives: Vec<i128>,
    pub seconds_per_liquidity_cumulative_x128s: Vec<Uint256>,
}

#[cw_serde]
pub struct ProtocolFeesResponse {
    pub amount_0: Uint128,
    pub amount_1: Uint128,
}

#[cw_serde]
pub struct MigrationStatusResponse {
    pub revision: u16,
    pub source_version: String,
    pub mode: LegacyLiquidityMode,
    pub migrated_at: u64,
    pub positions_migrated: u64,
    pub active_liquidity: Uint128,
    pub total_liquidity: Uint128,
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
pub struct MigrateMsg {
    pub legacy_liquidity_mode: LegacyLiquidityMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_prev_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub force_rebuild: Option<bool>,
}
