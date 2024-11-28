use cosmwasm_schema::QueryResponses;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use cosmwasm_std::{Addr, Binary, IbcPacketAckMsg, IbcPacketReceiveMsg, Uint128};

use crate::{
    chain::{Chain, ChainUid, CrossChainUser, CrossChainUserWithLimit},
    swap::NextSwapPair,
    token::{Pair, Token, TokenType},
    utils::pagination::Pagination,
};
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct InstantiateMsg {
    // Pool Code ID
    pub vlp_code_id: u64,
    pub virtual_balance_code_id: u64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[derive(cw_orch::ExecuteFns)]
pub enum ExecuteMsg {
    ReregisterChain {
        chain: ChainUid,
    },
    DeregisterChain {
        chain: ChainUid,
    },
    UpdateFactoryChannel {
        chain_uid: ChainUid,
        channel: String,
    },
    UpdateLock {},
    RegisterFactory {
        chain_uid: ChainUid,
        chain_info: RegisterFactoryChainType,
    },
    WithdrawVoucher {
        token: Token,
        amount: Option<Uint128>,
        cross_chain_addresses: Vec<CrossChainUserWithLimit>,
        timeout: Option<u64>,
    },
    ReleaseEscrowInternal {
        sender: CrossChainUser,
        token: Token,
        amount: Option<Uint128>,
        cross_chain_addresses: Vec<CrossChainUserWithLimit>,
        timeout: Option<u64>,
        tx_id: String,
    },
    // IBC Callbacks
    IbcCallbackAckAndTimeout {
        ack: IbcPacketAckMsg,
    },
    // IBC Callbacks
    IbcCallbackReceive {
        receive_msg: IbcPacketReceiveMsg,
    },

    NativeReceiveCallback {
        msg: Binary,
        chain_uid: ChainUid,
    },
    UpdateRouterState {
        // Contract admin
        admin: Option<String>,
        // Pool Code ID
        vlp_code_id: Option<u64>,
        virtual_balance_address: Option<Addr>,
        locked: Option<bool>,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[derive(cw_orch::QueryFns, QueryResponses)]
pub enum QueryMsg {
    #[returns(StateResponse)]
    GetState {},
    #[returns(ChainResponse)]
    GetChain { chain_uid: ChainUid },
    #[returns(AllChainResponse)]
    GetAllChains {},
    #[returns(VlpResponse)]
    GetVlp { pair: Pair },
    #[returns(AllVlpResponse)]
    GetAllVlps {
        pagination: Pagination<(Token, Token)>,
    },
    #[returns(SimulateSwapResponse)]
    SimulateSwap(QuerySimulateSwap),

    #[returns(SimulateEscrowReleaseResponse)]
    SimulateReleaseEscrow {
        token: Token,
        amount: Uint128,
        cross_chain_addresses: Vec<CrossChainUserWithLimit>,
    },

    #[returns(TokenEscrowsResponse)]
    QueryTokenEscrows {
        token: Token,
        pagination: Pagination<ChainUid>,
    },
    #[returns(AllEscrowsResponse)]
    QueryAllEscrows { pagination: Pagination<Token> },

    #[returns(AllTokensResponse)]
    QueryAllTokens { pagination: Pagination<Token> },

    #[returns(TokenDenomsResponse)]
    QueryTokenDenoms { token: Token },
}
// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct QuerySimulateSwap {
    pub asset_in: Token,
    pub amount_in: Uint128,
    pub asset_out: Token,
    pub min_amount_out: Uint128,
    pub swaps: Vec<NextSwapPair>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct StateResponse {
    pub admin: String,
    pub vlp_code_id: u64,
    pub virtual_balance_address: Option<Addr>,
    pub locked: bool,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct AllVlpResponse {
    pub vlps: Vec<VlpResponse>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct VlpResponse {
    pub vlp: String,
    pub token_1: Token,
    pub token_2: Token,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ChainResponse {
    pub chain: Chain,
    pub chain_uid: ChainUid,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct AllChainResponse {
    pub chains: Vec<ChainResponse>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct SimulateSwapResponse {
    pub amount_out: Uint128,
    pub asset_out: Token,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct SimulateEscrowReleaseResponse {
    pub remaining_amount: Uint128,
    pub release_amounts: Vec<(Uint128, CrossChainUserWithLimit)>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct TokenEscrowsResponse {
    pub chains: Vec<TokenEscrowChainResponse>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct TokenEscrowChainResponse {
    pub chain_uid: ChainUid,
    pub balance: Uint128,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct EscrowResponse {
    pub token: Token,
    pub chain_uid: ChainUid,
    pub balance: Uint128,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct AllEscrowsResponse {
    pub escrows: Vec<EscrowResponse>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct AllTokensResponse {
    pub tokens: Vec<Token>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct TokenDenom {
    pub chain_uid: ChainUid,
    pub token_type: TokenType,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct TokenDenomsResponse {
    pub denoms: Vec<TokenDenom>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub enum RegisterFactoryChainType {
    Native(RegisterFactoryChainNative),
    Ibc(RegisterFactoryChainIbc),
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct RegisterFactoryChainNative {
    pub factory_address: String,
}
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct RegisterFactoryChainIbc {
    pub channel: String,
    pub timeout: Option<u64>,
}
