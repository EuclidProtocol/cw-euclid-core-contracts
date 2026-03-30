use crate::{
    cross_chain_user::CrossChainUser,
    fee::{Fee, TotalFees},
    swap::NextSwapVlp,
    token::{Pair, PairWithAmount, Token},
};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128, Uint64};

pub const NEXT_SWAP_REPLY_ID: u64 = 2;

#[cw_serde]
pub struct State {
    // Token Pair Info
    pub pair: Pair,
    // Router Contract
    pub router: Addr,
    // Virtual Coin Contract
    pub virtual_balance_contract: Addr,
    // Fee per swap for each transaction
    pub fee: Fee,
    // Total lp and euclid fees collected
    pub total_fees_collected: TotalFees,
    // The last timestamp where the balances for each token have been updated
    pub last_updated: u64,
    // total number of LP tokens issued
    pub total_lp_tokens: Uint128,
}

#[cw_serde]
pub struct VlpSwapMsg {
    pub sender: CrossChainUser,
    pub tx_id: String,
    pub asset_in: Token,
    pub amount_in: Uint128,
    pub min_token_out: Uint128,
    pub next_swaps: Vec<NextSwapVlp>,
    pub test_fail: Option<bool>,
}

#[cw_serde]
pub struct VlpAddLiquidityMsg {
    pub sender: CrossChainUser,
    pub tx_id: String,
    pub liquidity: PairWithAmount,
    pub slippage_tolerance_bps: u64,
}

#[cw_serde]
pub struct VlpRemoveLiquidityMsg {
    pub sender: CrossChainUser,
    pub tx_id: String,
    pub lp_allocation: Uint128,
}

#[cw_serde]
pub struct VlpRegisterPoolMsg {
    pub sender: CrossChainUser,
    pub pair: Pair,
    pub tx_id: String,
}

#[cw_serde]
pub struct VlpConcentratedRegisterPoolMsg {
    pub sender: CrossChainUser,
    pub pool_key: PoolKey,
    pub tx_id: String,
}

#[cw_serde]
pub struct VlpSimulateSwapMsg {
    pub asset: Token,
    pub asset_amount: Uint128,
    pub swaps: Vec<NextSwapVlp>,
}

#[cw_serde]
pub struct VlpConcentratedAddLiquidityMsg {
    pub sender: CrossChainUser,
    pub tx_id: String,
    pub pool_key: PoolKey,
    pub liquidity: PairWithAmount,
    pub lower_tick_index: i64,
    pub upper_tick_index: i64,
    pub position_id: Option<Uint128>,
    pub slippage_tolerance_bps: u64,
}

#[cw_serde]
pub struct VlpConcentratedRemoveLiquidityMsg {
    pub sender: CrossChainUser,
    pub tx_id: String,
    pub pool_key: PoolKey,
    pub position_id: Uint128,
    #[serde(alias = "lp_allocation")]
    pub liquidity_delta: Uint128,
}

#[cw_serde]
pub struct VlpConcentratedCollectFeesMsg {
    pub sender: CrossChainUser,
    pub tx_id: String,
    pub pool_key: PoolKey,
    pub position_id: Uint128,
    pub recipient: CrossChainUser,
}

#[cw_serde]
pub struct VlpConcentratedCollectProtocolFeesMsg {
    pub sender: CrossChainUser,
    pub tx_id: String,
    pub pool_key: PoolKey,
    pub recipient: CrossChainUser,
    pub amount_0_requested: Uint128,
    pub amount_1_requested: Uint128,
}

#[cw_serde]
pub struct GetLiquidityQueryResponse {
    pub pair: Pair,
    pub token_1_reserve: Uint128,
    pub token_2_reserve: Uint128,
    pub total_lp_tokens: Uint128,
}

#[cw_serde]
pub struct GetSwapQueryResponse {
    pub amount_out: Uint128,
    pub asset_out: Token,
    pub spread_amount: Uint128,
    pub lp_fee: Uint128,
    pub euclid_fee: Uint128,
}

#[cw_serde]
pub struct PoolCreationResponse {
    pub vlp_contract: String,
    pub tx_id: String,
    pub mint_lp_tokens: Uint128,
    pub sender: CrossChainUser,
}

#[cw_serde]
pub struct ConcentratedPoolCreationResponse {
    pub vlp_contract: String,
    pub tx_id: String,
    pub sender: CrossChainUser,
    pub pool_key: PoolKey,
}

#[cw_serde]
pub struct VlpSwapResponse {
    pub sender: CrossChainUser,
    pub tx_id: String,
    pub asset_out: Token,
    pub amount_out: Uint128,
}

