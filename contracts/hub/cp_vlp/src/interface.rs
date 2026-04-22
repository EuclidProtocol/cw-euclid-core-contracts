use cw_orch::{interface, prelude::CwEnv};
use euclid::msgs::vlp::cp::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
pub const CONTRACT_ID: &str = "vlp_contract";

#[interface(InstantiateMsg, ExecuteMsg, QueryMsg, MigrateMsg, id = CONTRACT_ID)]
pub struct VlpContract<Chain: CwEnv>;
