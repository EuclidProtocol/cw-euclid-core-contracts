use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;

use crate::{
    error::ContractError,
    token::{Pair, PairWithDenom, Token},
};

pub const MINIMUM_LIQUIDITY: u128 = 1000;

#[cw_serde]
pub struct Pool {
    pub pair: Pair,
    // The total reserve of token_1
    pub reserve_1: Uint128,
    // The total reserve of token_2
    pub reserve_2: Uint128,
}

impl Pool {
    pub fn new(pair: Pair, reserve_1: Uint128, reserve_2: Uint128) -> Pool {
        Pool {
            pair,
            reserve_1,
            reserve_2,
        }
    }

    pub fn get_reserve(&self, token: Token) -> Result<Uint128, ContractError> {
        if token == self.pair.token_1 {
            Ok(self.reserve_1)
        } else if token == self.pair.token_2 {
            Ok(self.reserve_2)
        } else {
            Err(ContractError::AssetDoesNotExist {})
        }
    }
}

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

// Struct to handle Acknowledgement Response for a Pool Creation Request
#[cw_serde]
pub struct PoolCreationResponse {
    pub vlp_contract: String,
}
