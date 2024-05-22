use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;

#[cw_serde]
pub enum Cw20ExecuteMsg {
    Transfer { recipient: String, amount: Uint128 },
}
