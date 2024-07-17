use cosmwasm_std::Env;

use crate::chain::CrossChainUser;

pub fn generate_tx(env: &Env, sender: &CrossChainUser) -> String {
    let sender = sender.to_sender_string();
    let height = env.block.height;
    let chain_id = env.block.chain_id.clone();
    let index = env.transaction.clone().map(|tx| tx.index).unwrap_or_default();
    format!("{sender}:{chain_id}:{height}:{index}")
}
