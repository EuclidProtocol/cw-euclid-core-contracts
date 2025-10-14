use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Binary, Uint128};
use cw_storage_plus::{Item, Map};
use euclid::{
    chain::{Chain, ChainUid, CrossChainUser},
    msgs::router::TokenDenom,
    token::{PairWithDenomAndAmount, Token},
};
use euclid_ibc::msg::{ChainIbcRemoveLiquidityExecuteMsg, ChainIbcSwapExecuteMsg};

#[cw_serde]
pub struct State {
    // Contract admin
    pub admin: String,
    // Pool Code ID
    pub constant_product_vlp_code_id: u64,
    // Stable Pool Code ID
    pub stable_vlp_code_id: u64,
    pub virtual_balance_address: Option<Addr>,
    pub locked: bool,
}

pub const STATE: Item<State> = Item::new("state");

pub const MOCK_RELAYER_ADDRESSES: Item<Vec<String>> = Item::new("mock_relayer_addresses");

// Convert it to multi index map?
pub const VLPS: Map<(String, String), String> = Map::new("vlps");

// Store all tokens in a map for easy access
pub const TOKEN_VLPS: Map<Token, Vec<String>> = Map::new("token_vlps");

// Store all tokens in a map for easy access
pub const TOKEN_DENOMS: Map<Token, Vec<TokenDenom>> = Map::new("token_denoms");

// Token escrow balance on each chain
pub const ESCROW_BALANCES: Map<(String, ChainUid), Uint128> = Map::new("escrow_balances");

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

pub const PROCESSED_RECEIVE_PACKET_SEQUENCE: Map<(ChainUid, u128), Uint128> =
    Map::new("processed_receive_packet_sequence");

// The key is a tuple of (ChainUid, sequence). Sequence is the count of packets relayed for that chain
pub const PACKET_RELAY: Map<(ChainUid, u128), Binary> = Map::new("packet_relay");

// The value here is the count of packets relayed on the chain
pub const PACKET_RELAY_COUNT_CHAIN: Map<ChainUid, u128> = Map::new("packet_relay_count_chain");

// The value here is the count of packets relayed for the cross chain user
pub const PACKET_RELAY_COUNT_CROSS_CHAIN_USER: Map<CrossChainUser, u128> =
    Map::new("packet_relay_count_cross_chain_user");
