use cw_orch::{interface, prelude::CwEnv};
use euclid::msgs::claimer::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
pub const CONTRACT_ID: &str = "claimer_contract";

#[interface(InstantiateMsg, ExecuteMsg, QueryMsg, MigrateMsg, id = CONTRACT_ID)]
pub struct ClaimerContract<Chain: CwEnv>;
