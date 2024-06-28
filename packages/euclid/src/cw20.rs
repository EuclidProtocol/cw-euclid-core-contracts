use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;

use crate::token::TokenWithDenom;

#[cw_serde]
pub enum Cw20ExecuteMsg {
    Transfer { recipient: String, amount: Uint128 },
}

// CW20 Hook Msg
#[cw_serde]
pub enum Cw20HookMsg {
    Deposit {},
    Swap {
        asset: TokenWithDenom,
        min_amount_out: Uint128,
        timeout: Option<u64>,
    },
}
