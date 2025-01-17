use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint256};
use euclid::{
    chain::{AnyContractInfo, ChainUid},
    deposit::DepositTokenRequest,
    fee::DenomFees,
    liquidity::{AddLiquidityRequest, RemoveLiquidityRequest},
    pool::{DenomRegisterDeregisterRequest, PoolCreateRequest},
    swap::SwapRequest,
    token::{PairWithDenomAndAmount, Token, TokenWithDenomAndAmount},
};
use secret_toolkit::{
    serialization::Json,
    storage::{Item, Keymap},
};

#[cw_serde]
pub struct State {
    // The Router Contract Address on the Virtual Settlement Layer
    pub router_contract: String,
    // Router contract code_hash only useful if router is on secret network.
    pub router_contract_code_hash: String,
    // Contract admin
    pub admin: String,
    // Escrow Code ID
    pub escrow_code_id: u64,
    // Escrow Code Hash
    pub escrow_code_hash: String,
    // SNIP20 Code ID
    pub snip20_code_id: u64,
    // SNIP20 Code Hash
    pub snip20_code_hash: String,
    // The Unique Chain Identifier
    // THIS IS DIFFERENT THAN THE CHAIN_ID OF THE CHAIN, THIS REPRESENTS A UNIQUE IDENTIFIER FOR THE CHAIN
    // IN THE EUCLID ECOSYSTEM
    pub chain_uid: ChainUid,
    pub is_native: bool,
    // Total partner fees collected
    pub partner_fees_collected: DenomFees,
    pub proxy_address : String
}

pub const STATE: Item<State> = Item::new(b"state");

// Channel that connects factory to hub chain
pub const HUB_CHANNEL: Item<String> = Item::new(b"hub_channel");

// Keymap Pair to vlp address
pub const PAIR_TO_VLP: Keymap<(Token, Token), String, Json> = Keymap::new(b"pair_to_vlp");

// Keymap vlp to LP Allocations
pub const VLP_TO_LP_SHARES: Keymap<String, Uint256, Json> = Keymap::new(b"vlp_to_lp_shares");

// New Factory states
pub const TOKEN_TO_ESCROW: Keymap<Token, AnyContractInfo, Json> = Keymap::new(b"token_to_escrow");

// New SNIP20 states
pub const VLP_TO_SNIP20: Keymap<String, Addr, Json> = Keymap::new(b"vlp_to_cw20");

// Keymap for pending pool requests for user
pub const PENDING_POOL_REQUESTS: Keymap<(Addr, String), PoolCreateRequest, Json> =
    Keymap::new(b"request_to_pool");

pub const PENDING_DENOM_REGISTER_DEREGISTER_REQUESTS: Keymap<
    (Addr, String),
    DenomRegisterDeregisterRequest,
    Json,
> = Keymap::new(b"request_denom_register_deregister");

// Keymap for pending swaps for user
pub const PENDING_SWAPS: Keymap<(Addr, String), SwapRequest, Json> = Keymap::new(b"pending_swaps");

// Keymap for pending token deposits for user
pub const PENDING_TOKEN_DEPOSIT: Keymap<(Addr, String), DepositTokenRequest, Json> =
    Keymap::new(b"pending_token_deposit");

// Keymap for PENDING liquidity transactions
pub const PENDING_ADD_LIQUIDITY: Keymap<(Addr, String), AddLiquidityRequest, Json> =
    Keymap::new(b"pending_add_liquidity");
// Keymap for PENDING liquidity transactions
pub const PENDING_REMOVE_LIQUIDITY: Keymap<(Addr, String), RemoveLiquidityRequest, Json> =
    Keymap::new(b"pending_remove_liquidity");

pub const PENDING_DEPOSIT_TOKEN: Keymap<Token, TokenWithDenomAndAmount, Json> =
    Keymap::new(b"pending_deposit_token");

pub const FUNDS_INFO: Item<PairWithDenomAndAmount> = Item::new(b"funds_info");
