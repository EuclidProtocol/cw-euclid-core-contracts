use cosmwasm_schema::cw_serde;

use crate::chain::CrossChainUser;

// Fee Config for a VLP contract
#[cw_serde]
pub struct Fee {
    // The percentage of the fee for LP providers - 0 to 1
    pub lp_fee_bps: u64,
    // The percentage of the fee for euclid - 0 to 1
    pub euclid_fee_bps: u64,

    pub recipient: CrossChainUser,
}

pub const MAX_PARTNER_FEE_BPS: u64 = 3000;

// Fee Config for a VLP contract
#[cw_serde]
pub struct PartnerFee {
    // The percentage of the fee for platform - 0 to 1
    pub partner_fee_bps: u64,
    pub recipient: String,
}
