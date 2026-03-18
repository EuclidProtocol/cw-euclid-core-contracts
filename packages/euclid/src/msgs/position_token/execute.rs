use cosmwasm_schema::cw_serde;

#[cw_serde]
#[cfg_attr(not(target_arch = "wasm32"), derive(cw_orch::ExecuteFns))]
pub enum ExecuteMsg {
    Mint {
        token_id: String,
        owner: String,
        token_uri: Option<String>,
    },
    Burn {
        token_id: String,
    },
    Transfer {
        token_id: String,
        recipient: String,
    },
    UpdateState {
        admin: Option<String>,
        minter: Option<String>,
    },
}
