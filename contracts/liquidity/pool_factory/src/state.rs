use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Int256};
use cw_storage_plus::{Item, Map};
use euclid::{
    liquidity::{AddLiquidityRequest, RemoveLiquidityRequest},
    msgs::vlp::base::PoolKey,
    token::PairWithDenomAndAmount,
};

/// Address of the main factory on this chain. All `On*` execute entries
/// require `info.sender == MAIN_FACTORY_ADDRESS`.
pub const MAIN_FACTORY_ADDRESS: Item<Addr> = Item::new("main_factory_address");

/// One-shot flag set by `MigrateAcceptPoolState`. Once true, any further
/// invocation of the migration entry is rejected.
pub const MIGRATION_ACCEPTED: Item<bool> = Item::new("migration_accepted");

/// Mirror of main factory's pool registry. Lookups use `Pair::get_tupple()`
/// as the key, keeping parity with the original `PAIR_TO_VLP` map.
pub const PAIR_TO_VLP: Map<(String, String), String> = Map::new("pair_to_vlp");

/// Maps VLP address → LP cw20 token address. Same key shape as main factory's
/// pre-refactor state so the migration can copy entries verbatim.
pub const VLP_TO_LP_TOKEN: Map<String, Addr> = Map::new("vlp_to_lp_token");

#[cw_serde]
pub struct PoolCreateRequest {
    pub tx_id: String,
    pub sender: Addr,
    pub pair_info: PairWithDenomAndAmount,
    pub lp_token_instantiate_msg: cw20_base::msg::InstantiateMsg,
}

/// Pool factory's view of in-flight CP/Stable pool creations, keyed by
/// (sender, tx_id) to match the main-factory pending-queue shape.
pub const PENDING_POOL_REQUESTS: Map<(Addr, String), PoolCreateRequest> =
    Map::new("request_to_pool");

/// Pool factory's view of in-flight CP/Stable add-liquidity requests, keyed by
/// (sender, tx_id) — matches the main-factory pending-queue shape so migration
/// can copy entries verbatim.
pub const PENDING_ADD_LIQUIDITY: Map<(Addr, String), AddLiquidityRequest> =
    Map::new("pending_add_liquidity");

/// Pool factory's view of in-flight CP/Stable remove-liquidity requests,
/// keyed by (sender, tx_id) — matches the main-factory pending-queue shape so
/// migration can copy entries verbatim. The held LP cw20 tokens stay on main
/// factory (they arrived via the `cw20::Send` hook before delegation); the
/// ack path issues `ProxyBurnLpToken` on success or `ProxyTransferLpToken` on
/// failure.
pub const PENDING_REMOVE_LIQUIDITY: Map<(Addr, String), RemoveLiquidityRequest> =
    Map::new("pending_remove_liquidity");

/// Mirror of main factory's per-VLP LP share accounting. Decremented on a
/// successful remove-liquidity ack so the on-chain accounting tracks parity
/// with what the hub VLP says.
pub const VLP_TO_LP_SHARES: Map<String, Int256> = Map::new("vlp_to_lp_shares");

/// Mirror of main factory's per-`PoolKey` concentrated pool registry. The map
/// key matches `PoolKey::to_map_key()` so the migration can copy entries
/// verbatim from main factory's `POOL_KEY_TO_VLP` map.
pub const CONCENTRATED_VLPS: Map<String, String> = Map::new("concentrated_vlps");

/// Mirror of main factory's singleton position-token NFT contract address.
/// Populated by `MigrateAcceptPoolState` (or as a Slice 4 carry-over the legacy
/// instantiate path keeps main factory authoritative). Used by Slice 5+ when
/// pool factory drives the position mint/update via `ProxyMintPosition` /
/// `ProxyUpdatePosition`.
pub const POSITION_TOKEN_CONTRACT: Item<Addr> = Item::new("position_token_contract");

#[cw_serde]
pub struct ConcentratedPoolCreateRequest {
    pub tx_id: String,
    pub sender: Addr,
    pub pair_info: PairWithDenomAndAmount,
    pub pool_key: PoolKey,
}

/// Pool factory's view of in-flight CLP pool creations, keyed by
/// (sender, tx_id) to match the main-factory pending-queue shape so migration
/// can copy entries verbatim.
pub const PENDING_CONCENTRATED_POOL_REQUESTS: Map<(Addr, String), ConcentratedPoolCreateRequest> =
    Map::new("pending_concentrated_pool_requests");
