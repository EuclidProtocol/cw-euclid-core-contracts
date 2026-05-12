use crate::{
    cross_chain_user::CrossChainUser,
    fee::{Fee, TotalFees},
    swap::NextSwapVlp,
    token::{Pair, PairWithAmount, Token},
};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint256, Uint64};

pub const NEXT_SWAP_REPLY_ID: u64 = 2;

#[cw_serde]
pub struct State {
    // Token Pair Info
    pub pair: Pair,
    // Router Contract
    pub router: Addr,
    // Virtual Coin Contract
    pub virtual_balance_contract: Addr,
    // Fee per swap for each transaction
    pub fee: Fee,
    // Total lp and euclid fees collected
    pub total_fees_collected: TotalFees,
    // The last timestamp where the balances for each token have been updated
    pub last_updated: u64,
    // total number of LP tokens issued
    pub total_lp_tokens: Uint256,
}

#[cw_serde]
pub struct VlpSwapMsg {
    pub sender: CrossChainUser,
    pub tx_id: String,
    pub asset_in: Token,
    pub amount_in: Uint256,
    pub min_token_out: Uint256,
    pub next_swaps: Vec<NextSwapVlp>,
    pub test_fail: Option<bool>,
}

#[cw_serde]
pub struct VlpAddLiquidityMsg {
    pub sender: CrossChainUser,
    pub tx_id: String,
    pub liquidity: PairWithAmount,
    pub slippage_tolerance_bps: u64,
}

#[cw_serde]
pub struct VlpRemoveLiquidityMsg {
    pub sender: CrossChainUser,
    pub tx_id: String,
    pub lp_allocation: Uint256,
}

#[cw_serde]
pub struct VlpRegisterPoolMsg {
    pub sender: CrossChainUser,
    pub pair: Pair,
    pub tx_id: String,
}

#[cw_serde]
pub struct VlpSimulateSwapMsg {
    pub asset: Token,
    pub asset_amount: Uint256,
    pub swaps: Vec<NextSwapVlp>,
}

#[cw_serde]
pub struct GetLiquidityQueryResponse {
    pub pair: Pair,
    pub token_1_reserve: Uint256,
    pub token_2_reserve: Uint256,
    pub total_lp_tokens: Uint256,
}

#[cw_serde]
pub struct GetSwapQueryResponse {
    pub amount_out: Uint256,
    pub asset_out: Token,
    pub spread_amount: Uint256,
    pub lp_fee: Uint256,
    pub euclid_fee: Uint256,
}

#[cw_serde]
pub struct PoolCreationResponse {
    pub vlp_contract: String,
    pub tx_id: String,
    pub mint_lp_tokens: Uint256,
    pub sender: CrossChainUser,
}

#[cw_serde]
pub struct VlpSwapResponse {
    pub sender: CrossChainUser,
    pub tx_id: String,
    pub asset_out: Token,
    pub amount_out: Uint256,
}

#[cw_serde]
pub struct VlpAddLiquidityResponse {
    pub liquidity_added: PairWithAmount,
    pub mint_lp_tokens: Uint256,
    pub tx_id: String,
    pub sender: CrossChainUser,
    pub vlp_address: String,
}

#[cw_serde]
pub struct VlpRemoveLiquidityResponse {
    pub liquidity_released: PairWithAmount,
    pub burn_lp_tokens: Uint256,
    pub tx_id: String,
    pub sender: CrossChainUser,
    pub vlp_address: String,
}

#[cw_serde]
pub struct RegisterDenomResponse {}

#[cw_serde]
pub struct DeregisterDenomResponse {}

#[cw_serde]
pub enum PoolConfig {
    Stable { amp_factor: Option<Uint64> },
    ConstantProduct {},
}

#[cw_serde]
pub enum QueryMsg {
    Liquidity {},
    SimulateSwap(VlpSimulateSwapMsg),
}

#[cw_serde]
pub enum ExecuteMsg {
    RegisterPool(VlpRegisterPoolMsg),
    AddLiquidity(VlpAddLiquidityMsg),
    RemoveLiquidity(VlpRemoveLiquidityMsg),
    Swap(VlpSwapMsg),
}
