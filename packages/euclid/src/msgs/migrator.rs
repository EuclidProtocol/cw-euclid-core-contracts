use crate::msgs::router::RouterMigrateMsg;

use super::{virtual_balance::VBalanceMigrateMsg, vlp::VlpMigrateMsg};
use cosmwasm_schema::{cw_serde, QueryResponses};

#[cw_serde]
pub struct InstantiateMsg {
    pub router: String,
    pub virtual_balance: String,
    pub vlp: String,
    pub admin: String,
}

#[cw_serde]
pub enum ExecuteMsg {
    MigrateVBalance {
        vbalance_address: String,
        router_address: String,
        channel_id: String,
        timeout: Option<u64>,
    },
    MigrateVLP {
        vbalance_address: String,
        router_address: String,
        vlp_address: String,
        channel_id: String,
        timeout: Option<u64>,
    },
    MigrateRouter {
        vbalance_address: String,
        router_address: String,
        vlp_address: String,
        channel_id: String,
        timeout: Option<u64>,
    },
    UpdateState {
        router: Option<String>,
        virtual_balance: Option<String>,
        vlp: Option<String>,
        admin: Option<String>,
    },
}

#[cw_serde]
pub enum IbcExecuteMsg {
    MigrateVBalance {
        vbalance_address: String,
        migrate_msg: VBalanceMigrateMsg,
    },
    MigrateVLP {
        vlp_address: String,
        migrate_msg: VlpMigrateMsg,
    },
    MigrateRouter {
        router_address: String,
        migrate_msg: RouterMigrateMsg,
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

#[macro_export]
macro_rules! generate_query_all {
    (
        $fn_name:ident,
        $map:ident,
        $response_ty:ident,
        $field_name:ident
    ) => {
        pub fn $fn_name(deps: Deps) -> Result<$response_ty, ContractError> {
            let keys = $map.keys(deps.storage, None, None, cosmwasm_std::Order::Ascending);
            let mut key_value = Vec::new();
            for key in keys {
                let key = key?;
                let value = $map.load(deps.storage, key.clone())?;
                key_value.push((key.clone(), value));
            }
            Ok($response_ty {
                $field_name: key_value,
            })
        }
    };
}
