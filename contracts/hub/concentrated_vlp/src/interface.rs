use crate::contract::{execute, instantiate, query};
use cw_orch::{interface, prelude::*};
use euclid::msgs::concentrated_vlp::QueryMsg;
use euclid::msgs::concentrated_vlp::{ExecuteMsg, InstantiateMsg, MigrateMsg};
pub const CONTRACT_ID: &str = "concentrated_vlp_contract";

#[interface(InstantiateMsg, ExecuteMsg, QueryMsg, MigrateMsg, id = CONTRACT_ID)]
pub struct ConcentratedVlpContract<Chain: CwEnv>;

// Implement the Uploadable trait so it can be uploaded to the mock.
impl<Chain> Uploadable for ConcentratedVlpContract<Chain> {
    fn wrapper() -> Box<dyn MockContract<Empty>> {
        Box::new(
            ContractWrapper::new_with_empty(execute, instantiate, query)
                .with_reply(crate::contract::reply),
        )
    }
}
