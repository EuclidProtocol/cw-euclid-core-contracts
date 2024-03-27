use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;

use crate::token::PairInfo;

#[cw_serde]
pub struct Pool {
    // The chain where the pool is deployed
    pub chain: String,
    // The smart contract address of the pool
    pub contract_address: String,
    // The PairInfo of the pool
    pub pair: PairInfo,
    // The total reserve of token_1 
    pub reserve_1: Uint128,
    // The total reserve of token_2
    pub reserve_2: Uint128,
}