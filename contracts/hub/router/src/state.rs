use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};
use euclid::{chain::ChainUid, escrow::EscrowReleaseRequest, msgs::router::Chain, token::Token};
use euclid_ibc::msg::{ChainIbcRemoveLiquidityExecuteMsg, ChainIbcSwapExecuteMsg};

#[cw_serde]
pub struct State {
    // Contract admin
    pub admin: String,
    // Pool Code ID
    pub vlp_code_id: u64,
    pub vcoin_address: Option<Addr>,
}

pub const STATE: Item<State> = Item::new("state");

// Convert it to multi index map?
pub const VLPS: Map<(Token, Token), String> = Map::new("vlps");

// Token escrow balance on each chain
pub const ESCROW_BALANCES: Map<(Token, ChainUid), Uint128> = Map::new("escrow_balances");

pub const CHAIN_UID_TO_CHAIN: Map<ChainUid, Chain> = Map::new("chain_uid_to_chain");
pub const CHANNEL_TO_CHAIN_UID: Map<String, ChainUid> = Map::new("channel_to_chain_uid");

// Map for (ChainUID ,Sender, TX ID)
pub const SWAP_ID_TO_MSG: Map<(ChainUid, String, String), ChainIbcSwapExecuteMsg> =
    Map::new("swap_id_to_msg");

// Map for (ChainUID ,Sender, TX ID)
pub const PENDING_REMOVE_LIQUIDITY: Map<
    (ChainUid, String, String),
    ChainIbcRemoveLiquidityExecuteMsg,
> = Map::new("pending_remove_liquidity");

// Map for (ChainUID ,Sender, TX ID, Token)
pub const PENDING_ESCROW_RELEASE: Map<(ChainUid, String, String, Token), EscrowReleaseRequest> =
    Map::new("pending_escrow_release");
