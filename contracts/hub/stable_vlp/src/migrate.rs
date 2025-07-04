use cosmwasm_std::{entry_point, DepsMut, Env, Response, Uint128};
use euclid::{error::ContractError, msgs::stable_vlp::MigrateMsg, pool::MINIMUM_LIQUIDITY};

use cw2::{get_contract_version, set_contract_version, CONTRACT};

use crate::state::{COLLATERAL_LP_TOKENS, STATE};

const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// This is the migrate entry point for the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(mut deps: DepsMut, env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    let mut version = get_contract_version(deps.storage)?;
    let response = match version.version.as_str() {
        "0.2.0" => migrate_v0_2_0_to_v0_2_2(&mut deps, env),
        _ => Ok(Response::default()),
    }?;

    version.version = CONTRACT_VERSION.to_string();
    set_contract_version(deps.storage, &version.contract, &version.version)?;
    Ok(response)
}

// Migrate v0.2.0 to 0.2.1 with token denoms
fn migrate_v0_2_0_to_v0_2_2(deps: &mut DepsMut, _env: Env) -> Result<Response, ContractError> {
    let contract_version = CONTRACT.load(deps.storage)?;
    if contract_version.version != "0.2.2" {
        return Ok(Response::default());
    }
    let mut state = STATE.load(deps.storage)?;
    state.total_lp_tokens = state
        .total_lp_tokens
        .checked_add(Uint128::from(MINIMUM_LIQUIDITY))?;
    STATE.save(deps.storage, &state)?;
    COLLATERAL_LP_TOKENS.save(deps.storage, &Uint128::from(MINIMUM_LIQUIDITY))?;

    Ok(Response::default()
        .add_attribute("action", "migrate")
        .add_attribute("collateral_lp_tokens", MINIMUM_LIQUIDITY.to_string()))
}
