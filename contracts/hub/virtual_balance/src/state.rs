use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Timestamp, Uint256};
use cw_storage_plus::{Item, Map, Path};
use euclid::{
    admin::EuclidAdmin,
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    msgs::virtual_balance::msg::State,
    token::{TokenMetadata, TokenType},
    voucher::SerializedBalanceKey,
};

pub const STATE: Item<State> = Item::new("state");
pub const ADMIN: Item<EuclidAdmin> = Item::new("admin");

pub const VOUCHER_DECIMAL: u32 = 24;
#[deprecated(note = "BALANCES has been moved to VOUCHER_BALANCES")]
pub const BALANCES: Map<SerializedBalanceKey, Uint256> = Map::new("balances");

// Voucher balances are stored as Uint256 to avoid precision loss.
pub const VOUCHER_BALANCES: Map<SerializedBalanceKey, Uint256> = Map::new("voucher_balances");

#[deprecated(note = "ALLOWANCES has been moved to VOUCHER_ALLOWANCES")]
#[cw_serde]
pub struct Allowance {
    pub spender: CrossChainUser,
    pub amount: Uint256,
}

#[cw_serde]
pub struct VoucherAllowance {
    // The user who is allowed to spend the tokens.
    pub spender: CrossChainUser,
    // The amount of tokens that the spender is allowed to spend.
    pub amount: Uint256,
    // The allowance expires at the given timestamp. If None, the allowance never expires.
    pub expires_at: Option<Timestamp>,
}

// Allowance is stored as a map of balance key to allowance. It allows another user to spend on behalf of the owner.
// Only 1 allowance per balance key is allowed at a time.
#[deprecated(note = "ALLOWANCES has been moved to VOUCHER_ALLOWANCES")]
pub const ALLOWANCES: Map<SerializedBalanceKey, Allowance> = Map::new("allowances");

pub const VOUCHER_ALLOWANCES: Map<SerializedBalanceKey, VoucherAllowance> =
    Map::new("voucher_allowances");

// Token Metadata is stored as a map of token id to token metadata.
pub const TOKEN_METADATA: Map<(String, ChainUid, String), TokenMetadata> =
    Map::new("token_metadata");

pub fn get_token_metadata_key(
    token: String,
    chain_uid: ChainUid,
    token_type: TokenType,
) -> Path<TokenMetadata> {
    TOKEN_METADATA.key((token, chain_uid, token_type.get_key()))
}

// Escrow balances are stored as Uint256 to avoid precision loss.
pub const ESCROW_BALANCES: Map<(String, ChainUid, String), Uint256> = Map::new("escrow_balances");

pub fn get_escrow_balance_key(
    token: String,
    chain_uid: ChainUid,
    token_type: TokenType,
) -> Path<Uint256> {
    ESCROW_BALANCES.key((token, chain_uid, token_type.get_key()))
}
