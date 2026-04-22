use cw_orch::{interface, prelude::CwEnv};
use euclid::msgs::virtual_balance::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
pub const CONTRACT_ID: &str = "virtual_balance_contract";

#[interface(InstantiateMsg, ExecuteMsg, QueryMsg, MigrateMsg, id = CONTRACT_ID)]
pub struct VirtualBalanceContract<Chain: CwEnv>;
