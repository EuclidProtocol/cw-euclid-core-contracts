use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;

use crate::{error::ContractError, token::PairInfo};

#[cw_serde]
pub struct LiquidityTxInfo {
    pub sender: String,
    pub token_1_liquidity: Uint128,
    pub token_2_liquidity: Uint128,
    pub liquidity_id: String,
    pub vlp_address: String,
    pub pair_info: PairInfo,
}

#[cw_serde]
pub struct LiquidityExtractedId {
    pub sender: String,
    pub index: u128,
}

// Function to extract sender from swap_id
pub fn parse_liquidity_id(liquidity_id: &str) -> Result<LiquidityExtractedId, ContractError> {
    let parsed: Vec<&str> = liquidity_id.split('-').collect();
    Ok(LiquidityExtractedId {
        sender: parsed[0].to_string(),
        index: parsed[1].parse()?,
    })
}
