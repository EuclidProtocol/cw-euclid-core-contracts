use super::virtual_balance::VBalanceMigrateMsg;
use cosmwasm_schema::{cw_serde, QueryResponses};

#[cw_serde]
pub struct InstantiateMsg {
    pub router: String,
    pub virtual_balance: String,
    pub admin: String,
}

#[cw_serde]
pub enum ExecuteMsg {
    MigrateVBalance {
        vbalance_address: String,
        channel_id: String,
        timeout: Option<u64>,
    },
    UpdateState {
        router: Option<String>,
        virtual_balance: Option<String>,
        admin: Option<String>,
    },
}

#[cw_serde]
pub enum IbcExecuteMsg {
    MigrateVBalance {
        vbalance_address: String,
        migrate_msg: VBalanceMigrateMsg,
    },
}

#[cw_serde]
#[derive(QueryResponses)]

pub enum QueryMsg {
    #[returns(GetStateResponse)]
    State {},
}

#[cw_serde]
pub struct GetStateResponse {
    pub router: String,
    pub virtual_balance: String,
    pub admin: String,
}
