use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary, Uint256};

use crate::{
    admin::{AdminType, EuclidAdmin}, chain::ChainUid, cross_chain_user::CrossChainUser, token::{TokenMetadata, TokenType}, utils::pagination::Pagination, voucher::{BalanceKey, SerializedBalanceKey}
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
#[derive(cw_orch::ExecuteFns)]
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
    Approve(ExecuteApprove),
    RegisterTokenMetadata {
        token_metadata: TokenMetadata,
    },
    UpdateTokenMetadata {
        token_metadata: TokenMetadata,
    },
}

#[cw_serde]
pub struct ExecuteMint {
    pub amount: Uint256,
    pub balance_key: BalanceKey,
    pub token_type: TokenType,
    pub token_source_chain_uid: ChainUid,
}

#[cw_serde]
pub struct ExecuteTransfer {
    pub amount: Uint256,
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
    pub amount: Uint256,
    pub balance_key: BalanceKey,
    pub token_type: TokenType,
    pub token_source_chain_uid: ChainUid,
}

#[cw_serde]
pub struct ExecuteApprove {
    pub amount: Uint256,
    pub token_id: String,
    pub spender: CrossChainUser,
    pub owner: CrossChainUser,
}

#[cw_serde]
pub struct MigrateMsg {}

#[cw_serde]
#[derive(cw_orch::QueryFns, QueryResponses)]
pub enum QueryMsg {
    // Query to simulate a swap for the asset
    #[returns(State)]
    GetState {},

    #[returns(EuclidAdmin)]
    GetAdmin {},

    // Query to simulate a swap for the asset
    #[returns(GetBalanceResponse)]
    GetBalance { balance_key: BalanceKey },

    // Query to simulate a swap for the asset
    #[returns(GetUserBalancesResponse)]
    GetUserBalances {
        user: CrossChainUser,
        pagination: Option<Pagination<Uint256>>,
    },
    #[returns(GetAllBalancesResponse)]
    GetAllBalances {
        pagination: Option<Pagination<Uint256>>,
    },
    #[returns(GetTokenBalancesResponse)]
    GetTokenBalances {
        token_id: String,
        pagination: Option<Pagination<Uint256>>,
    },
    #[returns(GetEscrowBalanceResponse)]
    GetEscrowBalance {
        token_id: String,
        chain_uid: ChainUid,
        token_type: TokenType,
    },
}

#[cw_serde]
pub struct GetBalanceResponse {
    pub amount: Uint256,
}

#[cw_serde]
pub struct GetUserBalancesResponse {
    pub balances: Vec<GetUserBalancesResponseItem>,
}

#[cw_serde]
pub struct GetUserBalancesResponseItem {
    pub amount: Uint256,
    pub token_id: String,
}

#[cw_serde]
pub struct GetAllBalancesResponse {
    pub balances: Vec<GetAllBalancesResponseItem>,
}

#[cw_serde]
pub struct GetAllBalancesResponseItem {
    pub balance: Uint256,
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
    pub balance: Uint256,
    pub chain_uid: ChainUid,
}

#[cw_serde]
pub struct GetEscrowBalanceResponse {
    pub balance: Uint256,
}
