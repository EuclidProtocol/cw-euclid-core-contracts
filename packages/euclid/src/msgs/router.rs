use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary, IbcPacketAckMsg, IbcPacketReceiveMsg, Uint128};

use crate::{
    chain::{Chain, ChainUid, CrossChainUser, CrossChainUserWithLimit},
    msgs::hook::MetaReceive,
    swap::NextSwapPair,
    token::{Pair, Token, TokenType},
    utils::pagination::Pagination,
};
#[cw_serde]
pub struct InstantiateMsg {
    pub constant_product_vlp_code_id: u64,
    pub stable_vlp_code_id: u64,

    pub virtual_balance_code_id: u64,
    pub mock_relayer_addresses: Option<Vec<String>>,
}

#[cw_serde]
#[cfg_attr(not(target_arch = "wasm32"), derive(cw_orch::ExecuteFns))]
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
    UpdateRouterState(UpdateRouterState),

    EvmSendPacket {
        msg: Binary,
        chain_uid: ChainUid,
    },

    EvmReceivePacket {
        msg: Binary,
        chain_uid: ChainUid,
        // Store sequence of packet relayed so we don't relay same sequence again
        sequence: u128,
        // Continous hash of the packet to make sure its linked to the same source flow
        hash: String,
    },

    EvmReceivePacketInternalCallback {
        msg: Binary,
        chain_uid: ChainUid,
    },

    EvmReceiveAck {
        msg: Binary,
        chain_uid: ChainUid,
        // Store sequence of packet relayed so we don't relay same sequence again
        sequence: u128,
        // Continous hash of the packet to make sure its linked to the same source flow
        hash: String,
        ack: Binary,
    },

    SolanaSendPacket {
        msg: Binary,
        chain_uid: ChainUid,
    },

    SolanaReceivePacket {
        msg: Binary,
        chain_uid: ChainUid,
        // Store sequence of packet relayed so we don't relay same sequence again
        sequence: u128,
        // Continous hash of the packet to make sure its linked to the same source flow
        hash: String,
    },

    SolanaReceivePacketInternalCallback {
        msg: Binary,
        chain_uid: ChainUid,
    },

    SolanaReceiveAck {
        msg: Binary,
        chain_uid: ChainUid,
        // Store sequence of packet relayed so we don't relay same sequence again
        sequence: u128,
        // Continous hash of the packet to make sure its linked to the same source flow
        hash: String,
        ack: Binary,
    },

    // COSMOS REALYING MSGS
    CosmosSendPacket {
        msg: Binary,
        chain_uid: ChainUid,
    },

    CosmosReceivePacket {
        msg: Binary,
        chain_uid: ChainUid,
        // Store sequence of packet relayed so we don't relay same sequence again
        sequence: u128,
        // Continous hash of the packet to make sure its linked to the same source flow
        hash: String,
    },

    CosmosReceivePacketInternalCallback {
        msg: Binary,
        chain_uid: ChainUid,
    },

    CosmosReceiveAck {
        msg: Binary,
        chain_uid: ChainUid,
        // Store sequence of packet relayed so we don't relay same sequence again
        sequence: u128,
        // Continous hash of the packet to make sure its linked to the same source flow
        hash: String,
        ack: Binary,
    },

    MetaReceive(MetaReceive),
}

#[cw_serde]
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
        pagination: Pagination<(String, String)>,
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
    QueryAllEscrows { pagination: Pagination<String> },

    #[returns(AllTokensResponse)]
    QueryAllTokens { pagination: Pagination<Token> },

    #[returns(TokenDenomsResponse)]
    QueryTokenDenoms { token: Token },

    #[returns(RelayerAddressesResponse)]
    QueryRelayerAddresses {},
}
// We define a custom struct for each query response
#[cw_serde]
pub struct MigrateMsg {
    pub v0_2_0_to_v0_2_1: Option<MigrateV020ToV021>,
}

#[cw_serde]
pub struct MigrateV020ToV021 {
    pub denoms: Vec<(Token, TokenDenom)>,
}

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
    pub constant_product_vlp_code_id: u64,
    pub stable_vlp_code_id: u64,
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
pub struct TokenDenom {
    pub chain_uid: ChainUid,
    pub token_type: TokenType,
}

#[cw_serde]
pub struct TokenDenomsResponse {
    pub denoms: Vec<TokenDenom>,
}

#[cw_serde]
pub struct RelayerAddressesResponse {
    pub relayer_addresses: Vec<String>,
}

#[cw_serde]
pub enum RegisterFactoryChainType {
    Native(RegisterFactoryChainNative),
    Ibc(RegisterFactoryChainIbc),
    Evm(RegisterFactoryChainEvm),
    Solana(RegisterFactoryChainSolana),
}

#[cw_serde]
pub struct RegisterFactoryChainNative {
    pub factory_address: String,
    pub factory_chain_id: String,
}

#[cw_serde]
pub struct RegisterFactoryChainEvm {
    pub factory_address: String,
    pub factory_chain_id: String,
}

#[cw_serde]
pub struct RegisterFactoryChainSolana {
    pub factory_address: String,
    pub factory_chain_id: String,
}

#[cw_serde]
pub struct RegisterFactoryChainIbc {
    pub channel: String,
    pub timeout: Option<u64>,
    pub factory_address: String,
    pub factory_chain_id: String,
}

#[cw_serde]
pub struct UpdateRouterState {
    // Contract admin
    pub admin: Option<String>,
    // Pool Code ID
    pub vlp_code_id: Option<u64>,
    pub stable_vlp_code_id: Option<u64>,
    pub virtual_balance_address: Option<Addr>,
    pub locked: Option<bool>,
    pub mock_relayer_addresses: Option<Vec<String>>,
    pub meta_transaction_contract: Option<Addr>,
}
