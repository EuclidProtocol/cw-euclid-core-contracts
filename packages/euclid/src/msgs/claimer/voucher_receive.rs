use cosmwasm_schema::cw_serde;
use cosmwasm_std::Binary;

#[cw_serde]
pub enum VoucherReceiveHookMsg {
    CreateVoucherClaim(CreateVoucherClaim),
}

#[cw_serde]
pub struct CreateVoucherClaim {
    pub claimer_pubkey: Binary,
    // Adds a pseudo claim id used by indexers
    pub pseudo_claim_id: Option<String>,
    // Group id used by indexers and on chain search using unique identifier
    pub claim_group_id: Option<String>,
}
