use cosmwasm_schema::cw_serde;
use cosmwasm_std::{entry_point, DepsMut, Env, Response};
use cw2::set_contract_version;
use cw_storage_plus::Item;
use euclid::{admin::EuclidAdmin, error::ContractError, msgs::router::MigrateMsg};

use crate::state::{State, STATE};

const CONTRACT_NAME: &str = "crates.io:router";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cw_serde]
struct LegacyStateWithAdmins {
    pub admins: String,
    pub constant_product_vlp_code_id: u64,
    pub stable_vlp_code_id: u64,
    pub locked: bool,
}

#[cw_serde]
struct LegacyStateWithAdmin {
    pub admin: String,
    pub constant_product_vlp_code_id: u64,
    pub stable_vlp_code_id: u64,
    pub locked: bool,
}

/// This is the migrate entry point for the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    let migrated = if STATE.load(deps.storage).is_ok() {
        false
    } else if let Ok(legacy_state) = Item::<LegacyStateWithAdmins>::new("state").load(deps.storage)
    {
        let admin = deps.api.addr_validate(&legacy_state.admins)?;
        let state = State {
            admins: EuclidAdmin::default(admin),
            constant_product_vlp_code_id: legacy_state.constant_product_vlp_code_id,
            stable_vlp_code_id: legacy_state.stable_vlp_code_id,
            locked: legacy_state.locked,
        };
        STATE.save(deps.storage, &state)?;
        true
    } else {
        let legacy_state = Item::<LegacyStateWithAdmin>::new("state").load(deps.storage)?;
        let admin = deps.api.addr_validate(&legacy_state.admin)?;
        let state = State {
            admins: EuclidAdmin::default(admin),
            constant_product_vlp_code_id: legacy_state.constant_product_vlp_code_id,
            stable_vlp_code_id: legacy_state.stable_vlp_code_id,
            locked: legacy_state.locked,
        };
        STATE.save(deps.storage, &state)?;
        true
    };

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::new()
        .add_attribute("method", "migrate")
        .add_attribute("admins_migrated", migrated.to_string()))
}
