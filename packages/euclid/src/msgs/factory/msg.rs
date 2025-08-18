use crate::{
    chain::{ChainUid, CrossChainUser, CrossChainUserWithLimit},
    fee::{DenomFees, PartnerFee},
    liquidity::{AddLiquidityRequest, RemoveLiquidityRequest},
    msgs::hook::EuclidReceive,
    pool::PoolConfig,
    swap::{NextSwapPair, SwapRequest},
    token::{Pair, PairWithDenomAndAmount, Token, TokenType, TokenWithDenom},
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
    pub mock_relayer_address: Option<String>,
}

#[cw_serde]
#[cfg_attr(not(target_arch = "wasm32"), derive(cw_orch::ExecuteFns))]
pub enum ExecuteMsg {
    AddLiquidityRequest {
        pair_info: PairWithDenomAndAmount,
        slippage_tolerance_bps: u64,
        timeout: Option<u64>,
    },
    ExecuteSwapRequest(ExecuteSwapRequest),
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
        pool_config: PoolConfig,
        slippage_tolerance_bps: u64,
        timeout: Option<u64>,
        lp_token_name: String,
        lp_token_symbol: String,
        lp_token_decimal: u8,
        lp_token_marketing: Option<cw20_base::msg::InstantiateMarketingInfo>,
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
        // If user has approval for transfer, they can set the address to transfer from (Behaves like cw20 allowance)
        from: Option<CrossChainUser>,
        // Msg that we want to trigger with transfer, behaves like cw20 send
        msg: Option<Binary>,
        timeout: Option<u64>,
    },
    DepositToken {
        asset_in: TokenWithDenom,
        amount_in: Uint128,
        timeout: Option<u64>,
        recipient: Option<CrossChainUser>,
        msg: Option<Binary>,
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
        mock_relayer_address: Option<String>,
    },
    // Recieve CW20 TOKENS structure
    Receive(Cw20ReceiveMsg),

    EuclidReceive(EuclidReceive),

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

    // COSMOS RELAYER ENTRY POINTS
    CosmosSendPacket {
        msg: Binary,
    },

    CosmosReceivePacket {
        msg: Binary,
        // Store sequence of packet relayed so we don't relay same sequence again
        sequence: u128,
        // Continous hash of the packet to make sure its linked to the same source flow
        hash: String,
    },

    CosmosReceivePacketInternalCallback {
        msg: Binary,
    },

    CosmosReceiveAck {
        msg: Binary,
        // Store sequence of packet relayed so we don't relay same sequence again
        sequence: u128,
        // Continous hash of the packet to make sure its linked to the same source flow
        hash: String,
        ack: Binary,
    },
}

#[cw_serde]
pub struct ExecuteSwapRequest {
    pub sender: Option<CrossChainUser>,
    pub asset_in: TokenWithDenom,
    pub amount_in: Uint128,
    pub asset_out: Token,
    pub min_amount_out: Uint128,
    pub timeout: Option<u64>,
    pub swaps: Vec<NextSwapPair>,
    // First element in array has highest priority
    pub cross_chain_addresses: Vec<CrossChainUserWithLimit>,
    pub partner_fee: Option<PartnerFee>,
    pub meta: Option<String>,
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

    #[returns(GetRelayerResponse)]
    GetRelayer {},
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
    pub amount: Uint128,
    pub new_balance: Uint128,
}

#[cw_serde]
pub struct ReleaseEscrowResponse {
    pub factory_address: String,
    pub chain_id: String,
    pub amount: Uint128,
    pub token: Token,
    pub to_address: String,
    pub denoms: Vec<ReleaseEscrowDenomsResponse>,
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

#[cw_serde]
pub struct GetRelayerResponse {
    pub relayer_address: String,
}
