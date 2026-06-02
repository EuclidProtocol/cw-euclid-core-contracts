use cosmwasm_schema::cw_serde;
use cosmwasm_std::Addr;

#[cw_serde]
pub struct InstantiateMsg {
    pub constant_product_vlp_code_id: u64,
    pub stable_vlp_code_id: u64,
    pub concentrated_vlp_code_id: u64,

    pub virtual_balance_code_id: u64,
    pub relayer_contract: Addr,

    pub release_fee_recipient: Addr,
    pub default_fee_recipient: Addr,
}
