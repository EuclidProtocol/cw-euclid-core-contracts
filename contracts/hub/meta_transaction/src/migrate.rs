use cosmwasm_schema::cw_serde;
use cosmwasm_std::{entry_point, Addr, DepsMut, Env, Response};
use cw2::set_contract_version;
use cw_storage_plus::Item;
use euclid::{
    admin::EuclidAdmin,
    error::ContractError,
    msgs::meta_transaction::msg::{MigrateMsg, State},
};

use crate::state::STATE;

const CONTRACT_NAME: &str = "crates.io:meta-transaction";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cw_serde]
struct LegacyState {
    pub router_contract: Addr,
    pub admin: String,
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    let migrated = if STATE.load(deps.storage).is_ok() {
        false
    } else {
        let legacy_state = Item::<LegacyState>::new("state").load(deps.storage)?;
        let admin = deps.api.addr_validate(&legacy_state.admin)?;
        let state = State {
            router_contract: legacy_state.router_contract,
            admin: EuclidAdmin::default(admin),
        };
        STATE.save(deps.storage, &state)?;
        true
    };

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::new()
        .add_attribute("method", "migrate")
        .add_attribute("admins_migrated", migrated.to_string()))
}
