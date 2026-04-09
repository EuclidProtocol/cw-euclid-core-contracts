use cosmwasm_schema::cw_serde;
use cosmwasm_std::{to_json_binary, Addr, CosmosMsg, StdResult, WasmMsg};

use crate::msgs::position_token::MintMsg;

#[cw_serde]
pub struct InstantiateMsg {
    pub name: String,
    pub symbol: String,
    pub vlp_address: String,
    pub mint_msg: Option<MintMsg>,
}

#[cw_serde]
pub struct InstantiateResponse {
    pub position_token_address: Addr,
    pub vlp_address: String,
}

impl InstantiateMsg {
    pub fn to_msg(&self, code_id: u64, admin: Option<String>) -> StdResult<CosmosMsg> {
        Ok(WasmMsg::Instantiate {
            admin,
            code_id,
            label: "position_token".to_string(),
            msg: to_json_binary(&self)?,
            funds: vec![],
        }
        .into())
    }
}
