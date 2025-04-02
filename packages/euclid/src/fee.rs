use std::collections::HashMap;

use crate::chain::CrossChainUser;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;

pub const BPS_100_PERCENT: u64 = 10000;
pub const BPS_50_PERCENT: u64 = 5000;
pub const BPS_20_PERCENT: u64 = 2000;
pub const BPS_10_PERCENT: u64 = 1000;
pub const BPS_1_PERCENT: u64 = 100;
pub const BPS_0_5_PERCENT: u64 = 50;

// Set maximum fee as 10%
pub const MAX_FEE_BPS: u64 = BPS_10_PERCENT;
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

impl Fee {
    pub fn new(lp_fee_bps: u64, euclid_fee_bps: u64, recipient: CrossChainUser) -> Self {
        Self {
            lp_fee_bps,
            euclid_fee_bps,
            recipient,
        }
    }
}

#[cw_serde]
pub struct TotalFees {
    // Fee for lp providers
    pub lp_fees: DenomFees,
    // Fee for euclid treasury, distributed among stakers and other euclid related rewards
    pub euclid_fees: DenomFees,
}

#[cw_serde]
pub struct DenomFees {
    // A map to store the total fees per denomination
    pub totals: HashMap<String, Uint128>,
}

impl DenomFees {
    // Create a new DenomFees instance with optional initial values
    pub fn new(initial_totals: Option<HashMap<String, Uint128>>) -> Self {
        Self {
            totals: initial_totals.unwrap_or_default(),
        }
    }

    // Add or update the total for a given denomination
    pub fn add_fee(&mut self, token: String, amount: Uint128) {
        self.totals
            .entry(token)
            .and_modify(|total| *total += amount)
            .or_insert(amount);
    }
    // Get the total for a given denomination
    pub fn get_fee(&self, token: &str) -> Uint128 {
        self.totals.get(token).cloned().unwrap_or_default()
    }
}
// Set maximum fee as 0.3%
pub const MAX_PARTNER_FEE_BPS: u64 = 30;

// Fee Config for a VLP contract
#[cw_serde]
pub struct PartnerFee {
    // The percentage of the fee for platform - 0 to 1
    pub partner_fee_bps: u64,
    pub recipient: String,
}
