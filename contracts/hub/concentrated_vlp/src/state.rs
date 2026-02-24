use cosmwasm_std::{Uint128, Uint256};
use cw_storage_plus::{Item, Map};
use euclid::{
    cross_chain_user::CrossChainUser,
    msgs::vlp::{
        base::{PoolKey, State},
        concentrated::msg::LegacyLiquidityMode,
    },
    token::Token,
};
use euclid::{chain::ChainUid, error::ContractError};
use cosmwasm_std::Order;

pub const STATE: Item<State> = Item::new("state");

pub const CHAIN_LP_TOKENS: Map<ChainUid, Uint128> = Map::new("chain_lp_tokens");

pub const BALANCES: Map<Token, Uint128> = Map::new("balances");

pub const COLLATERAL_LP_TOKENS: Item<Uint128> = Item::new("collateral_lp_tokens");

pub const POOL_KEY: Item<PoolKey> = Item::new("pool_key");

pub const MIN_TICK: i64 = -887272;
pub const MAX_TICK: i64 = 887272;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct Slot0 {
    pub sqrt_price_x96: Uint256,
    pub tick: i64,
    pub observation_index: u64,
    pub observation_cardinality: u16,
    pub observation_cardinality_next: u16,
    pub unlocked: bool,
}

pub const SLOT0: Item<Slot0> = Item::new("slot0");
pub const ACTIVE_LIQUIDITY: Item<Uint128> = Item::new("active_liquidity");
pub const FEE_GROWTH_GLOBAL_0_X128: Item<Uint256> = Item::new("fee_growth_global_0_x128");
pub const FEE_GROWTH_GLOBAL_1_X128: Item<Uint256> = Item::new("fee_growth_global_1_x128");
pub const PROTOCOL_FEES_0: Item<Uint128> = Item::new("protocol_fees_0");
pub const PROTOCOL_FEES_1: Item<Uint128> = Item::new("protocol_fees_1");

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct TickInfo {
    pub initialized: bool,
    pub liquidity_gross: Uint128,
    pub liquidity_net: i128,
    pub fee_growth_outside_0_x128: Uint256,
    pub fee_growth_outside_1_x128: Uint256,
}

pub const TICKS: Map<i64, TickInfo> = Map::new("ticks");
pub const TICK_BITMAP: Map<i64, Uint256> = Map::new("tick_bitmap");

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct Observation {
    pub block_timestamp: u64,
    pub tick_cumulative: i128,
    pub seconds_per_liquidity_cumulative_x128: Uint256,
    pub initialized: bool,
}

pub const OBSERVATIONS: Map<u64, Observation> = Map::new("observations");

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct MigrationMetadata {
    pub source_version: String,
    pub mode: LegacyLiquidityMode,
    pub migrated_at: u64,
    pub positions_migrated: u64,
}

pub const MIGRATION_REVISION: Item<u16> = Item::new("migration_revision");
pub const MIGRATION_METADATA: Item<MigrationMetadata> = Item::new("migration_metadata");

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct ConcentratedPosition {
    pub owner: CrossChainUser,
    pub lower_tick_index: i64,
    pub upper_tick_index: i64,
    pub liquidity: Uint128,
    pub pool_key: PoolKey,
    #[serde(default)]
    pub fee_growth_inside_0_last_x128: Uint256,
    #[serde(default)]
    pub fee_growth_inside_1_last_x128: Uint256,
    #[serde(default)]
    pub tokens_owed_0: Uint128,
    #[serde(default)]
    pub tokens_owed_1: Uint128,
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

pub fn initialize_position_namespace_if_missing(
    storage: &mut dyn cosmwasm_std::Storage,
    contract_addr: &str,
) -> Result<(), ContractError> {
    let prefix = match POSITION_ID_PREFIX.may_load(storage)? {
        Some(existing) => existing,
        None => {
            let computed = position_prefix_from_addr(contract_addr);
            POSITION_ID_PREFIX.save(storage, &computed)?;
            computed
        }
    };
    let mut max_nonce = POSITION_NONCE.may_load(storage)?.unwrap_or(0);
    for item in POSITIONS.range(storage, None, None, Order::Ascending) {
        let (id, _) = item?;
        if ((id >> 64) as u64) == prefix {
            let nonce = id as u64;
            if nonce > max_nonce {
                max_nonce = nonce;
            }
        }
    }
    POSITION_NONCE.save(storage, &max_nonce)?;
    Ok(())
}

pub fn next_position_id(
    storage: &mut dyn cosmwasm_std::Storage,
) -> Result<Uint128, ContractError> {
    let prefix = POSITION_ID_PREFIX
        .may_load(storage)?
        .ok_or_else(|| ContractError::new("position id prefix not initialized"))?;
    let mut nonce = POSITION_NONCE.may_load(storage)?.unwrap_or(0);

    loop {
        nonce = nonce
            .checked_add(1)
            .ok_or_else(|| ContractError::new("position id overflow"))?;
        let id = (u128::from(prefix) << 64) | u128::from(nonce);
        if POSITIONS.may_load(storage, id)?.is_none() {
            POSITION_NONCE.save(storage, &nonce)?;
            return Ok(Uint128::new(id));
        }
    }
}
