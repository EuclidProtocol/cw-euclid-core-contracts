use cw_orch::{interface, prelude::CwEnv};
use euclid::msgs::router::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
pub const CONTRACT_ID: &str = "router_contract";

#[interface(InstantiateMsg, ExecuteMsg, QueryMsg, MigrateMsg, id = CONTRACT_ID)]
pub struct RouterContract<Chain: CwEnv>;
