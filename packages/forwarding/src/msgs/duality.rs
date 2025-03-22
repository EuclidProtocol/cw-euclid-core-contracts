use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Uint128};
use cw20::Cw20ReceiveMsg;
use euclid::{msgs::hook::EuclidReceive, token::TokenType};
use neutron_std::types::neutron::dex::{MsgMultiHopSwap, MultiHopRoute};

#[cw_serde]
pub struct InstantiateMsg {
    pub duality_router: Addr,
}

#[cw_serde]
#[derive(cw_orch::ExecuteFns)]
pub enum ExecuteMsg {
    Receive(Cw20ReceiveMsg),
    EuclidReceive(EuclidReceive),
    Swap(SwapMsg),
}

#[cw_serde]
#[derive(cw_orch::QueryFns, QueryResponses)]
pub enum QueryMsg {}

#[cw_serde]
pub struct SwapMsg {
    pub recipient: String,
    pub forwarding_msg: Option<EuclidReceive>,
    pub to_token: TokenType,
    pub min_output_amount: Uint128,
}

#[cw_serde]
pub struct MigrateMsg {}
