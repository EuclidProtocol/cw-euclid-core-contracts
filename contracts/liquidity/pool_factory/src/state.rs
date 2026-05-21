use cosmwasm_schema::cw_serde;
use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map};
use euclid::token::PairWithDenomAndAmount;

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
