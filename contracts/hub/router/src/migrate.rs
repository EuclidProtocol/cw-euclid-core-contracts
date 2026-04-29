use crate::contract::{CONTRACT_NAME, CONTRACT_VERSION};
use crate::state::{State, ADMIN, STATE};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{entry_point, DepsMut, Env, Response};
use cw2::set_contract_version;
use cw_storage_plus::Item;
use euclid::{admin::EuclidAdmin, error::ContractError, msgs::router::MigrateMsg};

#[cw_serde]
struct LegacyStateWithEuclidAdmin {
    pub admins: EuclidAdmin,
    pub constant_product_vlp_code_id: u64,
    pub stable_vlp_code_id: u64,
    pub locked: bool,
}

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
    let migrated = if ADMIN.load(deps.storage).is_ok() {
        false
    } else if let Ok(legacy_state) =
        Item::<LegacyStateWithEuclidAdmin>::new("state").load(deps.storage)
    {
        let state = State {
            constant_product_vlp_code_id: legacy_state.constant_product_vlp_code_id,
            stable_vlp_code_id: legacy_state.stable_vlp_code_id,
            locked: legacy_state.locked,
        };
        STATE.save(deps.storage, &state)?;
        ADMIN.save(deps.storage, &legacy_state.admins)?;
        true
    } else if let Ok(legacy_state) = Item::<LegacyStateWithAdmins>::new("state").load(deps.storage)
    {
        let admin = deps.api.addr_validate(&legacy_state.admins)?;
        let state = State {
            constant_product_vlp_code_id: legacy_state.constant_product_vlp_code_id,
            stable_vlp_code_id: legacy_state.stable_vlp_code_id,
            locked: legacy_state.locked,
        };
        STATE.save(deps.storage, &state)?;
        ADMIN.save(deps.storage, &EuclidAdmin::default(admin))?;
        true
    } else {
        let legacy_state = Item::<LegacyStateWithAdmin>::new("state").load(deps.storage)?;
        let admin = deps.api.addr_validate(&legacy_state.admin)?;
        let state = State {
            constant_product_vlp_code_id: legacy_state.constant_product_vlp_code_id,
            stable_vlp_code_id: legacy_state.stable_vlp_code_id,
            locked: legacy_state.locked,
        };
        STATE.save(deps.storage, &state)?;
        ADMIN.save(deps.storage, &EuclidAdmin::default(admin))?;
        true
    };

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::new()
        .add_attribute("action", "migrate")
        .add_attribute("admins_migrated", migrated.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env};
    use cw_storage_plus::Item;
    use euclid::admin::EuclidAdmin;

    // -------------------------------------------------------------------------
    // Helpers: write each legacy shape directly into mock storage.
    // Non-admin fields are fixed dummies; only `admin`/`admins` varies.
    // -------------------------------------------------------------------------

    fn write_legacy_euclid_admin_state(
        deps: &mut cosmwasm_std::OwnedDeps<
            cosmwasm_std::MemoryStorage,
            cosmwasm_std::testing::MockApi,
            cosmwasm_std::testing::MockQuerier,
        >,
        admins: EuclidAdmin,
    ) {
        let legacy = LegacyStateWithEuclidAdmin {
            admins,
            constant_product_vlp_code_id: 1,
            stable_vlp_code_id: 2,
            locked: false,
        };
        Item::<LegacyStateWithEuclidAdmin>::new("state")
            .save(deps.as_mut().storage, &legacy)
            .unwrap();
    }

    fn write_legacy_admins_state(
        deps: &mut cosmwasm_std::OwnedDeps<
            cosmwasm_std::MemoryStorage,
            cosmwasm_std::testing::MockApi,
            cosmwasm_std::testing::MockQuerier,
        >,
        admins: &str,
    ) {
        let legacy = LegacyStateWithAdmins {
            admins: admins.to_string(),
            constant_product_vlp_code_id: 1,
            stable_vlp_code_id: 2,
            locked: false,
        };
        Item::<LegacyStateWithAdmins>::new("state")
            .save(deps.as_mut().storage, &legacy)
            .unwrap();
    }

    fn write_legacy_admin_state(
        deps: &mut cosmwasm_std::OwnedDeps<
            cosmwasm_std::MemoryStorage,
            cosmwasm_std::testing::MockApi,
            cosmwasm_std::testing::MockQuerier,
        >,
        admin: &str,
    ) {
        let legacy = LegacyStateWithAdmin {
            admin: admin.to_string(),
            constant_product_vlp_code_id: 1,
            stable_vlp_code_id: 2,
            locked: false,
        };
        Item::<LegacyStateWithAdmin>::new("state")
            .save(deps.as_mut().storage, &legacy)
            .unwrap();
    }

    // -------------------------------------------------------------------------
    // Branch 1 – current Admin already exists → admin not migrated
    // -------------------------------------------------------------------------

    #[test]
    fn test_migrate_skips_when_current_admin_exists() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let admin = deps.api.addr_make("admin");
        ADMIN
            .save(deps.as_mut().storage, &EuclidAdmin::default(admin))
            .unwrap();

        let res = migrate(deps.as_mut(), env, MigrateMsg {}).unwrap();

        assert_eq!(
            res.attributes
                .iter()
                .find(|a| a.key == "admins_migrated")
                .map(|a| a.value.as_str()),
            Some("false")
        );
    }

    // -------------------------------------------------------------------------
    // Branch 2 – LegacyStateWithEuclidAdmin → admin is migrated
    // -------------------------------------------------------------------------

    #[test]
    fn test_migrate_promotes_legacy_euclid_admin_to_separate_item() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let admin = deps.api.addr_make("admin");
        let euclid_admin = EuclidAdmin::default(admin);
        write_legacy_euclid_admin_state(&mut deps, euclid_admin.clone());

        migrate(deps.as_mut(), env, MigrateMsg {}).unwrap();

        let saved_admin = ADMIN.load(deps.as_ref().storage).unwrap();
        assert_eq!(saved_admin, euclid_admin);

        let saved_state = STATE.load(deps.as_ref().storage).unwrap();
        // constant_product_vlp_code_id should be 1 from write_legacy_euclid_admin_state
        assert_eq!(saved_state.constant_product_vlp_code_id, 1);
    }

    // -------------------------------------------------------------------------
    // Branch 3 – LegacyStateWithAdmins → admin is migrated
    // -------------------------------------------------------------------------

    #[test]
    fn test_migrate_reports_migrated_from_legacy_admins_state() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let admin = deps.api.addr_make("admin");
        write_legacy_admins_state(&mut deps, admin.as_str());

        let res = migrate(deps.as_mut(), env, MigrateMsg {}).unwrap();

        assert_eq!(
            res.attributes
                .iter()
                .find(|a| a.key == "admins_migrated")
                .map(|a| a.value.as_str()),
            Some("true")
        );
    }

    #[test]
    fn test_migrate_promotes_legacy_admins_string_to_separate_admin_item() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let admin = deps.api.addr_make("admin");
        write_legacy_admins_state(&mut deps, admin.as_str());

        migrate(deps.as_mut(), env, MigrateMsg {}).unwrap();

        let saved = ADMIN.load(deps.as_ref().storage).unwrap();
        assert_eq!(saved, EuclidAdmin::default(admin));
    }

    #[test]
    fn test_migrate_invalid_admins_address_returns_error() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        write_legacy_admins_state(&mut deps, "BAD!!ADDR");

        let err = migrate(deps.as_mut(), env, MigrateMsg {});
        assert!(
            err.is_err(),
            "Expected error when legacy admins address is invalid"
        );
    }

    // -------------------------------------------------------------------------
    // Branch 4 – LegacyStateWithAdmin → admin is migrated
    // -------------------------------------------------------------------------

    #[test]
    fn test_migrate_reports_migrated_from_legacy_admin_state() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let admin = deps.api.addr_make("admin");
        write_legacy_admin_state(&mut deps, admin.as_str());

        let res = migrate(deps.as_mut(), env, MigrateMsg {}).unwrap();

        assert_eq!(
            res.attributes
                .iter()
                .find(|a| a.key == "admins_migrated")
                .map(|a| a.value.as_str()),
            Some("true")
        );
    }

    #[test]
    fn test_migrate_promotes_legacy_admin_string_to_separate_admin_item() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let admin = deps.api.addr_make("admin");
        write_legacy_admin_state(&mut deps, admin.as_str());

        migrate(deps.as_mut(), env, MigrateMsg {}).unwrap();

        let saved = ADMIN.load(deps.as_ref().storage).unwrap();
        assert_eq!(saved, EuclidAdmin::default(admin));
    }
}
