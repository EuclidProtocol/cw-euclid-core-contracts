use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary, Uint128};

use crate::{
    admin::{AdminType, EuclidAdmin},
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    utils::pagination::Pagination,
    voucher::{BalanceKey, SerializedBalanceKey},
};

#[cw_serde]
pub struct State {
    pub router: Addr,
}

#[cw_serde]

pub struct InstantiateMsg {
    pub router: Addr,
    pub admin: Option<EuclidAdmin>,
}

#[cw_serde]
pub enum ExecuteMsg {
    Mint(ExecuteMint),
    Transfer(ExecuteTransfer),
    Burn(ExecuteBurn),
    UpdateAdmin {
        new_admin: String,
        admin_type: AdminType,
    },
    UpdateRouter {
        router: Addr,
    },
    RemoveZeroStateValues {
        start_after: Option<SerializedBalanceKey>,
        limit: Option<u32>,
    },
    NormalizeBalanceKeys {
        skip: Option<u32>,
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
pub struct MigrateMsg {}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    // Query to simulate a swap for the asset
    #[returns(State)]
    GetState {},

    #[returns(EuclidAdmin)]
    GetAdmin {},

    // Query to simulate a swap for the asset
    #[returns(GetBalanceResponse)]
    GetBalance { balance_key: BalanceKey },

    #[returns(GetAllowanceResponse)]
    GetAllowance { balance_key: BalanceKey },

    // Query to simulate a swap for the asset
    #[returns(GetUserBalancesResponse)]
    GetUserBalances {
        user: CrossChainUser,
        pagination: Option<Pagination<Uint128>>,
    },
    #[returns(GetAllBalancesResponse)]
    GetAllBalances {
        pagination: Option<Pagination<Uint128>>,
    },
    #[returns(GetTokenBalancesResponse)]
    GetTokenBalances {
        token_id: String,
        pagination: Option<Pagination<Uint128>>,
    },
}

#[cw_serde]
pub struct GetBalanceResponse {
    pub amount: Uint128,
}

#[cw_serde]
pub struct Allowance {
    pub spender: CrossChainUser,
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

#[cw_serde]
pub struct GetAllBalancesResponse {
    pub balances: Vec<GetAllBalancesResponseItem>,
}

#[cw_serde]
pub struct GetAllBalancesResponseItem {
    pub balance: Uint128,
    pub address: String,
    pub token_id: String,
    pub chain_uid: ChainUid,
}

#[cw_serde]
pub struct GetTokenBalancesResponse {
    pub balances: Vec<GetTokenBalancesResponseItem>,
}

#[cw_serde]
pub struct GetTokenBalancesResponseItem {
    pub balance: Uint128,
    pub chain_uid: ChainUid,
}
