use super::common_old::{EuclidReceive, TokenType};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary};
use cw20::Cw20ReceiveMsg;
use osmosis_std::types::osmosis::poolmanager::v1beta1::SwapAmountInRoute;
use swaprouter::msg::Slippage as OsmosisSlippage;

#[cw_serde]
pub struct InstantiateMsg {
    pub osmo_router: Addr,
}

#[cw_serde]
pub enum ExecuteMsg {
    Receive(Cw20ReceiveMsg),
    EuclidReceive(EuclidReceive),
    Swap(SwapMsg),
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {}

#[cw_serde]
pub struct SwapMsg {
    pub slippage: OsmosisSlippage,
    pub route: Vec<SwapRoute>,
    pub forwarding_msg: Option<Binary>,
    pub to_token: TokenType,
    pub recipient: String,
}

#[cw_serde]
pub struct SwapRoute {
    pub pool_id: u64,
    pub token_out_denom: String,
}

impl From<SwapRoute> for SwapAmountInRoute {
    fn from(route: SwapRoute) -> Self {
        Self {
            pool_id: route.pool_id,
            token_out_denom: route.token_out_denom,
        }
    }
}

#[cw_serde]
pub struct MigrateMsg {}
