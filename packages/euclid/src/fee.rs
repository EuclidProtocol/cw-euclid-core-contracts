use cosmwasm_schema::cw_serde;

// Fee Config for a VLP contract
#[cw_serde]
pub struct Fee {
    // The percentage of the fee for LP providers
    pub lp_fee: u64,
    // The percentage of the fee for the treasury
    pub treasury_fee: u64,
    // The percentage of the fee for the stakers
    pub staker_fee: u64,
}
