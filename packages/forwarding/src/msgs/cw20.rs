use cosmwasm_schema::cw_serde;
use euclid::msgs::hook::EuclidReceive;

use super::astroport::SwapMsg;
use super::osmosis::SwapMsg as OsmosisSwapMsg;

#[cw_serde]
pub enum Cw20HookMsg {
    EuclidReceive(EuclidReceive),
    Swap(SwapMsg),
}

#[cw_serde]
pub enum OsmosisCw20HookMsg {
    EuclidReceive(EuclidReceive),
    Swap(OsmosisSwapMsg),
}
