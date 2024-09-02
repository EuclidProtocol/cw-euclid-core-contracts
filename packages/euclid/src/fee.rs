use std::collections::HashMap;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Deps, Uint128};

use crate::{chain::CrossChainUser, token::{TokenType, TokenWithDenom}};

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
    // Add or update the total for a given denomination
    pub fn add_fee(&mut self, deps: &Deps, denom: String, amount: Uint128) {
        let key = self.get_key(deps, &denom);

        self.totals
            .entry(key)
            .and_modify(|total| *total += amount)
            .or_insert(amount);
    }

    pub fn add_fee_token_with_denom(&mut self, token_with_denom: TokenWithDenom, amount: Uint128) {
       let key =  match token_with_denom.token_type {
            TokenType::Native { denom } => {
                format!("native{}", denom ) 
            }
            TokenType::Smart { contract_address } => {
                format!("smart{}", contract_address ) 
            }
        };
        self.totals
            .entry(key)
            .and_modify(|total| *total += amount)
            .or_insert(amount);
    }



    // Get the total for a given denomination
    pub fn get_fee(&self, denom: &str) -> Uint128 {
        self.totals.get(denom).cloned().unwrap_or_default()
    }

    pub fn get_key(&self, deps: &Deps, denom: &str) -> String {
        let denom_type = deps.api.addr_validate(denom);
        match denom_type {
            // If it's a valid address, it's a smart contract
            Ok(_) => format!("smart{}", denom ),
            // If it's not a valid address, it's a native token
            Err(_) => format!("native{}", denom ),
        }
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
