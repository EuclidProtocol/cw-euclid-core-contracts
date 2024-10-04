use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary, IbcPacketAckMsg, IbcPacketReceiveMsg, Uint128};

use crate::{
    chain::{Chain, ChainUid, CrossChainUser, CrossChainUserWithLimit},
    swap::NextSwapPair,
    token::{Pair, Token},
    utils::pagination::Pagination,
};
#[cw_serde]
pub struct InstantiateMsg {
    // Pool Code ID
    pub vlp_code_id: u64,
    pub virtual_balance_code_id: u64,
}

#[cw_serde]
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
    TransferVirtualBalance {
        token: Token,
        recipient: CrossChainUser,
        amount: Option<Uint128>,
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

#[cw_serde]
#[derive(QueryResponses)]
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
}
// We define a custom struct for each query response
#[cw_serde]
pub struct MigrateMsg {}

#[cw_serde]
pub struct QuerySimulateSwap {
    pub asset_in: Token,
    pub amount_in: Uint128,
    pub asset_out: Token,
    pub min_amount_out: Uint128,
    pub swaps: Vec<NextSwapPair>,
}

#[cw_serde]
pub struct StateResponse {
    pub admin: String,
    pub vlp_code_id: u64,
    pub virtual_balance_address: Option<Addr>,
    pub locked: bool,
}

#[cw_serde]
pub struct AllVlpResponse {
    pub vlps: Vec<VlpResponse>,
}

#[cw_serde]
pub struct VlpResponse {
    pub vlp: String,
    pub token_1: Token,
    pub token_2: Token,
}

#[cw_serde]
pub struct ChainResponse {
    pub chain: Chain,
    pub chain_uid: ChainUid,
}

#[cw_serde]
pub struct AllChainResponse {
    pub chains: Vec<ChainResponse>,
}

#[cw_serde]
pub struct SimulateSwapResponse {
    pub amount_out: Uint128,
    pub asset_out: Token,
}

#[cw_serde]
pub struct SimulateEscrowReleaseResponse {
    pub remaining_amount: Uint128,
    pub release_amounts: Vec<(Uint128, CrossChainUserWithLimit)>,
}

#[cw_serde]
pub struct TokenEscrowsResponse {
    pub chains: Vec<TokenEscrowChainResponse>,
}

#[cw_serde]
pub struct TokenEscrowChainResponse {
    pub chain_uid: ChainUid,
    pub balance: Uint128,
}

#[cw_serde]
pub struct EscrowResponse {
    pub token: Token,
    pub chain_uid: ChainUid,
    pub balance: Uint128,
}

#[cw_serde]
pub struct AllEscrowsResponse {
    pub escrows: Vec<EscrowResponse>,
}

#[cw_serde]
pub struct AllTokensResponse {
    pub tokens: Vec<Token>,
}

#[cw_serde]
pub enum RegisterFactoryChainType {
    Native(RegisterFactoryChainNative),
    Ibc(RegisterFactoryChainIbc),
}

#[cw_serde]
pub struct RegisterFactoryChainNative {
    pub factory_address: String,
}
#[cw_serde]
pub struct RegisterFactoryChainIbc {
    pub channel: String,
    pub timeout: Option<u64>,
}
