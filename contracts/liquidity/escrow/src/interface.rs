use cw_orch::{interface, prelude::CwEnv};
use euclid::msgs::escrow::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
pub const CONTRACT_ID: &str = "escrow_contract";

#[interface(InstantiateMsg, ExecuteMsg, QueryMsg, MigrateMsg, id = CONTRACT_ID)]
pub struct EscrowContract<Chain: CwEnv>;
