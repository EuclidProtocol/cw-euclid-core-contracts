use cw_orch::{interface, prelude::CwEnv};
use euclid::msgs::meta_transaction::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
pub const CONTRACT_ID: &str = "meta_transaction_contract";

#[interface(InstantiateMsg, ExecuteMsg, QueryMsg, MigrateMsg, id = CONTRACT_ID)]
pub struct MetaTransactionContract<Chain: CwEnv>;
