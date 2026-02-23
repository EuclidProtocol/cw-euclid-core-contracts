use cosmwasm_std::Uint128;
use cw_storage_plus::{Item, Map};
use euclid::{
    cross_chain_user::CrossChainUser,
    msgs::vlp::base::{PoolKey, State},
    token::Token,
};
use euclid::{chain::ChainUid, error::ContractError};

pub const STATE: Item<State> = Item::new("state");

pub const CHAIN_LP_TOKENS: Map<ChainUid, Uint128> = Map::new("chain_lp_tokens");

pub const BALANCES: Map<Token, Uint128> = Map::new("balances");

pub const COLLATERAL_LP_TOKENS: Item<Uint128> = Item::new("collateral_lp_tokens");

pub const POOL_KEY: Item<PoolKey> = Item::new("pool_key");

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct ConcentratedPosition {
    pub owner: CrossChainUser,
    pub lower_tick_index: i64,
    pub upper_tick_index: i64,
    pub liquidity: Uint128,
    pub pool_key: PoolKey,
}

pub const POSITIONS: Map<u128, ConcentratedPosition> = Map::new("positions");
pub const POSITION_NONCE: Item<u64> = Item::new("position_nonce");
pub const POSITION_ID_PREFIX: Item<u64> = Item::new("position_id_prefix");

fn position_prefix_from_addr(contract_addr: &str) -> u64 {
    // Stable FNV-1a hash so each VLP gets a unique position-id namespace.
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in contract_addr.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

pub fn initialize_position_nonce(
    storage: &mut dyn cosmwasm_std::Storage,
    contract_addr: &str,
) -> Result<(), ContractError> {
    POSITION_NONCE.save(storage, &0)?;
    POSITION_ID_PREFIX.save(storage, &position_prefix_from_addr(contract_addr))?;
    Ok(())
}

pub fn next_position_id(
    storage: &mut dyn cosmwasm_std::Storage,
) -> Result<Uint128, ContractError> {
    let prefix = POSITION_ID_PREFIX
        .may_load(storage)?
        .ok_or_else(|| ContractError::new("position id prefix not initialized"))?;
    let nonce = POSITION_NONCE
        .may_load(storage)?
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| ContractError::new("position id overflow"))?;
    POSITION_NONCE.save(storage, &nonce)?;

    let id = (u128::from(prefix) << 64) | u128::from(nonce);
    Ok(Uint128::new(id))
}
