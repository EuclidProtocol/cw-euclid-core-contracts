use crate::{
    chain::{ChainUid, CrossChainUser, CrossChainUserWithLimit},
    fee::{DenomFees, PartnerFee},
    liquidity::{AddLiquidityRequest, RemoveLiquidityRequest},
    swap::{NextSwapPair, SwapRequest},
    token::{Pair, PairWithDenom, PairWithDenomAndAmount, Token, TokenType, TokenWithDenom},
    utils::pagination::Pagination,
};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary, IbcPacketAckMsg, IbcPacketReceiveMsg, Uint128};
use cw20::Cw20ReceiveMsg;

#[cw_serde]
pub struct InstantiateMsg {
    // Router contract on VLP
    pub router_contract: String,
    pub chain_uid: ChainUid,
    pub escrow_code_id: u64,
    pub cw20_code_id: u64,
    pub is_native: bool,
}

#[cw_serde]
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
    },
    RequestDeregisterDenom {
        token: TokenWithDenom,
    },
    RequestPoolCreation {
        pair: PairWithDenom,
        timeout: Option<u64>,
        lp_token_name: String,
        lp_token_symbol: String,
        lp_token_decimal: u8,
        lp_token_marketing: Option<cw20_base::msg::InstantiateMarketingInfo>,
    },
    RequestRegisterEscrow {
        token: TokenWithDenom,
        timeout: Option<u64>,
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
        cross_chain_addresses: Vec<CrossChainUserWithLimit>,
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
        // Contract admin
        admin: Option<String>,
        // Escrow Code ID
        escrow_code_id: Option<u64>,
        // CW20 Code ID
        cw20_code_id: Option<u64>,
        is_native: Option<bool>,
    },
    // Recieve CW20 TOKENS structure
    Receive(Cw20ReceiveMsg),

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

#[cw_serde]
#[derive(QueryResponses)]
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

#[cw_serde]
pub struct GetVlpResponse {
    pub vlp_address: String,
}

#[cw_serde]
pub struct GetLPTokenResponse {
    pub token_address: Addr,
}

#[cw_serde]
pub struct GetEscrowResponse {
    pub escrow_address: Option<Addr>,
    pub denoms: Vec<TokenType>,
}
// We define a custom struct for each query response
#[cw_serde]
pub struct StateResponse {
    pub chain_uid: ChainUid,
    pub router_contract: String,
    pub hub_channel: Option<String>,
    pub admin: String,
    // Escrow Code ID
    pub escrow_code_id: u64,
    // CW20 Code ID
    pub cw20_code_id: u64,
    pub is_native: bool,
    pub partner_fees_collected: DenomFees,
}

#[cw_serde]
pub struct PartnerFeesCollectedResponse {
    pub total: DenomFees,
}

#[cw_serde]
pub struct PartnerFeesCollectedPerDenomResponse {
    pub total: Uint128,
}

#[cw_serde]
pub struct AllPoolsResponse {
    pub pools: Vec<PoolVlpResponse>, // Assuming pool addresses are strings
}
#[cw_serde]
pub struct PoolVlpResponse {
    pub pair: Pair,
    pub vlp: String,
}

#[cw_serde]
pub struct MigrateMsg {}

#[cw_serde]
pub struct RegisterFactoryResponse {
    pub factory_address: String,
    pub chain_id: String,
}

#[cw_serde]
pub struct ReleaseEscrowResponse {
    pub factory_address: String,
    pub chain_id: String,
    pub amount: Uint128,
    pub token: Token,
    pub to_address: String,
}

#[cw_serde]
pub struct GetPendingSwapsResponse {
    pub pending_swaps: Vec<SwapRequest>,
}
#[cw_serde]
pub struct GetPendingLiquidityResponse {
    pub pending_add_liquidity: Vec<AddLiquidityRequest>,
}

#[cw_serde]
pub struct GetPendingRemoveLiquidityResponse {
    pub pending_remove_liquidity: Vec<RemoveLiquidityRequest>,
}

#[cw_serde]
pub struct AllTokensResponse {
    pub tokens: Vec<Token>, // Assuming pool addresses are strings
}
