use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Uint128};

use crate::{token::Token, vcoin::BalanceKey};

#[cw_serde]
pub struct State {
    pub router: String,
    pub admin: Addr,
}

#[cw_serde]
pub struct InstantiateMsg {
    pub router: Addr,
    pub admin: Option<Addr>,
}

#[cw_serde]
pub enum ExecuteMsg {
    Mint(ExecuteMint),
    Transfer(ExecuteTransfer),
    Burn(ExecuteBurn),
}

#[cw_serde]
pub struct ExecuteMint {
    pub amount: Uint128,
    pub balance_key: BalanceKey,
}

#[cw_serde]
pub struct ExecuteTransfer {
    pub amount: Uint128,
    pub token_id: String,

    // Source Address
    pub from_address: String,
    pub from_chain_uid: String,

    // Destination Address
    pub to_address: String,
    pub to_chain_uid: String,
}

#[cw_serde]
pub struct ExecuteBurn {
    pub amount: Uint128,
    pub balance_key: BalanceKey,
}

#[cw_serde]
pub struct MigrateMsg {}

#[cw_serde]
#[derive(QueryResponses)]

pub enum QueryMsg {
    // Query to simulate a swap for the asset
    #[returns(GetStateResponse)]
    GetState {},

    // Query to simulate a swap for the asset
    #[returns(GetBalanceResponse)]
    GetBalance { balance_key: BalanceKey },

    // Query to simulate a swap for the asset
    #[returns(GetUserBalancesResponse)]
    GetUserBalances { chain_id: String, address: String },
}

// We define a custom struct for each query response
#[cw_serde]
pub struct GetStateResponse {
    pub state: State,
}

#[cw_serde]
pub struct GetBalanceResponse {
    pub amount: Uint128,
}

#[cw_serde]
pub struct GetUserBalancesResponse {
    pub balances: Vec<GetUserBalancesResponseItem>,
}

#[cw_serde]
pub struct GetUserBalancesResponseItem {
    pub amount: Uint128,
    pub token_id: String,
}
