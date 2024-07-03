use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Uint128};

use crate::{chain::ChainUid, swap::NextSwap, token::Token};
#[cw_serde]
pub struct InstantiateMsg {
    // Pool Code ID
    pub vlp_code_id: u64,
    pub vcoin_code_id: u64,
}

#[cw_serde]
pub enum ExecuteMsg {
    // Update Pool Code ID
    UpdateVLPCodeId {
        new_vlp_code_id: u64,
    },
    RegisterFactory {
        chain_uid: ChainUid,
        channel: String,
        timeout: Option<u64>,
        tx_id: String,
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(StateResponse)]
    GetState {},
    #[returns(ChainResponse)]
    GetChain { chain_id: String },
    #[returns(AllChainResponse)]
    GetAllChains {},
    #[returns(VlpResponse)]
    GetVlp { token_1: Token, token_2: Token },
    #[returns(AllVlpResponse)]
    GetAllVlps {},
    #[returns(SimulateSwapResponse)]
    SimulateSwap(QuerySimulateSwap),
}
// We define a custom struct for each query response
#[cw_serde]
pub struct MigrateMsg {}

#[cw_serde]
pub struct QuerySimulateSwap {
    pub factory_chain: String,
    pub to_address: String,
    pub to_chain_id: String,
    pub asset_in: Token,
    pub amount_in: Uint128,
    pub min_amount_out: Uint128,
    pub swaps: Vec<NextSwap>,
}

#[cw_serde]
pub struct Chain {
    pub factory_chain_id: String,
    pub factory: String,
    pub from_hub_channel: String,
    pub from_factory_channel: String,
}
#[cw_serde]
pub struct StateResponse {
    pub admin: String,
    pub vlp_code_id: u64,
    pub vcoin_address: Option<Addr>,
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
}

#[cw_serde]
pub struct AllChainResponse {
    pub chains: Vec<ChainResponse>,
}

#[cw_serde]
pub struct SimulateSwapResponse {
    pub amount_out: Uint128,
    pub asset_out: Token,
    pub out_chains: Vec<SwapOutChain>,
}

#[cw_serde]
pub struct SwapOutChain {
    pub chain: Chain,
    pub amount: Uint128,
}