#[cw_serde]
pub struct VlpAddLiquidityResponse {
    pub liquidity_added: PairWithAmount,
    pub mint_lp_tokens: Uint128,
    pub tx_id: String,
    pub sender: CrossChainUser,
    pub vlp_address: String,
}

#[cw_serde]
pub struct VlpRemoveLiquidityResponse {
    pub liquidity_released: PairWithAmount,
    pub burn_lp_tokens: Uint128,
    pub tx_id: String,
    pub sender: CrossChainUser,
    pub vlp_address: String,
}

#[cw_serde]
pub struct VlpConcentratedAddLiquidityResponse {
    pub liquidity_added: PairWithAmount,
    pub liquidity_delta: Uint128,
    pub position_id: Uint128,
    pub tx_id: String,
    pub sender: CrossChainUser,
    pub vlp_address: String,
    pub pool_key: PoolKey,
}

#[cw_serde]
pub struct VlpConcentratedRemoveLiquidityResponse {
    pub liquidity_released: PairWithAmount,
    pub liquidity_delta: Uint128,
    pub liquidity_after: Uint128,
    pub position_id: Uint128,
    pub tx_id: String,
    pub sender: CrossChainUser,
    pub vlp_address: String,
    pub pool_key: PoolKey,
}

#[cw_serde]
pub struct VlpConcentratedCollectFeesResponse {
    pub pool_key: PoolKey,
    pub position_id: Uint128,
    pub amount_0: Uint128,
    pub amount_1: Uint128,
    pub tx_id: String,
    pub sender: CrossChainUser,
    pub recipient: CrossChainUser,
    pub vlp_address: String,
}

#[cw_serde]
pub struct VlpConcentratedCollectProtocolFeesResponse {
    pub pool_key: PoolKey,
    pub amount_0: Uint128,
    pub amount_1: Uint128,
    pub tx_id: String,
    pub sender: CrossChainUser,
    pub recipient: CrossChainUser,
    pub vlp_address: String,
}

#[cw_serde]
pub struct RegisterDenomResponse {}

#[cw_serde]
pub struct DeregisterDenomResponse {}

#[cw_serde]
pub enum PoolType {
    ConstantProduct {},
    Stable {},
    Concentrated {
        fee_tier_bps: u64,
        tick_spacing: u64,
    },
}

#[cw_serde]
pub struct PoolKey {
    pub pair: Pair,
    pub pool_type: PoolType,
}

impl PoolKey {
    /// Encode this pool key as a null-byte-delimited string suitable for use as a storage map key.
    pub fn to_map_key(&self) -> String {
        let (fee_tier_bps, tick_spacing) = match self.pool_type {
            PoolType::Concentrated {
                fee_tier_bps,
                tick_spacing,
            } => (fee_tier_bps, tick_spacing),
            _ => (0, 0),
        };
        format!(
            "{}\0{}\0{}\0{}",
            self.pair.token_1, self.pair.token_2, fee_tier_bps, tick_spacing
        )
    }

    /// Decode a null-byte-delimited map key into its component parts.
    pub fn parse_map_key(key: &str) -> Option<(String, String, u64, u64)> {
        let mut parts = key.split('\0');
        let token_1 = parts.next()?.to_string();
        let token_2 = parts.next()?.to_string();
        let fee_tier_bps = parts.next()?.parse::<u64>().ok()?;
        let tick_spacing = parts.next()?.parse::<u64>().ok()?;
        if parts.next().is_some() {
            return None;
        }
        Some((token_1, token_2, fee_tier_bps, tick_spacing))
    }
}

#[cw_serde]
pub enum PoolConfig {
    Stable {
        amp_factor: Option<Uint64>,
    },
    ConstantProduct {},
    Concentrated {
        fee_tier_bps: u64,
        tick_spacing: u64,
    },
}

#[cw_serde]
pub enum QueryMsg {
    Liquidity {},
    SimulateSwap(VlpSimulateSwapMsg),
}

#[cw_serde]
pub enum ExecuteMsg {
    RegisterPool(VlpRegisterPoolMsg),
    RegisterConcentratedPool(VlpConcentratedRegisterPoolMsg),
    AddLiquidity(VlpAddLiquidityMsg),
    AddConcentratedLiquidity(VlpConcentratedAddLiquidityMsg),
    RemoveLiquidity(VlpRemoveLiquidityMsg),
    RemoveConcentratedLiquidity(VlpConcentratedRemoveLiquidityMsg),
    CollectConcentratedFees(VlpConcentratedCollectFeesMsg),
    CollectConcentratedProtocolFees(VlpConcentratedCollectProtocolFeesMsg),
    Swap(VlpSwapMsg),
}
