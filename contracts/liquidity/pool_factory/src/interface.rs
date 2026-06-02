use crate::contract::{execute, instantiate, query};
use cw_orch::{interface, prelude::*};
use euclid::msgs::pool_factory::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};

pub const CONTRACT_ID: &str = "pool_factory_contract";

#[interface(InstantiateMsg, ExecuteMsg, QueryMsg, MigrateMsg, id = CONTRACT_ID)]
pub struct PoolFactoryContract<Chain: CwEnv>;

impl<Chain> Uploadable for PoolFactoryContract<Chain> {
    fn wrapper() -> Box<dyn MockContract<Empty>> {
        Box::new(
            ContractWrapper::new_with_empty(execute, instantiate, query)
                .with_reply(crate::contract::reply),
        )
    }
}
