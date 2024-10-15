use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;

use crate::token::{PairWithDenom, TokenWithDenom};

pub const MINIMUM_LIQUIDITY: u128 = 1000;

// Request to create pool saved in state to manage during acknowledgement
#[cw_serde]
pub struct PoolCreateRequest {
    // Request sender
    pub sender: String,
    // Pool request id
    pub tx_id: String,
    // Pool Pair
    pub pair_info: PairWithDenom,
    pub lp_token_instantiate_msg: cw20_base::msg::InstantiateMsg,
}

#[cw_serde]
pub struct EscrowCreateRequest {
    // Request sender
    pub sender: String,
    // Escrow request id
    pub tx_id: String,
    // Escrow Token
    pub token: TokenWithDenom,
}

// Struct to handle Acknowledgement Response for a Pool Creation Request
#[cw_serde]
pub struct PoolCreationResponse {
    pub vlp_contract: String,
}

#[cw_serde]
pub struct PoolCreationWithFundsResponse {
    pub mint_lp_tokens: Uint128,
    pub vlp_contract: String,
}

#[cw_serde]
pub struct EscrowCreationResponse {}
