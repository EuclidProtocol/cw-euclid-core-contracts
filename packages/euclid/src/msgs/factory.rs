use crate::token::{PairInfo, Token};
use crate::{msgs::pool::ExecuteMsg as PoolExecuteMsg, token::TokenInfo};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;
#[cw_serde]
pub struct InstantiateMsg {
    // Router contract on VLP
    pub router_contract: String,
    // Chain ID
    pub chain_id: String,
    // Pool Code ID
    pub pool_code_id: u64,
}

#[cw_serde]
pub enum ExecuteMsg {
    // Request Pool Creation
    RequestPoolCreation {
        pair_info: PairInfo,
        token_1_reserve: Uint128,
        token_2_reserve: Uint128,
        channel: String,
    },
    ExecuteSwap {
        asset: Token,
        asset_amount: Uint128,
        min_amount_out: Uint128,
        channel: String,
        swap_id: String,
    },
    // Add Liquidity Request to the VLP
    AddLiquidity {
        token_1_liquidity: Uint128,
        token_2_liquidity: Uint128,
        slippage_tolerance: u64,
        channel: String,
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {}
