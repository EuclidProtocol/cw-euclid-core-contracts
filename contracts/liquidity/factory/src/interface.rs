use cw_orch::{interface, prelude::CwEnv};
use euclid::msgs::factory::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
pub const CONTRACT_ID: &str = "factory_contract";

#[interface(InstantiateMsg, ExecuteMsg, QueryMsg, MigrateMsg, id = CONTRACT_ID)]
pub struct FactoryContract<Chain: CwEnv>;
