use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Coin, Decimal, Uint128};
use cw20::Cw20ReceiveMsg;
use euclid::{msgs::hook::EuclidReceive, token::TokenType};
use osmosis_std::types::osmosis::poolmanager::v1beta1::SwapAmountInRoute;

#[cw_serde]
pub struct InstantiateMsg {
    pub osmo_router: Addr,
}

#[cw_serde]
pub enum Slippage {
    Twap {
        window_seconds: Option<u64>,
        slippage_percentage: Decimal,
    },
    MinOutputAmount(Uint128),
}

#[cw_serde]
#[derive(cw_orch::ExecuteFns)]
pub enum ExecuteMsg {
    Receive(Cw20ReceiveMsg),
    EuclidReceive(EuclidReceive),
    Swap(SwapMsg),
}

#[cw_serde]
pub enum OsmosisExecuteMsg {
    /// The contract's owner determines how can update the routes. This method
    /// allows the owner to be transfered to someone else.
    TransferOwnership { new_owner: String },
    SetRoute {
        input_denom: String,
        output_denom: String,
        pool_route: Vec<SwapAmountInRoute>,
    },
    Swap {
        input_coin: Coin,
        output_denom: String,
        slippage: Slippage,
        route: Option<Vec<SwapAmountInRoute>>,
    },
}

#[cw_serde]
#[derive(cw_orch::QueryFns, QueryResponses)]
pub enum QueryMsg {}

#[cw_serde]
pub struct SwapMsg {
    pub slippage: Slippage,
    pub route: Option<Vec<SwapAmountInRoute>>,
    pub forwarding_msg: Option<EuclidReceive>,
    pub to_token: TokenType,
    pub recipient: String,
}
