use cosmwasm_schema::cw_serde;

#[cw_serde]
pub struct MigrateMsg {
    pub concentrated_vlp_code_id: Option<u64>,
}
