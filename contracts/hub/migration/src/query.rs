use cosmwasm_std::{
    ensure, to_json_binary, Binary, Decimal, Decimal256, Deps, Env, Isqrt, Uint128,
};

use euclid::error::ContractError;
use euclid::msgs::migrator::GetStateResponse;
use euclid::swap::NextSwapVlp;
use euclid::token::Token;
use euclid::utils::math::Decimal256Ext;

use crate::state::{State, STATE};

pub fn query_state(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&GetStateResponse {
        router: state.router,
        virtual_balance: state.virtual_balance,
        admin: state.admin,
    })?)
}
