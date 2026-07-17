use crate::contract::{execute, instantiate, query};
use cw_orch::{interface, prelude::*};

use euclid::msgs::position_token::{ExecuteMsg, InstantiateMsg, QueryMsg};

pub const CONTRACT_ID: &str = "position_token_contract";

#[interface(InstantiateMsg, ExecuteMsg, QueryMsg, Empty, id = CONTRACT_ID)]
pub struct PositionTokenContract<Chain: CwEnv>;

impl<Chain> Uploadable for PositionTokenContract<Chain> {
    fn wrapper() -> Box<dyn MockContract<Empty>> {
        Box::new(ContractWrapper::new_with_empty(execute, instantiate, query))
    }
}
