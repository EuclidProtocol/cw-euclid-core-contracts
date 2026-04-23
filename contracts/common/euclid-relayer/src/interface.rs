use cw_orch::{interface, prelude::*};
use relayer::msgs::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
pub const CONTRACT_ID: &str = "relayer_contract";

#[interface(InstantiateMsg, ExecuteMsg, QueryMsg, MigrateMsg, id = CONTRACT_ID)]
pub struct RelayerContract<Chain: CwEnv>;
