use cosmwasm_schema::cw_serde;

use super::astroport::SwapMsg;

#[cw_serde]
pub enum AstroportEuclidReceiveHook {
    Swap(SwapMsg),
}
