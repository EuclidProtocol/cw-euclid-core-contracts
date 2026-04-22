use cw_orch::{interface, prelude::CwEnv};
use euclid::msgs::vlp::stable::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
pub const CONTRACT_ID: &str = "stable_vlp_contract";

#[interface(InstantiateMsg, ExecuteMsg, QueryMsg, MigrateMsg, id = CONTRACT_ID)]
pub struct StableVlpContract<Chain: CwEnv>;
