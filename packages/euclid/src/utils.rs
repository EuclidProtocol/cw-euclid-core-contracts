use cosmwasm_schema::cw_serde;
use cosmwasm_std::{DepsMut, Env};
use cw_storage_plus::Item;

use crate::{chain::CrossChainUser, error::ContractError};

const TX_NONCE: Item<u128> = Item::new("tx_nonce");

pub fn generate_tx(
    deps: DepsMut,
    env: &Env,
    sender: &CrossChainUser,
) -> Result<String, ContractError> {
    let sender = sender.to_sender_string();
    let height = env.block.height;
    let chain_id = env.block.chain_id.clone();
    let index = env
        .transaction
        .clone()
        .map(|tx| tx.index)
        .unwrap_or_default();
    let mut nonce = TX_NONCE.may_load(deps.storage)?.unwrap_or_default();
    nonce = nonce.wrapping_add(1);
    TX_NONCE.save(deps.storage, &nonce)?;
    Ok(format!("{sender}:{chain_id}:{height}:{index}:{nonce}"))
}

#[cw_serde]
pub struct Pagination<T> {
    pub min: Option<T>,
    pub max: Option<T>,
    pub skip: Option<u64>,
    pub limit: Option<u64>,
}

impl<T: ToString> Pagination<T> {
    // Creates a new instance of Pagination
    pub fn new(min: Option<T>, max: Option<T>, skip: Option<u64>, limit: Option<u64>) -> Self {
        Pagination {
            min,
            max,
            skip,
            limit,
        }
    }
}
