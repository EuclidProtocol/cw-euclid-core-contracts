use crate::contract::{CONTRACT_NAME, CONTRACT_VERSION};
use crate::state::{State, ADMIN, STATE};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{entry_point, DepsMut, Env, Response};
use cw2::set_contract_version;
use cw_storage_plus::Item;
use euclid::{admin::EuclidAdmin, error::ContractError, msgs::factory::MigrateMsg};

#[cw_serde]
struct LegacyStateWithEuclidAdmin {
    pub router_contract: String,
    pub relayer_contract: cosmwasm_std::Addr,
    pub admin: EuclidAdmin,
    pub escrow_code_id: u64,
    pub lp_code_id: u64,
    pub chain_uid: euclid::chain::ChainUid,
    pub is_native: bool,
}

#[cw_serde]
struct LegacyState {
    pub router_contract: String,
    pub relayer_contract: cosmwasm_std::Addr,
    pub admin: String,
    pub escrow_code_id: u64,
    pub lp_code_id: u64,
    pub chain_uid: euclid::chain::ChainUid,
    pub is_native: bool,
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
            router_contract: legacy_state.router_contract,
            relayer_contract: legacy_state.relayer_contract,
            escrow_code_id: legacy_state.escrow_code_id,
            lp_code_id: legacy_state.lp_code_id,
            chain_uid: legacy_state.chain_uid,
            is_native: legacy_state.is_native,
        };
        STATE.save(deps.storage, &state)?;
        ADMIN.save(deps.storage, &legacy_state.admin)?;
        true
    } else {
        let legacy_state = Item::<LegacyState>::new("state").load(deps.storage)?;
        let admin = deps.api.addr_validate(&legacy_state.admin)?;
        let state = State {
            router_contract: legacy_state.router_contract,
            relayer_contract: legacy_state.relayer_contract,
            escrow_code_id: legacy_state.escrow_code_id,
            lp_code_id: legacy_state.lp_code_id,
            chain_uid: legacy_state.chain_uid,
            is_native: legacy_state.is_native,
        };
        STATE.save(deps.storage, &state)?;
        ADMIN.save(deps.storage, &EuclidAdmin::default(admin))?;
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
    use euclid::admin::EuclidAdmin;
    use euclid::chain::ChainUid;

    // -------------------------------------------------------------------------
    // Helper: write a LegacyState directly into mock storage.
    // Only `admin` is varied across tests; all other fields are fixed dummies.
    // -------------------------------------------------------------------------

    fn write_legacy_euclid_admin_state(
        deps: &mut cosmwasm_std::OwnedDeps<
            cosmwasm_std::MemoryStorage,
            cosmwasm_std::testing::MockApi,
            cosmwasm_std::testing::MockQuerier,
        >,
        admin: EuclidAdmin,
    ) {
        let legacy = LegacyStateWithEuclidAdmin {
            router_contract: "router".to_string(),
            relayer_contract: deps.api.addr_make("relayer"),
            admin,
            escrow_code_id: 1,
            lp_code_id: 2,
            chain_uid: ChainUid::create("andr".to_string()).unwrap(),
            is_native: false,
        };
        Item::<LegacyStateWithEuclidAdmin>::new("state")
            .save(deps.as_mut().storage, &legacy)
            .unwrap();
    }

    fn write_legacy_state(
        deps: &mut cosmwasm_std::OwnedDeps<
            cosmwasm_std::MemoryStorage,
            cosmwasm_std::testing::MockApi,
            cosmwasm_std::testing::MockQuerier,
        >,
        admin: &str,
    ) {
        let legacy = LegacyState {
            router_contract: "router".to_string(),
            relayer_contract: deps.api.addr_make("relayer"),
            admin: admin.to_string(),
            escrow_code_id: 1,
            lp_code_id: 2,
            chain_uid: ChainUid::create("andr".to_string()).unwrap(),
            is_native: false,
        };
        Item::<LegacyState>::new("state")
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

        let res = migrate(
            deps.as_mut(),
            env,
            MigrateMsg {
                mock_relayer_address: None,
            },
        )
        .unwrap();

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

        migrate(
            deps.as_mut(),
            env,
            MigrateMsg {
                mock_relayer_address: None,
            },
        )
        .unwrap();

        let saved_admin = ADMIN.load(deps.as_ref().storage).unwrap();
        assert_eq!(saved_admin, euclid_admin);

        let saved_state = STATE.load(deps.as_ref().storage).unwrap();
        assert_eq!(saved_state.escrow_code_id, 1);
    }

    // -------------------------------------------------------------------------
    // Branch 3 – LegacyState exists → admin is migrated
    // -------------------------------------------------------------------------

    #[test]
    fn test_migrate_reports_migrated_when_legacy_state_exists() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let admin = deps.api.addr_make("admin");
        write_legacy_state(&mut deps, admin.as_str());

        let res = migrate(
            deps.as_mut(),
            env,
            MigrateMsg {
                mock_relayer_address: None,
            },
        )
        .unwrap();

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
        write_legacy_state(&mut deps, admin.as_str());

        migrate(
            deps.as_mut(),
            env,
            MigrateMsg {
                mock_relayer_address: None,
            },
        )
        .unwrap();

        let saved = ADMIN.load(deps.as_ref().storage).unwrap();
        assert_eq!(saved, EuclidAdmin::default(admin));
    }

    #[test]
    fn test_migrate_invalid_admin_address_returns_error() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        write_legacy_state(&mut deps, "BAD!!ADDR");

        let err = migrate(
            deps.as_mut(),
            env,
            MigrateMsg {
                mock_relayer_address: None,
            },
        );
        assert!(
            err.is_err(),
            "Expected error when legacy admin address is invalid"
        );
    }

    #[test]
    fn test_migrate_sets_contract_version() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let admin = deps.api.addr_make("admin");
        write_legacy_state(&mut deps, admin.as_str());

        migrate(
            deps.as_mut(),
            env,
            MigrateMsg {
                mock_relayer_address: None,
            },
        )
        .unwrap();

        let version = get_contract_version(deps.as_ref().storage).unwrap();
        assert_eq!(version.contract, CONTRACT_NAME);
        assert_eq!(version.version, CONTRACT_VERSION);
    }
}
