use cosmwasm_schema::cw_serde;
use cosmwasm_std::{entry_point, DepsMut, Env, Response};
use cw2::set_contract_version;
use cw_storage_plus::Item;
use euclid::{admin::EuclidAdmin, error::ContractError};
use relayer::msgs::{MigrateMsg, State, Validator};

use crate::state::{ADMIN, STATE};

const CONTRACT_NAME: &str = "crates.io:euclid-relayer";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cw_serde]
struct LegacyState {
    pub message_signer: Validator,
    pub signature_threshold: u8,
    pub admin: String,
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    let migrated = if ADMIN.load(deps.storage).is_ok() {
        false
    } else {
        let legacy_state = Item::<LegacyState>::new("state").load(deps.storage)?;
        let admin = deps.api.addr_validate(&legacy_state.admin)?;
        let state = State {
            message_signer: legacy_state.message_signer,
            signature_threshold: legacy_state.signature_threshold,
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

    // -------------------------------------------------------------------------
    // Helper: write a LegacyState directly into mock storage.
    // Only `admin` is varied across tests; all other fields are fixed dummies.
    // -------------------------------------------------------------------------

    fn write_legacy_state(
        deps: &mut cosmwasm_std::OwnedDeps<
            cosmwasm_std::MemoryStorage,
            cosmwasm_std::testing::MockApi,
            cosmwasm_std::testing::MockQuerier,
        >,
        admin: &str,
    ) {
        let legacy = LegacyState {
            message_signer: Validator {
                pubkey: cosmwasm_std::Binary::from(vec![0u8; 33]),
                address: "signer".to_string(),
            },
            signature_threshold: 1,
            admin: admin.to_string(),
        };
        Item::<LegacyState>::new("state")
            .save(deps.as_mut().storage, &legacy)
            .unwrap();
    }

    // -------------------------------------------------------------------------
    // Branch 1 – current State already exists → admin not migrated
    // -------------------------------------------------------------------------

    #[test]
    fn test_migrate_skips_when_current_state_exists() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let admin = deps.api.addr_make("admin");
        let current_state = State {
            message_signer: Validator {
                pubkey: cosmwasm_std::Binary::from(vec![0u8; 33]),
                address: "signer".to_string(),
            },
            signature_threshold: 1,
        };
        STATE.save(deps.as_mut().storage, &current_state).unwrap();
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
    // Branch 2 – LegacyState exists → admin is migrated
    // -------------------------------------------------------------------------

    #[test]
    fn test_migrate_reports_migrated_when_legacy_state_exists() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let admin = deps.api.addr_make("admin");
        write_legacy_state(&mut deps, admin.as_str());

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
    fn test_migrate_promotes_legacy_admin_string_to_euclid_admin() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let admin = deps.api.addr_make("admin");
        write_legacy_state(&mut deps, admin.as_str());

        migrate(deps.as_mut(), env, MigrateMsg {}).unwrap();

        let saved_admin = ADMIN.load(deps.as_ref().storage).unwrap();
        assert_eq!(saved_admin, EuclidAdmin::default(admin));
    }

    #[test]
    fn test_migrate_invalid_admin_address_returns_error() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        write_legacy_state(&mut deps, "BAD!!ADDR");

        let err = migrate(deps.as_mut(), env, MigrateMsg {});
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

        migrate(deps.as_mut(), env, MigrateMsg {}).unwrap();

        let version = get_contract_version(deps.as_ref().storage).unwrap();
        assert_eq!(version.contract, CONTRACT_NAME);
        assert_eq!(version.version, CONTRACT_VERSION);
    }
}
