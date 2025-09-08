use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary, Uint128};

use crate::{
    chain::CrossChainUser,
    virtual_balance::{BalanceKey, SerializedBalanceKey},
};

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
#[derive(cw_orch::ExecuteFns)]
pub enum ExecuteMsg {
    Mint(ExecuteMint),
    Transfer(ExecuteTransfer),
    Burn(ExecuteBurn),
    UpdateState {
        router: Option<String>,
        admin: Option<Addr>,
    },
    RemoveZeroStateValues {
        start_after: Option<SerializedBalanceKey>,
        limit: Option<u32>,
    },
    Approve(ExecuteApprove),
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

    // Only router can set sender
    pub sender: Option<CrossChainUser>,

    // Destination Address
    pub to: CrossChainUser,
    // In case of approvals, the sender can set from
    pub from: Option<CrossChainUser>,
    pub msg: Option<Binary>,
}

#[cw_serde]
pub struct ExecuteBurn {
    pub amount: Uint128,
    pub balance_key: BalanceKey,
}

#[cw_serde]
pub struct ExecuteApprove {
    pub amount: Uint128,
    pub token_id: String,
    pub spender: CrossChainUser,
    pub owner: CrossChainUser,
}

#[cw_serde]
pub struct Allowance {
    pub spender: CrossChainUser,
    pub amount: Uint128,
}

#[cw_serde]
pub struct MigrateMsg {}

#[cw_serde]
#[derive(cw_orch::QueryFns, QueryResponses)]
pub enum QueryMsg {
    // Query to simulate a swap for the asset
    #[returns(GetStateResponse)]
    GetState {},

    // Query to simulate a swap for the asset
    #[returns(GetBalanceResponse)]
    GetBalance { balance_key: BalanceKey },

    // Query to simulate a swap for the asset
    #[returns(GetUserBalancesResponse)]
    GetUserBalances { user: CrossChainUser },

    // Query to simulate a swap for the asset
    #[returns(GetAllowanceResponse)]
    GetAllowance { balance_key: BalanceKey },
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
pub struct GetAllowanceResponse {
    pub allowance: Allowance,
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
