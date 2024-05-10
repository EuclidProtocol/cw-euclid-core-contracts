use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;
use cw20::Cw20ReceiveMsg;
use euclid::{pool::Pool, token::{Pair, PairInfo, TokenInfo}};

#[cw_serde]
pub struct InstantiateMsg {
    pub vlp_contract: String,
    pub token_pair: Pair,
    pub pair_info: PairInfo,
    pub pool: Pool,
    pub chain_id: String,
}

#[cw_serde]
pub enum ExecuteMsg {
    ExecuteSwap {
        asset: TokenInfo, 
        asset_amount: Uint128,
        min_amount_out: Uint128,
        channel: String,

    },

    // Recieve CW20 TOKENS structure
    Receive (Cw20ReceiveMsg),
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
}


// CW20 Hook Msg
#[cw_serde]
pub enum Cw20HookMsg {
    Swap {
        asset: TokenInfo,
        min_amount_out: Uint128,
        channel: String,
    },
}
