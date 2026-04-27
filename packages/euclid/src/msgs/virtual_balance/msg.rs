use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{to_json_binary, Addr, Binary, Coin, Timestamp, Uint256, WasmMsg};

use crate::{
    admin::{AdminType, EuclidAdmin},
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    error::ContractError,
    token::{TokenMetadata, TokenType},
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
    NormalizeBalanceKeys {
        skip: Option<u32>,
        limit: Option<u32>,
    },
    Approve(ExecuteApprove),
    RegisterTokenMetadata {
        token_metadata: TokenMetadata,
    },
    DeregisterTokenMetadata {
        token_id: String,
        chain_uid: ChainUid,
        token_type: TokenType,
    },
}

impl ExecuteMsg {
    pub fn to_wasm_msg(
        self,
        contract_addr: String,
        funds: Vec<Coin>,
    ) -> Result<WasmMsg, ContractError> {
        let msg_binary = to_json_binary(&self)?;
        let msg = WasmMsg::Execute {
            contract_addr,
            msg: msg_binary,
            funds,
        };
        Ok(msg)
    }
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
    pub voucher_amount: Uint256,
    pub from_user: CrossChainUser,
    pub token_id: String,
    pub release_denom: TokenType,
    pub release_chain_uid: ChainUid,
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

    #[returns(GetAllowanceResponse)]
    GetAllowance { balance_key: BalanceKey },

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
    #[returns(GetTokenEscrowsResponse)]
    GetTokenEscrows {
        token_id: String,
        pagination: Option<Pagination<(ChainUid, String)>>,
    },
    #[returns(GetAllEscrowBalancesResponse)]
    GetAllEscrowBalances {
        pagination: Option<Pagination<(String, ChainUid, String)>>,
    },
    #[returns(GetTokenMetadataByDenomResponse)]
    GetTokenMetadataByDenom {
        token_id: String,
        chain_uid: ChainUid,
        token_type: TokenType,
    },
    #[returns(GetTokenMetadataResponse)]
    GetTokenMetadata {
        token_id: String,
        pagination: Option<Pagination<(ChainUid, TokenType)>>,
    },
    #[returns(GetAllTokenMetadataResponse)]
    GetAllTokenMetadata {
        pagination: Option<Pagination<(String, ChainUid, String)>>,
    },
    #[returns(GetTokenRegisteredResponse)]
    GetTokenRegistered { token_id: String },
}

#[cw_serde]
pub struct GetTokenEscrowsResponse {
    pub escrows: Vec<GetTokenEscrowsResponseItem>,
}

#[cw_serde]
pub struct GetTokenEscrowsResponseItem {
    pub balance: Uint256,
    pub chain_uid: ChainUid,
    pub token_type: TokenType,
}

#[cw_serde]
pub struct GetAllEscrowBalancesResponse {
    pub escrows: Vec<GetAllEscrowBalancesResponseItem>,
}

#[cw_serde]
pub struct GetAllEscrowBalancesResponseItem {
    pub token_id: String,
    pub chain_uid: ChainUid,
    pub token_type: TokenType,
    pub balance: Uint256,
}

#[cw_serde]
pub struct GetTokenMetadataResponse {
    pub metadata: Vec<TokenMetadata>,
}

#[cw_serde]
pub struct GetTokenMetadataByDenomResponse {
    pub metadata: TokenMetadata,
}

#[cw_serde]
pub struct GetAllTokenMetadataResponse {
    pub metadata: Vec<TokenMetadata>,
}

#[cw_serde]
pub struct GetBalanceResponse {
    pub amount: Uint256,
}

#[cw_serde]
pub struct Allowance {
    pub spender: CrossChainUser,
    pub amount: Uint256,
}

#[cw_serde]
pub struct VoucherAllowance {
    // The user who is allowed to spend the tokens.
    pub spender: CrossChainUser,
    // The amount of tokens that the spender is allowed to spend.
    pub amount: Uint256,
    // The allowance expires at the given timestamp. If None, the allowance never expires.
    pub expires_at: Option<Timestamp>,
}

#[cw_serde]
pub struct GetAllowanceResponse {
    pub allowance: VoucherAllowance,
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

#[cw_serde]
pub struct GetTokenRegisteredResponse {
    pub token_registered: bool,
}
