use crate::{
    admin::{AdminType, EuclidAdmin},
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    fee::{DenomFees, PartnerFee},
    liquidity::{AddLiquidityRequest, RemoveLiquidityRequest, SingleSidedLiquidityRequest},
    msgs::{cross_chain_config::CrossChainConfig, hook::EuclidReceive, vlp::base::PoolConfig},
    recipient::Recipient,
    swap::{NextSwapPair, SwapRequest},
    token::{Pair, PairWithDenomAndAmount, Token, TokenType, TokenWithDenom},
    utils::pagination::Pagination,
};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary, Uint128, Uint256};
use cw20::Cw20ReceiveMsg;
#[cw_serde]
pub struct InstantiateMsg {
    // Router contract on VLP
    pub router_contract: String,
    pub chain_uid: ChainUid,
    pub escrow_code_id: u64,
    pub lp_code_id: u64,
    pub is_native: bool,
    pub relayer_contract: Addr,
    pub rate_limit_fee_recipient: Addr,
    pub rate_limit_fee_denom: String,
    pub rate_limit_free_limit: Uint256,
}

#[cw_serde]
#[cfg_attr(not(target_arch = "wasm32"), derive(cw_orch::ExecuteFns))]
pub enum ExecuteMsg {
    ManageFactoryState(ManageFactoryState),
    RegisterDenom {
        token_with_denom: TokenWithDenom,
        cross_chain_config: CrossChainConfig,
    },
    DeregisterDenom {
        token_with_denom: TokenWithDenom,
        cross_chain_config: CrossChainConfig,
    },
    #[cfg_attr(not(target_arch = "wasm32"), cw_orch(payable))]
    DepositToken {
        asset_in: TokenWithDenom,
        amount_in: Uint256,
        recipients: Vec<Recipient>,
        cross_chain_config: CrossChainConfig,
    },
    TransferVoucher {
        token_id: Token,
        amount: Uint256,
        // If user has approval for transfer, they can set the address to transfer from (Behaves like cw20 allowance)
        from: Option<CrossChainUser>,
        recipients: Vec<Recipient>,
        cross_chain_config: CrossChainConfig,
    },
    #[cfg_attr(not(target_arch = "wasm32"), cw_orch(payable))]
    RequestPoolCreation {
        pair_with_denom_and_amount: PairWithDenomAndAmount,
        pool_config: PoolConfig,
        lp_token_name: String,
        lp_token_symbol: String,
        lp_token_decimal: u8,
        slippage_tolerance_bps: u64,
        lp_token_marketing: Option<cw20_base::msg::InstantiateMarketingInfo>,
        cross_chain_config: CrossChainConfig,
    },
    AddLiquidity {
        pair_with_denom_and_amount: PairWithDenomAndAmount,
        slippage_tolerance_bps: u64,
        cross_chain_config: CrossChainConfig,
    },
    #[cfg_attr(not(target_arch = "wasm32"), cw_orch(payable))]
    AddSingleSidedLiquidity {
        asset_in: TokenWithDenom,
        amount_in: Uint256,
        pair: Pair,
        swap_amount: Uint256,
        swap_route: Vec<NextSwapPair>,
        min_lp_out: Uint256,
        partner_fee: Option<PartnerFee>,
        cross_chain_config: CrossChainConfig,
    },
    #[cfg_attr(not(target_arch = "wasm32"), cw_orch(payable))]
    ExecuteSwapRequest(ExecuteSwapRequest),

    // Recieve CW20 TOKENS structure
    Receive(Cw20ReceiveMsg),

    EuclidReceive(EuclidReceive),

    NativeReceiveCallback {
        msg: Binary,
    },

    SendPacket {
        msg: Binary,
        timeout: Option<u64>,
        ack_response: Option<Binary>,
        sender: Addr,
    },

    ReceivePacket {
        source_port: String,
        destination_port: String,
        msg: Binary,
        sequence: u128,
        timeout: u64,
    },

    ReceivePacketInternalCallback {
        msg: Binary,
        timeout: u64,
    },
    AcknowledgePacket {
        source_port: String,
        destination_port: String,
        msg: Binary,
        sequence: u128,
        ack: Binary,
    },
}

#[cw_serde]
pub enum ManageFactoryState {
    UpdateAdmin {
        admin: String,
        admin_type: AdminType,
    },
    UpdateEscrowCodeId {
        escrow_code_id: u64,
    },
    UpdateLPCodeId {
        lp_code_id: u64,
    },
    UpdateRelayerAddress {
        relayer_address: String,
    },
}

#[cw_serde]
pub struct ExecuteSwapRequest {
    pub asset_in: TokenWithDenom,
    pub amount_in: Uint256,
    pub asset_out: Token,
    pub min_amount_out: Uint256,
    pub swaps: Vec<NextSwapPair>,
    pub recipients: Vec<Recipient>,
    pub partner_fee: Option<PartnerFee>,
    pub cross_chain_config: CrossChainConfig,
}

#[cw_serde]
#[derive(cw_orch::QueryFns, QueryResponses)]
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
        pagination: Pagination<Uint256>,
    },
    #[returns(GetPendingLiquidityResponse)]
    PendingLiquidity {
        user: Addr,
        pagination: Pagination<Uint256>,
    },
    #[returns(GetPendingRemoveLiquidityResponse)]
    PendingRemoveLiquidity {
        user: Addr,
        pagination: Pagination<Uint256>,
    },
    #[returns(GetPendingSingleSidedLiquidityResponse)]
    PendingSingleSidedLiquidity {
        user: Addr,
        pagination: Pagination<Uint256>,
    },

    #[returns(GetEscrowResponse)]
    GetEscrow { token_id: String },

    #[returns(GetRateLimitStateResponse)]
    GetRateLimitState {},

    #[returns(GetUserRateLimitResponse)]
    GetUserRateLimit { user: Addr },
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
    pub relayer_contract: Addr,
    pub admin: EuclidAdmin,
    // Escrow Code ID
    pub escrow_code_id: u64,
    // CW20 Code ID
    pub lp_code_id: u64,
    pub is_native: bool,
}

#[cw_serde]
pub struct PartnerFeesCollectedResponse {
    pub total: DenomFees,
}

#[cw_serde]
pub struct PartnerFeesCollectedPerDenomResponse {
    pub total: Uint256,
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
pub struct MigrateMsg {
    pub mock_relayer_address: Option<String>,
}

#[cw_serde]
pub struct RegisterFactoryResponse {
    pub factory_address: String,
    pub chain_id: String,
}
#[cw_serde]
pub struct ReleaseEscrowDenomsResponse {
    pub token_type: TokenType,
    pub amount: Uint256,
    pub new_balance: Uint256,
}

#[cw_serde]
pub struct ReleaseEscrowResponse {
    pub amount: Uint256,
    pub to_address: String,
    pub escrow_balance: Uint256,
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
pub struct GetPendingSingleSidedLiquidityResponse {
    pub pending_single_sided_liquidity: Vec<SingleSidedLiquidityRequest>,
}

#[cw_serde]
pub struct AllTokensResponse {
    pub tokens: Vec<Token>, // Assuming pool addresses are strings
}

#[cw_serde]
pub struct FeeBracket {
    pub threshold: u128,
    pub fee: Uint256,
}

#[cw_serde]
pub struct GetRateLimitStateResponse {
    pub free_limit: u128,
    pub fee_brackets: Vec<FeeBracket>,
}

#[cw_serde]
pub struct GetUserRateLimitResponse {
    pub user: Addr,
    pub free_limit: Option<u128>,
    pub pending_packets: u128,
}
