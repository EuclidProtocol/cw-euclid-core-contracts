use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, DepsMut, Uint128, Uint256};
use cw_storage_plus::{Item, Map};
use euclid::{
    admin::EuclidAdmin,
    chain::{Chain, ChainUid},
    error::ContractError,
    msgs::{router::TokenDenom, vlp::base::PoolKey},
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

#[deprecated(note = "TOKEN_DENOMS has been moved to virtual_balance contract")]
// Store all tokens in a map for easy access
pub const TOKEN_DENOMS: Map<Token, Vec<TokenDenom>> = Map::new("token_denoms");

#[deprecated(note = "ESCROW_BALANCES has been moved to virtual_balance contract")]
// Token escrow balance on each chain. Mapping of (token, chain_uid) to balance
pub const ESCROW_BALANCES: Map<(String, ChainUid), Uint256> = Map::new("escrow_balances");

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
    /// Initial tick for pool price. `None` means tick 0 (1:1).
    pub initial_tick: Option<i64>,
}
/// Singleton holding funds info for the current concentrated pool creation.
/// Safe as a singleton because CosmWasm SubMsg replies are synchronous — the
/// reply handler runs before control returns to the caller, so no concurrent
/// pool creation can overwrite this value between save and load.
pub const CONCENTRATED_FUNDS_INFO: Item<ConcentratedFundsInfo> =
    Item::new("concentrated_funds_info");

#[cw_serde]
pub struct PendingReleaseVoucher {
    pub total_amount: Uint256,
    pub release_fee_amount: Uint256,
    pub unsafe_refund_voucher: bool,
}
// Tx Id to Release Voucher Request
pub const PENDING_RELEASE_VOUCHER: Map<String, PendingReleaseVoucher> =
    Map::new("pending_release_voucher");

/// Singleton holding funds info for the current classic pool creation.
/// Same synchronous-SubMsg safety rationale as CONCENTRATED_FUNDS_INFO.
pub const FUNDS_INFO: Item<(PairWithDenomAndAmount, u64)> = Item::new("funds_info");

/// The key is TokenID_ChainUID
pub const RELEASE_FEES: Map<(Token, ChainUid), Uint256> = Map::new("release_fees");
pub const DEFAULT_RELEASE_FEE: Item<Uint256> = Item::new("default_release_fee");

// The key is ChainUid and the value is the timeout in seconds for chain send packets
pub const CHAIN_TIMEOUT_SECONDS: Map<ChainUid, u64> = Map::new("chains_timeout_seconds");

// Router acts as centralized entity for CLP position id handling
pub const CLP_POSITION_ID_NONCE: Item<u128> = Item::new("clp_position_id_nonce");

// The key is Position ID and the value is the VLP address
pub const CLP_POSITION_ID_VLP_MAP: Map<u128, Addr> = Map::new("clp_position_id_vlp_map");

/// Get a new CLP position id which is not already used (avoiding collisions)
/// Mutates the CLP_POSITION_ID_NONCE storage item to the last checked position id so next time we start from the next position id
/// Returns the position id if found, otherwise returns an error
pub fn get_clp_position_id(deps: &mut DepsMut) -> Result<u128, ContractError> {
    let iters = 1000;
    let mut position_id = CLP_POSITION_ID_NONCE.load(deps.storage).unwrap_or(0);
    for _ in 0..iters {
        position_id = position_id.wrapping_add(1);
        if !CLP_POSITION_ID_VLP_MAP.has(deps.storage, position_id) {
            CLP_POSITION_ID_NONCE.save(deps.storage, &position_id)?;
            return Ok(position_id);
        }
    }
    // Update the nonce to the last checked position id so next time we start from the next position id (we only restrict 1000 iterations to avoid infinite loop)
    CLP_POSITION_ID_NONCE.save(deps.storage, &position_id)?;
    Err(ContractError::Generic {
        err: format!("No position id available after {} attempts", iters),
    })
}
