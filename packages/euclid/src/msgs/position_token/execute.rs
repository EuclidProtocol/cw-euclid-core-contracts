use cosmwasm_schema::cw_serde;
use cosmwasm_std::{to_json_binary, Addr, CosmosMsg, Int256, StdResult, Uint128, WasmMsg};

#[cw_serde]
#[cfg_attr(not(target_arch = "wasm32"), derive(cw_orch::ExecuteFns))]
pub enum ExecuteMsg {
    Mint(MintMsg),
    Burn {
        token_id: String,
    },
    Transfer {
        token_id: String,
        recipient: String,
    },
    UpdatePosition {
        token_id: String,
        liquidity_change: Int256,
    },
}

impl ExecuteMsg {
    pub fn to_msg(&self, contract_addr: Addr) -> StdResult<CosmosMsg> {
        Ok(WasmMsg::Execute {
            contract_addr: contract_addr.to_string(),
            msg: to_json_binary(&self)?,
            funds: vec![],
        }
        .into())
    }
}

#[cw_serde]
pub struct MintMsg {
    pub token_id: String,
    pub token_info: TokenInfo,
    pub position_info: PositionInfo,
}

#[cw_serde]
pub struct State {
    pub name: String,
    pub symbol: String,
    pub factory: Addr,
    pub vlp_address: String,
    pub total_tokens: u64,
}

#[cw_serde]
pub struct TokenInfo {
    pub owner: Addr,
    pub token_uri: Option<String>,
}

#[cw_serde]
pub struct PositionInfo {
    pub liquidity: Uint128,
}
