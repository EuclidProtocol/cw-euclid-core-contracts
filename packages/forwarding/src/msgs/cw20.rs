use cosmwasm_schema::cw_serde;
use euclid::msgs::hook::EuclidReceive;

use super::astroport::SwapMsg;

#[cw_serde]
pub enum Cw20HookMsg {
    EuclidReceive(EuclidReceive),
    Swap(SwapMsg),
}
