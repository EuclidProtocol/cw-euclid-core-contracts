use cosmwasm_schema::cw_serde;
use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map};
use euclid::{
    liquidity::{LiquidityTxInfo, RemoveLiquidityTxInfo},
    pool::PoolCreateRequest,
    swap::SwapInfo,
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
    // The Unique Chain Identifier
    // THIS IS DIFFERENT THAN THE CHAIN_ID OF THE CHAIN, THIS REPRESENTS A UNIQUE IDENTIFIER FOR THE CHAIN
    // IN THE EUCLID ECOSYSTEM
    pub chain_uid: String,
}

pub const STATE: Item<State> = Item::new("state");

// Channel that connects factory to hub chain. This is set after factory registration call from router
pub const HUB_CHANNEL: Item<String> = Item::new("hub_channel");

// Map VLP address to Pool address
pub const PAIR_TO_VLP: Map<(Token, Token), String> = Map::new("pair_to_vlp");

// New Factory states
pub const TOKEN_TO_ESCROW: Map<Token, Addr> = Map::new("token_to_escrow");

// Map for pending pool requests for user
pub const PENDING_POOL_REQUESTS: Map<(Addr, String), PoolCreateRequest> =
    Map::new("request_to_pool");

// Map for pending swaps for user
pub const PENDING_SWAPS: Map<(Addr, String), SwapInfo> = Map::new("pending_swaps");

// Map for PENDING liquidity transactions
pub const PENDING_ADD_LIQUIDITY: Map<(Addr, String), LiquidityTxInfo> =
    Map::new("pending_add_liquidity");
// Map for PENDING liquidity transactions
pub const PENDING_REMOVE_LIQUIDITY: Map<(Addr, String), RemoveLiquidityTxInfo> =
    Map::new("pending_remove_liquidity");
