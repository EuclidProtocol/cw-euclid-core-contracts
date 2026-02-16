use cosmwasm_schema::cw_serde;
use cosmwasm_std::Binary;

#[cw_serde]
pub struct InstantiateMsg {
    pub virtual_balance: String,
    pub admin: Option<String>,
    pub root_challenge_period: Option<u64>,
    pub permit_signer_pubkey: Option<Binary>,
    pub permit_signer_address: Option<String>,
    pub authorized_posters: Option<Vec<String>>,
}
