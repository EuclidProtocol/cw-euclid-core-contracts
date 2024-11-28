use crate::{
    chain::{ChainUid, CrossChainUser, CrossChainUserWithLimit},
    fee::{DenomFees, PartnerFee},
    liquidity::{AddLiquidityRequest, RemoveLiquidityRequest},
    swap::{NextSwapPair, SwapRequest},
    token::{Pair, PairWithDenomAndAmount, Token, TokenType, TokenWithDenom},
    utils::pagination::Pagination,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use cosmwasm_schema::QueryResponses;
use cosmwasm_std::{Addr, Binary, IbcPacketAckMsg, IbcPacketReceiveMsg, Uint128};
use snip20_reference_impl::receiver::Snip20ReceiveMsg;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct InstantiateMsg {
    // Router contract on VLP
    pub router_contract: String,
    //Applicable if router is on secret which is impossible
    pub router_contract_code_hash: Option<String>,
    pub chain_uid: ChainUid,
    pub escrow_code_id: u64,
    pub escrow_code_hash: String,
    pub snip20_code_id: u64,
    pub snip20_code_hash: String,
    pub is_native: bool,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
// #[derive(cw_orch::ExecuteFns)]
pub enum ExecuteMsg {
    AddLiquidityRequest {
        pair_info: PairWithDenomAndAmount,
        slippage_tolerance_bps: u64,
        timeout: Option<u64>,
    },
    ExecuteSwapRequest {
        asset_in: TokenWithDenom,
        amount_in: Uint128,
        asset_out: Token,
        min_amount_out: Uint128,
        timeout: Option<u64>,
        swaps: Vec<NextSwapPair>,
        // First element in array has highest priority
        cross_chain_addresses: Vec<CrossChainUserWithLimit>,

        partner_fee: Option<PartnerFee>,
    },
    RequestRegisterDenom {
        token: TokenWithDenom,
        timeout: Option<u64>,
    },
    RequestDeregisterDenom {
        token: TokenWithDenom,
        timeout: Option<u64>,
    },
    RequestPoolCreation {
        pair: PairWithDenomAndAmount,
        slippage_tolerance_bps: u64,
        timeout: Option<u64>,
        lp_token_name: String,
        lp_token_symbol: String,
        lp_token_decimal: u8,
    },
    UpdateHubChannel {
        new_channel: String,
    },
    WithdrawVirtualBalance {
        token: Token,
        amount: Uint128,
        cross_chain_addresses: Vec<CrossChainUserWithLimit>,
        timeout: Option<u64>,
    },
    TransferVirtualBalance {
        token: Token,
        amount: Uint128,
        recipient_address: CrossChainUser,
        timeout: Option<u64>,
    },
    DepositToken {
        asset_in: TokenWithDenom,
        amount_in: Uint128,
        timeout: Option<u64>,
        recipient: Option<CrossChainUser>,
    },
    UpdateFactoryState {
        // The Router Contract Address on the Virtual Settlement Layer
        router_contract: Option<String>,
        router_contract_code_hash: Option<String>,
        // Contract admin
        admin: Option<String>,
        // Escrow Code ID
        escrow_code_id: Option<u64>,
        // Escrow Code Hash
        escrow_code_hash: Option<String>,
        // SNIP20 Code ID
        snip20_code_id: Option<u64>,
        // SNIP20 Code Hash
        snip20_code_hash: Option<String>,
        is_native: Option<bool>,
    },
    // Recieve CW20 TOKENS structure
    Receive(Snip20ReceiveMsg),

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
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug,QueryResponses)]
pub enum QueryMsg {
    #[returns(GetVlpResponse)]
    GetVlp { pair: Pair },

    #[returns(GetLPTokenResponse)]
    GetLPToken { vlp: String },

    #[returns(StateResponse)]
    GetState {},

    #[returns(PartnerFeesCollectedResponse)]
    GetPartnerFeesCollected {},

    // Query to get all pools in the factory
    #[returns(AllPoolsResponse)]
    GetAllPools {},

    // Query to get all pools in the factory
    #[returns(AllTokensResponse)]
    GetAllTokens {},

    // Fetch pending swaps with pagination for a user
    #[returns(GetPendingSwapsResponse)]
    PendingSwapsUser {
        user: Addr,
        pagination: Pagination<Uint128>,
    },
    #[returns(GetPendingLiquidityResponse)]
    PendingLiquidity {
        user: Addr,
        pagination: Pagination<Uint128>,
    },
    #[returns(GetPendingRemoveLiquidityResponse)]
    PendingRemoveLiquidity {
        user: Addr,
        pagination: Pagination<Uint128>,
    },

    #[returns(GetEscrowResponse)]
    GetEscrow { token_id: String },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct GetVlpResponse {
    pub vlp_address: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct GetLPTokenResponse {
    pub token_address: Addr,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct GetEscrowResponse {
    pub escrow_address: Addr,
    pub escrow_code_hash: String,
    pub denoms: Vec<TokenType>,
}
// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct StateResponse {
    pub chain_uid: ChainUid,
    pub router_contract: String,
    pub hub_channel: Option<String>,
    pub admin: String,
    // Escrow Code ID
    pub escrow_code_id: u64,
    pub escrow_code_hash: String,
    // Snip20 Code ID
    pub snip20_code_id: u64,
    pub snip20_code_hash: String,
    pub is_native: bool,
    pub partner_fees_collected: DenomFees,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct PartnerFeesCollectedResponse {
    pub total: DenomFees,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct PartnerFeesCollectedPerDenomResponse {
    pub total: Uint128,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct AllPoolsResponse {
    pub pools: Vec<PoolVlpResponse>, // Assuming pool addresses are strings
}
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct PoolVlpResponse {
    pub pair: Pair,
    pub vlp: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct RegisterFactoryResponse {
    pub factory_address: String,
    pub chain_id: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ReleaseEscrowResponse {
    pub factory_address: String,
    pub chain_id: String,
    pub amount: Uint128,
    pub token: Token,
    pub to_address: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct GetPendingSwapsResponse {
    pub pending_swaps: Vec<SwapRequest>,
}
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct GetPendingLiquidityResponse {
    pub pending_add_liquidity: Vec<AddLiquidityRequest>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct GetPendingRemoveLiquidityResponse {
    pub pending_remove_liquidity: Vec<RemoveLiquidityRequest>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct AllTokensResponse {
    pub tokens: Vec<Token>, // Assuming pool addresses are strings
}
