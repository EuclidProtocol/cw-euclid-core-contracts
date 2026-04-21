use cosmwasm_std::Empty;
use cw_orch::{interface, prelude::CwEnv};
use euclid::msgs::lp_token::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
pub const CONTRACT_ID: &str = "lp_token_contract";

#[interface(InstantiateMsg, ExecuteMsg, QueryMsg, Empty, id = CONTRACT_ID)]
pub struct LpTokenContract<Chain: CwEnv>;
