use crate::contract::{CONTRACT_NAME, CONTRACT_VERSION};
use crate::state::{State, STATE};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{entry_point, DepsMut, Env, Response};
use cw2::set_contract_version;
use cw_storage_plus::Item;
use euclid::{error::ContractError, msgs::router::MigrateMsg};

#[cw_serde]
struct LegacyStateWithoutCLP {
    pub constant_product_vlp_code_id: u64,
    pub stable_vlp_code_id: u64,
    pub locked: bool,
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, msg: MigrateMsg) -> Result<Response, ContractError> {
    let migrated = if STATE.load(deps.storage).is_ok() {
        false
    } else {
        let legacy_state = Item::<LegacyStateWithoutCLP>::new("state").load(deps.storage)?;
        let state = State {
            constant_product_vlp_code_id: legacy_state.constant_product_vlp_code_id,
            stable_vlp_code_id: legacy_state.stable_vlp_code_id,
            concentrated_vlp_code_id: msg
                .concentrated_vlp_code_id
                .ok_or(ContractError::new("Concentrated VLP code ID not set"))?,
            locked: legacy_state.locked,
        };
        STATE.save(deps.storage, &state)?;
        true
    };
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::new()
        .add_attribute("method", "migrate")
        .add_attribute("state_migrated", migrated.to_string()))
}
