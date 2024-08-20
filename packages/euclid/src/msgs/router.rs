use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary, IbcPacketAckMsg, IbcPacketReceiveMsg, Uint128};

use crate::{
    chain::{Chain, ChainUid, CrossChainUser, CrossChainUserWithLimit},
    swap::NextSwapPair,
    token::{Pair, Token},
};
#[cw_serde]
pub struct InstantiateMsg {
    // Pool Code ID
    pub vlp_code_id: u64,
    pub vcoin_code_id: u64,
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
    // Update Pool Code ID
    UpdateVLPCodeId {
        new_vlp_code_id: u64,
    },
    RegisterFactory {
        chain_uid: ChainUid,
        chain_info: RegisterFactoryChainType,
    },
    ReleaseEscrowInternal {
        sender: CrossChainUser,
        token: Token,
        amount: Uint128,
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
        start: Option<(Token, Token)>,
        end: Option<(Token, Token)>,
        skip: Option<usize>,
        limit: Option<usize>,
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
        start: Option<ChainUid>,
        end: Option<ChainUid>,
        skip: Option<usize>,
        limit: Option<usize>,
    },

    #[returns(AllTokensResponse)]
    QueryAllTokens {
        start: Option<Token>,
        end: Option<Token>,
        skip: Option<usize>,
        limit: Option<usize>,
    },
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
    pub vcoin_address: Option<Addr>,
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
    pub chains: Vec<ChainUid>,
}

#[cw_serde]
pub struct TokenResponse {
    pub token: Token,
    pub chain_uid: ChainUid,
}

#[cw_serde]
pub struct AllTokensResponse {
    pub tokens: Vec<TokenResponse>,
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
