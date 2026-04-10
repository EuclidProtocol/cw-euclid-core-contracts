use cosmwasm_schema::cw_serde;
use cosmwasm_std::{to_json_binary, CosmosMsg, StdResult, WasmMsg};

#[cw_serde]
pub struct InstantiateMsg {
    pub name: String,
    pub symbol: String,
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
