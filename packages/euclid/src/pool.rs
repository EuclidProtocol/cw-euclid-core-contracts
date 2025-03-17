use crate::{
    chain::CrossChainUser,
    token::{PairWithDenomAndAmount, TokenWithDenom},
};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Uint128, Uint64};

pub const MINIMUM_LIQUIDITY: u128 = 1000;

// Request to create pool saved in state to manage during acknowledgement
#[cw_serde]
pub struct PoolCreateRequest {
    // Request sender
    pub sender: String,
    // Pool request id
    pub tx_id: String,
    // Pool Pair
    pub pair_info: PairWithDenomAndAmount,
    pub lp_token_instantiate_msg: cw20_base::msg::InstantiateMsg,
}

// Request to create pool saved in state to manage during acknowledgement
#[cw_serde]
pub struct PoolWithLiquidityCreateRequest {
    // Request sender
    pub sender: String,
    // Pool request id
    pub tx_id: String,
    // Pool Pair
    pub pair_info: PairWithDenomAndAmount,
    pub lp_token_instantiate_msg: cw20_base::msg::InstantiateMsg,
}

#[cw_serde]
pub struct DenomRegisterDeregisterRequest {
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
    pub tx_id: String,
    pub mint_lp_tokens: Uint128,
    pub sender: CrossChainUser,
}

#[cw_serde]
pub struct RegisterDenomResponse {}

#[cw_serde]
pub struct DeRegisterDenomResponse {}


#[cw_serde]
pub enum PoolConfig {
    Stable { amp_factor: Option<Uint64> },
    ConstantProduct {},
}

// #[cw_serde]
// pub struct PoolInstantiateMsg {
//     pub router: String,
//     pub virtual_balance: String,
//     pub pair: Pair,
//     pub fee: Fee,
//     pub execute: Option<PoolExecuteMsg>,
//     pub admin: String,
//     pub amp_factor: Option<Uint64>,
// }

// #[cw_serde]
// pub enum PoolExecuteMsg {
//     Stable(StableExecuteMsg),
//     ConstantProduct(ConstantProductExecuteMsg),
// }
