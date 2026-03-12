use cosmwasm_schema::cw_serde;
use cosmwasm_std::{entry_point, Addr, DepsMut, Env, Response};
use cw2::set_contract_version;
use cw_storage_plus::Item;
use euclid::{
    error::ContractError,
    msgs::claimer::msg::{MigrateMsg, State},
};

use crate::state::{ADMIN, STATE};

const CONTRACT_NAME: &str = "crates.io:euclid-claimer";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cw_serde]
struct LegacyState {
    pub vcoin_address: Addr,
    pub router_contract: Addr,
    pub admin: Addr,
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    let migrated = if ADMIN.load(deps.storage).is_ok() {
        false
    } else {
        let legacy_state = Item::<LegacyState>::new("state").load(deps.storage)?;
        let state = State {
            vcoin_address: legacy_state.vcoin_address,
            router_contract: legacy_state.router_contract,
        };
        STATE.save(deps.storage, &state)?;
        ADMIN.save(deps.storage, &legacy_state.admin)?;
        true
    };

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::new()
        .add_attribute("method", "migrate")
        .add_attribute("admins_migrated", migrated.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env};
    use cw2::get_contract_version;
    use cw_storage_plus::Item;

    fn write_legacy_state(
        deps: &mut cosmwasm_std::OwnedDeps<
            cosmwasm_std::MemoryStorage,
            cosmwasm_std::testing::MockApi,
            cosmwasm_std::testing::MockQuerier,
        >,
        admin: Addr,
    ) {
        let legacy = LegacyState {
            vcoin_address: Addr::unchecked("vcoin"),
            router_contract: Addr::unchecked("router"),
            admin,
        };
        Item::<LegacyState>::new("state")
            .save(deps.as_mut().storage, &legacy)
            .unwrap();
    }

    #[test]
    fn test_migrate_skips_when_admin_already_exists() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let admin = deps.api.addr_make("admin");
        let state = State {
            vcoin_address: Addr::unchecked("vcoin"),
            router_contract: Addr::unchecked("router"),
        };
        STATE.save(deps.as_mut().storage, &state).unwrap();
        ADMIN.save(deps.as_mut().storage, &admin).unwrap();

        let res = migrate(deps.as_mut(), env, MigrateMsg {}).unwrap();

        assert_eq!(
            res.attributes
                .iter()
                .find(|a| a.key == "admins_migrated")
                .map(|a| a.value.as_str()),
            Some("false")
        );
    }

    #[test]
    fn test_migrate_extracts_admin_from_legacy_state() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let admin = deps.api.addr_make("admin");
        write_legacy_state(&mut deps, admin.clone());

        let res = migrate(deps.as_mut(), env, MigrateMsg {}).unwrap();

        assert_eq!(
            res.attributes
                .iter()
                .find(|a| a.key == "admins_migrated")
                .map(|a| a.value.as_str()),
            Some("true")
        );

        let saved_admin = ADMIN.load(deps.as_ref().storage).unwrap();
        assert_eq!(saved_admin, admin);

        let saved_state = STATE.load(deps.as_ref().storage).unwrap();
        assert_eq!(saved_state.vcoin_address, Addr::unchecked("vcoin"));
        assert_eq!(saved_state.router_contract, Addr::unchecked("router"));
    }

    #[test]
    fn test_migrate_sets_contract_version() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let admin = deps.api.addr_make("admin");
        write_legacy_state(&mut deps, admin);

        migrate(deps.as_mut(), env, MigrateMsg {}).unwrap();

        let version = get_contract_version(deps.as_ref().storage).unwrap();
        assert_eq!(version.contract, CONTRACT_NAME);
        assert_eq!(version.version, CONTRACT_VERSION);
    }
}
