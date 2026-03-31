use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};
use euclid::{
    admin::EuclidAdmin,
    chain::{Chain, ChainUid},
    msgs::router::TokenDenom,
    msgs::vlp::base::{PoolKey, PoolType},
    token::{PairWithDenomAndAmount, Token},
};
use euclid_ibc::router_ibc::{
    RouterCrossChainConcentratedCollectFeesExecuteMsg,
    RouterCrossChainConcentratedCollectProtocolFeesExecuteMsg,
    RouterCrossChainConcentratedRemoveLiquidityExecuteMsg,
    RouterCrossChainRemoveLiquidityExecuteMsg, RouterCrossChainSwapExecuteMsg,
};

#[cw_serde]
pub struct State {
    // Pools
    pub constant_product_vlp_code_id: u64,
    pub stable_vlp_code_id: u64,
    pub concentrated_vlp_code_id: u64,

    pub locked: bool,
}

pub const STATE: Item<State> = Item::new("state");
pub const ADMIN: Item<EuclidAdmin> = Item::new("admin");

pub const META_TRANSACTION_CONTRACT: Item<Addr> = Item::new("meta_transaction_contract");
pub const VIRTUAL_BALANCE_CONTRACT: Item<Addr> = Item::new("virtual_balance_contract");
pub const RELAYER_CONTRACT: Item<Addr> = Item::new("relayer_contract");

#[cw_serde]
pub struct FeeState {
    pub release_fee_recipient: Addr,
    pub default_fee_recipient: Addr,
}
pub const FEE_STATE: Item<FeeState> = Item::new("fee_state");

// Convert it to multi index map?
pub const VLPS: Map<(String, String), Addr> = Map::new("vlps");
pub const CONCENTRATED_VLPS: Map<String, Addr> = Map::new("concentrated_vlps");

// Store all vlps related to a token
pub const TOKEN_VLPS: Map<Token, Vec<Addr>> = Map::new("token_vlps");

// Store all tokens in a map for easy access
pub const TOKEN_DENOMS: Map<Token, Vec<TokenDenom>> = Map::new("token_denoms");

// Token escrow balance on each chain. Mapping of (token, chain_uid) to balance
pub const ESCROW_BALANCES: Map<(String, ChainUid), Uint128> = Map::new("escrow_balances");

// Store info of chain against chain uid
pub const CHAIN_UID_TO_CHAIN: Map<ChainUid, Chain> = Map::new("chain_uid_to_chain");
pub const LOCKED_CHAINS: Item<Vec<ChainUid>> = Item::new("locked_chains");

// Tx Id to Swap Request
pub const PENDING_SWAPS: Map<String, RouterCrossChainSwapExecuteMsg> = Map::new("pending_swaps");

// Tx Id to Remove Liquidity Request
pub const PENDING_REMOVE_LIQUIDITY: Map<String, RouterCrossChainRemoveLiquidityExecuteMsg> =
    Map::new("pending_remove_liquidity");
pub const PENDING_CONCENTRATED_REMOVE_LIQUIDITY: Map<
    String,
    RouterCrossChainConcentratedRemoveLiquidityExecuteMsg,
> = Map::new("pending_concentrated_remove_liquidity");
pub const PENDING_CONCENTRATED_COLLECT_FEES: Map<
    String,
    RouterCrossChainConcentratedCollectFeesExecuteMsg,
> = Map::new("pending_concentrated_collect_fees");
pub const PENDING_CONCENTRATED_COLLECT_PROTOCOL_FEES: Map<
    String,
    RouterCrossChainConcentratedCollectProtocolFeesExecuteMsg,
> = Map::new("pending_concentrated_collect_protocol_fees");

#[cw_serde]
pub struct ConcentratedFundsInfo {
    pub pair_with_denom: PairWithDenomAndAmount,
    pub slippage_tolerance_bps: u64,
    pub pool_key: PoolKey,
    pub lower_tick_index: i64,
    pub upper_tick_index: i64,
    pub position_id: Option<Uint128>,
}
/// Singleton holding funds info for the current concentrated pool creation.
/// Safe as a singleton because CosmWasm SubMsg replies are synchronous — the
/// reply handler runs before control returns to the caller, so no concurrent
/// pool creation can overwrite this value between save and load.
pub const CONCENTRATED_FUNDS_INFO: Item<ConcentratedFundsInfo> =
    Item::new("concentrated_funds_info");

#[cw_serde]
pub struct PendingReleaseVoucher {
    pub total_amount: Uint128,
    pub release_fee_amount: Uint128,
    pub unsafe_refund_voucher: bool,
}
// Tx Id to Release Voucher Request
pub const PENDING_RELEASE_VOUCHER: Map<String, PendingReleaseVoucher> =
    Map::new("pending_release_voucher");

/// Singleton holding funds info for the current classic pool creation.
/// Same synchronous-SubMsg safety rationale as CONCENTRATED_FUNDS_INFO.
pub const FUNDS_INFO: Item<(PairWithDenomAndAmount, u64)> = Item::new("funds_info");

/// The key is TokenID_ChainUID
pub const RELEASE_FEES: Map<(Token, ChainUid), Uint128> = Map::new("release_fees");
pub const DEFAULT_RELEASE_FEE: Item<Uint128> = Item::new("default_release_fee");

pub fn pool_key_to_map_key(pool_key: &PoolKey) -> String {
    let (fee_tier_bps, tick_spacing) = match pool_key.pool_type {
        PoolType::Concentrated {
            fee_tier_bps,
            tick_spacing,
        } => (fee_tier_bps, tick_spacing),
        _ => (0, 0),
    };
    format!(
        "{}\0{}\0{}\0{}",
        pool_key.pair.token_1, pool_key.pair.token_2, fee_tier_bps, tick_spacing
    )
}

pub fn map_key_to_pool_parts(key: &str) -> Option<(String, String, u64, u64)> {
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

// The key is ChainUid and the value is the timeout in seconds for chain send packets
pub const CHAIN_TIMEOUT_SECONDS: Map<ChainUid, u64> = Map::new("chains_timeout_seconds");
