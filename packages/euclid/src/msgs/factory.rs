use crate::{
    chain::{ChainUid, CrossChainUser},
    liquidity::{AddLiquidityRequest, RemoveLiquidityRequest},
    swap::{NextSwapPair, SwapRequest},
    token::{Pair, PairWithDenom, Token, TokenWithDenom},
};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Decimal, IbcPacketAckMsg, IbcPacketReceiveMsg, Uint128};
use cw20::Cw20ReceiveMsg;

#[cw_serde]
pub struct InstantiateMsg {
    // Router contract on VLP
    pub router_contract: String,
    pub chain_uid: ChainUid,
    pub escrow_code_id: u64,
}

#[cw_serde]
pub enum ExecuteMsg {
    UpdateHubChannel {
        new_channel: String,
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
        tx_id: String,
    },
    AddLiquidityRequest {
        pair_info: PairWithDenom,
        token_1_liquidity: Uint128,
        token_2_liquidity: Uint128,
        slippage_tolerance: u64,
        timeout: Option<u64>,
        tx_id: String,
    },
    RemoveLiquidityRequest {
        pair: Pair,
        lp_allocation: Uint128,
        timeout: Option<u64>,
        // First element in array has highest priority
        cross_chain_addresses: Vec<CrossChainUser>,
        tx_id: String,
    },
    ExecuteSwapRequest {
        asset_in: TokenWithDenom,
        asset_out: Token,
        amount_in: Uint128,
        min_amount_out: Uint128,
        timeout: Option<u64>,
        swaps: Vec<NextSwapPair>,
        // First element in array has highest priority
        cross_chain_addresses: Vec<CrossChainUser>,
        tx_id: String,

        partner_fee: Option<Decimal>,
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
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(GetVlpResponse)]
    GetVlp { pair: Pair },
    #[returns(StateResponse)]
    GetState {},
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
        lower_limit: Option<u128>,
        upper_limit: Option<u128>,
    },
    #[returns(GetPendingLiquidityResponse)]
    PendingLiquidity {
        user: Addr,
        lower_limit: Option<u128>,
        upper_limit: Option<u128>,
    },
    #[returns(GetPendingLiquidityResponse)]
    PendingRemoveLiquidity {
        user: Addr,
        lower_limit: Option<u128>,
        upper_limit: Option<u128>,
    },

    #[returns(GetEscrowResponse)]
    GetEscrow { token_id: String },
}

#[cw_serde]
pub struct GetVlpResponse {
    pub vlp_address: String,
}

#[cw_serde]
pub struct GetEscrowResponse {
    pub escrow_address: Option<Addr>,
}
// We define a custom struct for each query response
#[cw_serde]
pub struct StateResponse {
    pub chain_uid: ChainUid,
    pub router_contract: String,
    pub hub_channel: Option<String>,
    pub admin: String,
    // pub pool_code_id: u64,
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
