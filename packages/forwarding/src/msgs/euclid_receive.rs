use cosmwasm_schema::cw_serde;

use super::astroport::SwapMsg;
use super::osmosis::SwapMsg as OsmosisSwapMsg;

#[cw_serde]
pub enum AstroportEuclidReceiveHook {
    Swap(SwapMsg),
}

#[cw_serde]
pub enum OsmosisEuclidReceiveHook {
    Swap(OsmosisSwapMsg),
}
