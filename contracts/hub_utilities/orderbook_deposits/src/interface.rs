use cw_orch::{interface, prelude::CwEnv};
use euclid::msgs::orderbook_deposits::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
pub const CONTRACT_ID: &str = "orderbook_deposits_contract";

#[interface(InstantiateMsg, ExecuteMsg, QueryMsg, MigrateMsg, id = CONTRACT_ID)]
pub struct OrderbookDepositsContract<Chain: CwEnv>;
