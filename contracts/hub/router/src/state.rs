use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};
use euclid::{
    admin::EuclidAdmin,
    chain::{Chain, ChainUid},
    msgs::router::TokenDenom,
    token::{PairWithDenomAndAmount, Token},
};
use euclid_ibc::router_ibc::{
    RouterCrossChainRemoveLiquidityExecuteMsg, RouterCrossChainSwapExecuteMsg,
};

#[cw_serde]
pub struct State {
    // Contract admin
    pub admins: EuclidAdmin,
    // Pools
    pub constant_product_vlp_code_id: u64,
    pub stable_vlp_code_id: u64,

    pub locked: bool,
}

pub const STATE: Item<State> = Item::new("state");

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

#[cw_serde]
pub struct PendingReleaseVoucher {
    pub total_amount: Uint128,
    pub release_fee_amount: Uint128,
    pub unsafe_refund_voucher: bool,
}
// Tx Id to Release Voucher Request
pub const PENDING_RELEASE_VOUCHER: Map<String, PendingReleaseVoucher> =
    Map::new("pending_release_voucher");

pub const FUNDS_INFO: Item<(PairWithDenomAndAmount, u64)> = Item::new("funds_info");

/// The key is TokenID_ChainUID
pub const RELEASE_FEES: Map<(Token, ChainUid), Uint128> = Map::new("release_fees");
pub const DEFAULT_RELEASE_FEE: Item<Uint128> = Item::new("default_release_fee");

// The key is ChainUid and the value is the timeout in seconds for chain send packets
pub const CHAIN_TIMEOUT_SECONDS: Map<ChainUid, u64> = Map::new("chains_timeout_seconds");
