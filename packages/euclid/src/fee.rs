use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;

use crate::chain::CrossChainUser;

// Set maximum fee as 10%
pub const MAX_FEE_BPS: u64 = 1000;
// Fee Config for a VLP contract
#[cw_serde]
pub struct Fee {
    // Fee for lp providers
    pub lp_fee_bps: u64,
    // Fee for euclid treasury, distributed among stakers and other euclid related rewards
    pub euclid_fee_bps: u64,
    // Recipient for the fee
    pub recipient: CrossChainUser,
}

#[cw_serde]
pub struct TotalFees {
    // Fee for lp providers
    pub lp_fees: Uint128,
    // Fee for euclid treasury, distributed among stakers and other euclid related rewards
    pub euclid_fees: Uint128,
}

// Set maximum fee as 0.3%
pub const MAX_PARTNER_FEE_BPS: u64 = 100;

// Fee Config for a VLP contract
#[cw_serde]
pub struct PartnerFee {
    // The percentage of the fee for platform - 0 to 1
    pub partner_fee_bps: u64,
    pub recipient: String,
}
