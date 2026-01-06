use cosmwasm_schema::cw_serde;
use cosmwasm_std::{ensure, Storage, Uint128};
use cw_storage_plus::{Item, Map};
use euclid::{
    chain::{ChainUid, CrossChainUser},
    error::ContractError,
    msgs::virtual_balance::State,
    virtual_balance::SerializedBalanceKey,
};

pub const STATE: Item<State> = Item::new("state");

pub const BALANCES: Map<SerializedBalanceKey, Uint128> = Map::new("snapshot_balances");

#[cw_serde]
pub struct Allowance {
    pub spender: CrossChainUser,
    pub amount: Uint128,
}

// Allowance is stored as a map of balance key to allowance. It allows another user to spend on behalf of the owner.
// Only 1 allowance per balance key is allowed at a time.
pub const ALLOWANCES: Map<SerializedBalanceKey, Allowance> = Map::new("allowances");

// A map of ChainUid and TokenId to the blockheight when the token was paused
pub const PAUSED_TOKENS: Map<(ChainUid, String), u64> = Map::new("paused_tokens");

/// Returns an error if the token is paused
pub(crate) fn token_pause_check(
    storage: &dyn Storage,
    chain_uid: ChainUid,
    token_id: String,
) -> Result<(), ContractError> {
    let paused_tokens = PAUSED_TOKENS
        .load(storage, (chain_uid.clone(), token_id.clone()))
        .unwrap_or(0);
    ensure!(
        paused_tokens == 0,
        ContractError::TokenPaused {
            msg: "This token's operation is paused, withdrawal is available".to_string(),
        }
    );
    Ok(())
}
