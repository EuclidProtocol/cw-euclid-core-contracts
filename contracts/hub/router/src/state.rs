use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Binary, Uint128};
use cw_storage_plus::{Item, Map};
use euclid::{
    chain::{Chain, ChainUid},
    msgs::router::TokenDenom,
    token::{PairWithDenomAndAmount, Token},
};
use euclid_ibc::msg::{ChainIbcRemoveLiquidityExecuteMsg, ChainIbcSwapExecuteMsg};

#[cw_serde]
pub struct State {
    // Contract admin
    pub admin: String,
    // Pool Code ID
    pub vlp_code_id: u64,
    pub virtual_balance_address: Option<Addr>,
    pub locked: bool,
}

pub const STATE: Item<State> = Item::new("state");

// Convert it to multi index map?
pub const VLPS: Map<(Token, Token), String> = Map::new("vlps");

// Store all tokens in a map for easy access
pub const TOKEN_VLPS: Map<Token, Vec<String>> = Map::new("token_vlps");

// Store all tokens in a map for easy access
pub const TOKEN_DENOMS: Map<Token, Vec<TokenDenom>> = Map::new("token_denoms");

// Token escrow balance on each chain
pub const ESCROW_BALANCES: Map<(Token, ChainUid), Uint128> = Map::new("escrow_balances");

pub const CHAIN_UID_TO_CHAIN: Map<ChainUid, Chain> = Map::new("chain_uid_to_chain");
pub const CHANNEL_TO_CHAIN_UID: Map<String, ChainUid> = Map::new("channel_to_chain_uid");
pub const DEREGISTERED_CHAINS: Item<Vec<ChainUid>> = Item::new("deregistered_chains");

// Map for (ChainUID ,Sender, TX ID)
pub const SWAP_ID_TO_MSG: Map<(ChainUid, String, String), ChainIbcSwapExecuteMsg> =
    Map::new("swap_id_to_msg");

// Map for (ChainUID ,Sender, TX ID)
pub const PENDING_REMOVE_LIQUIDITY: Map<
    (ChainUid, String, String),
    ChainIbcRemoveLiquidityExecuteMsg,
> = Map::new("pending_remove_liquidity");

pub const FUNDS_INFO: Item<(PairWithDenomAndAmount, u64)> = Item::new("funds_info");

//EVM Relay sequence map
pub const EVM_PACKET_RELAY_MAP: Map<(ChainUid, u128), Binary> = Map::new("evm_packet_relay_map");

//EVM Relay sequence count
pub const EVM_PACKET_RELAY_SEQUENCE_COUNT: Map<ChainUid, u128> =
    Map::new("evm_packet_relay_sequence_count");

//SOLANA Relay sequence map
pub const SOLANA_PACKET_RELAY_MAP: Map<(ChainUid, u128), Binary> =
    Map::new("solana_packet_relay_map");

//SOLANA Relay sequence count
pub const SOLANA_PACKET_RELAY_SEQUENCE_COUNT: Map<ChainUid, u128> =
    Map::new("solana_packet_relay_sequence_count");
