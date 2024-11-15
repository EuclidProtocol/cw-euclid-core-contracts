use cosmwasm_std::{entry_point, DepsMut, Env, Response};
use cw2::CONTRACT;
use euclid::{error::ContractError, msgs::vlp::MigrateMsg};

use crate::state::TOKEN_DENOMS;

/// This is the migrate entry point for the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, env: Env, msg: MigrateMsg) -> Result<Response, ContractError> {
    let response = migrate_v0_2_0_to_v0_2_1(deps, env, msg)?;
    Ok(response)
}

// Migrate v0.2.0 to 0.2.1 with token denoms
fn migrate_v0_2_0_to_v0_2_1(
    deps: DepsMut,
    _env: Env,
    msg: MigrateMsg,
) -> Result<Response, ContractError> {
    let contract_version = CONTRACT.load(deps.storage)?;
    if contract_version.version != "0.2.0" {
        return Ok(Response::default());
    }

    let denoms_iter = msg.denoms.iter();
    for (token, denom) in denoms_iter {
        let mut token_denoms = TOKEN_DENOMS
            .may_load(deps.storage, token.clone())?
            .unwrap_or_default();
        token_denoms.push(denom.clone());
        TOKEN_DENOMS
            .save(deps.storage, token.clone(), &token_denoms)
            .unwrap();
    }

    Ok(Response::default()
        .add_attribute("action", "migrate")
        .add_attribute("migrated_tokens", "true"))
}
