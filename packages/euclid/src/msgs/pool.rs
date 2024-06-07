use crate::{
    liquidity::LiquidityTxInfo,
    pool::{LiquidityResponse, Pool},
    swap::{SwapInfo, SwapResponse},
    token::{PairInfo, TokenInfo},
};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;
use cw20::Cw20ReceiveMsg;

#[cw_serde]
pub struct InstantiateMsg {
    pub vlp_contract: String,
    pub pool: Pool,
    pub chain_id: String,
}

#[cw_serde]
pub enum ExecuteMsg {
    // Add Liquidity Request to the VLP
    AddLiquidity {
        token_1_liquidity: Uint128,
        token_2_liquidity: Uint128,
        slippage_tolerance: u64,
        timeout: Option<u64>,
    },
    ExecuteSwap {
        asset: TokenInfo,
        asset_amount: Uint128,
        min_amount_out: Uint128,
        timeout: Option<u64>,
    },

    // Recieve CW20 TOKENS structure
    Receive(Cw20ReceiveMsg),

    Callback(CallbackExecuteMsg),
}

#[cw_serde]
pub enum CallbackExecuteMsg {
    CompleteSwap {
        swap_response: SwapResponse,
    },
    RejectSwap {
        swap_id: String,
        error: Option<String>,
    },

    // Add Liquidity Request to the VLP
    CompleteAddLiquidity {
        liquidity_response: LiquidityResponse,
        liquidity_id: String,
    },
    // Add Liquidity Request to the VLP
    RejectAddLiquidity {
        liquidity_id: String,
        error: Option<String>,
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(GetPairInfoResponse)]
    PairInfo {},
    #[returns(GetVLPResponse)]
    GetVlp {},
    // Fetch pending swaps with pagination for a user
    #[returns(GetPendingSwapsResponse)]
    PendingSwapsUser {
        user: String,
        lower_limit: Option<u128>,
        upper_limit: Option<u128>,
    },
    #[returns(GetPendingLiquidityResponse)]
    PendingLiquidity {
        user: String,
        lower_limit: Option<u128>,
        upper_limit: Option<u128>,
    },

    #[returns(GetPoolReservesResponse)]
    PoolReserves {},
}

// CW20 Hook Msg
#[cw_serde]
pub enum Cw20HookMsg {
    Deposit {},
    Swap {
        asset: TokenInfo,
        min_amount_out: Uint128,
        timeout: Option<u64>,
    },
}

#[cw_serde]
pub struct GetPairInfoResponse {
    pub pair_info: PairInfo,
}

#[cw_serde]
pub struct GetVLPResponse {
    pub vlp: String,
}

#[cw_serde]
pub struct GetPendingSwapsResponse {
    pub pending_swaps: Vec<SwapInfo>,
}
#[cw_serde]
pub struct GetPendingLiquidityResponse {
    pub pending_liquidity: Vec<LiquidityTxInfo>,
}

#[cw_serde]
pub struct GetPoolReservesResponse {
    pub reserve_1: Uint128,
    pub reserve_2: Uint128,
}

#[cw_serde]
pub struct MigrateMsg {}
