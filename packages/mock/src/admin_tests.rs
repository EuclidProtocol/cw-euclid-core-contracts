use cosmwasm_std::{
    testing::MockStorage,
    testing::{message_info, mock_env, MockApi, MockQuerier},
    DepsMut, Env, MessageInfo, OwnedDeps, Response,
};
use euclid::{admin::AdminType, error::ContractError};

/// Run the six standard `UpdateAdmin` access-control cases against any contract
/// that uses the `EuclidAdmin` pattern.
///
/// `make_initialized_deps` should return fully instantiated `OwnedDeps` (i.e. the
/// contract's `instantiate` has already been called, with `"sender"` as the admin).
///
/// `call_update_admin` wraps the contract's `execute` dispatcher.  The helper
/// constructs `MessageInfo` and calls your closure with
/// `(deps, env, info, admin_type, new_admin_string)`.
///
/// # Example
/// ```ignore
/// use mock::admin_tests::run_update_admin_access_control;
///
/// run_update_admin_access_control(
///     || {
///         let mut deps = mock_dependencies();
///         let sender = deps.api.addr_make("sender");
///         let info = message_info(&sender, &[]);
///         instantiate(deps.as_mut(), mock_env(), info, make_instantiate_msg()).unwrap();
///         deps
///     },
///     |deps, env, info, admin_type, new_admin| {
///         execute(deps, env, info, ExecuteMsg::UpdateAdmin(UpdateAdminMsg { new_admin, admin_type }))
///     },
/// );
/// ```
pub fn run_update_admin_access_control<F>(
    make_initialized_deps: impl Fn() -> OwnedDeps<MockStorage, MockApi, MockQuerier>,
    call_update_admin: F,
) where
    F: Fn(DepsMut, Env, MessageInfo, AdminType, String) -> Result<Response, ContractError>,
{
    // (test_name, sender_name, admin_type, new_admin_name, expect_error)
    let cases: &[(&str, &str, AdminType, &str, bool)] = &[
        (
            "general_admin_updates_general_admin",
            "sender",
            AdminType::GeneralAdmin,
            "new_general",
            false,
        ),
        (
            "wrong_sender_cannot_update_general_admin",
            "attacker",
            AdminType::GeneralAdmin,
            "new_general",
            true,
        ),
        (
            "general_admin_updates_fee_admin",
            "sender",
            AdminType::FeeAdmin,
            "new_fee",
            false,
        ),
        (
            "wrong_sender_cannot_update_fee_admin",
            "attacker",
            AdminType::FeeAdmin,
            "new_fee",
            true,
        ),
        (
            "migration_admin_updates_migration_admin",
            "sender",
            AdminType::MigrationAdmin,
            "new_migration",
            false,
        ),
        (
            "wrong_sender_cannot_update_migration_admin",
            "attacker",
            AdminType::MigrationAdmin,
            "new_migration",
            true,
        ),
    ];

    for (name, sender_name, admin_type, new_admin_name, expect_error) in cases {
        let mut deps = make_initialized_deps();
        let sender = deps.api.addr_make(sender_name);
        let new_admin = deps.api.addr_make(new_admin_name).to_string();
        let info = message_info(&sender, &[]);

        let result = call_update_admin(
            deps.as_mut(),
            mock_env(),
            info,
            admin_type.clone(),
            new_admin,
        );

        if *expect_error {
            assert!(result.is_err(), "{name}: expected error but got Ok");
        } else {
            assert!(
                result.is_ok(),
                "{name}: expected Ok but got {:?}",
                result.err()
            );
        }
    }
}
