use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};
use euclid::{
    chain::ChainUid,
    liquidity::{AddLiquidityRequest, RemoveLiquidityRequest},
    pool::{EscrowCreateRequest, PoolCreateRequest},
    swap::SwapRequest,
    token::Token,
};

#[cw_serde]
pub struct State {
    // The Router Contract Address on the Virtual Settlement Layer
    pub router_contract: String,
    // Contract admin
    pub admin: String,
    // Escrow Code ID
    pub escrow_code_id: u64,
    // CW20 Code ID
    pub cw20_code_id: u64,
    // The Unique Chain Identifier
    // THIS IS DIFFERENT THAN THE CHAIN_ID OF THE CHAIN, THIS REPRESENTS A UNIQUE IDENTIFIER FOR THE CHAIN
    // IN THE EUCLID ECOSYSTEM
    pub chain_uid: ChainUid,
}

pub const STATE: Item<State> = Item::new("state");

// Channel that connects factory to hub chain. This is set after factory registration call from router
pub const HUB_CHANNEL: Item<String> = Item::new("hub_channel");

// Map Pair to vlp address
pub const PAIR_TO_VLP: Map<(Token, Token), String> = Map::new("pair_to_vlp");

// Map vlp to LP Allocations
pub const VLP_TO_LP_SHARES: Map<String, Uint128> = Map::new("vlp_to_lp_shares");

// New Factory states
pub const TOKEN_TO_ESCROW: Map<Token, Addr> = Map::new("token_to_escrow");

// New CW20 states
pub const VLP_TO_CW20: Map<String, Addr> = Map::new("vlp_to_cw20");

// Map for pending pool requests for user
pub const PENDING_POOL_REQUESTS: Map<(Addr, String), PoolCreateRequest> =
    Map::new("request_to_pool");

pub const PENDING_ESCROW_REQUESTS: Map<(Addr, String), EscrowCreateRequest> =
    Map::new("request_to_pool");

// Map for pending swaps for user
pub const PENDING_SWAPS: Map<(Addr, String), SwapRequest> = Map::new("pending_swaps");

// Map for PENDING liquidity transactions
pub const PENDING_ADD_LIQUIDITY: Map<(Addr, String), AddLiquidityRequest> =
    Map::new("pending_add_liquidity");
// Map for PENDING liquidity transactions
pub const PENDING_REMOVE_LIQUIDITY: Map<(Addr, String), RemoveLiquidityRequest> =
    Map::new("pending_remove_liquidity");
