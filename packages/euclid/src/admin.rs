use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, DepsMut, Env, Response, WasmMsg};
use std::fmt;

use crate::error::ContractError;

#[cw_serde]
pub struct EuclidAdmin {
    pub general_admin: Addr,
    pub fee_admin: Addr,
    pub migration_admin: Addr,
}

impl fmt::Display for EuclidAdmin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "general_admin: {}, fee_admin: {}, migration_admin: {}",
            self.general_admin, self.fee_admin, self.migration_admin
        )
    }
}

#[cw_serde]
pub enum AdminType {
    GeneralAdmin,
    FeeAdmin,
    MigrationAdmin,
}

impl EuclidAdmin {
    /// Creates a `EuclidAdmin` where all admin roles use the same address/value.
    pub fn default(admin: Addr) -> Self {
        Self {
            general_admin: admin.clone(),
            fee_admin: admin.clone(),
            migration_admin: admin,
        }
    }

    /// Creates a `EuclidAdmin` with distinct values for each admin role.
    pub fn new(general_admin: Addr, fee_admin: Addr, migration_admin: Addr) -> Self {
        Self {
            general_admin,
            fee_admin,
            migration_admin,
        }
    }

    /// Verifies that the sender has the appropriate admin access based on the specified `AdminType`.
    /// Only the admin corresponding to the `AdminType` can perform updates for that role. For example, only the `general_admin` can update the general admin address, and same for the rest.
    pub fn verify_update_access(
        &self,
        sender: &Addr,
        admin_type: &AdminType,
    ) -> Result<(), ContractError> {
        let expected_admin = match admin_type {
            AdminType::GeneralAdmin => &self.general_admin,
            AdminType::FeeAdmin => &self.fee_admin,
            AdminType::MigrationAdmin => &self.migration_admin,
        };

        if sender != expected_admin {
            return Err(ContractError::UnauthorizedWithMsg {
                msg: format!(
                    "only {} can update {}",
                    expected_admin,
                    Self::admin_type_label(admin_type)
                ),
            });
        }

        Ok(())
    }

    fn admin_type_label(admin_type: &AdminType) -> &'static str {
        match admin_type {
            AdminType::GeneralAdmin => "general admin",
            AdminType::FeeAdmin => "fee admin",
            AdminType::MigrationAdmin => "migration admin",
        }
    }
}

pub fn update_admin(
    admins: &EuclidAdmin,
    deps: &DepsMut,
    env: &Env,
    sender: &Addr,
    admin: String,
    admin_type: AdminType,
) -> Result<(EuclidAdmin, Response), ContractError> {
    admins.verify_update_access(sender, &admin_type)?;

    let mut updated_admins = admins.clone();
    let validated_admin = deps.api.addr_validate(admin.as_str())?;
    let mut response = Response::new().add_attribute("method", "update_admin");
    match admin_type {
        AdminType::GeneralAdmin => updated_admins.general_admin = validated_admin,
        AdminType::FeeAdmin => updated_admins.fee_admin = validated_admin,
        AdminType::MigrationAdmin => {
            // Sends a chain-level message to update the contract's admin to the new migration admin address.
            let migrate_msg = WasmMsg::UpdateAdmin {
                contract_addr: env.contract.address.to_string(),
                admin: validated_admin.to_string(),
            };
            updated_admins.migration_admin = validated_admin;
            response = response.add_message(migrate_msg);
        }
    }

    Ok((updated_admins, response))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::{
        testing::MockStorage,
        testing::{mock_dependencies, mock_env},
        CosmosMsg, OwnedDeps,
    };

    type TestDeps =
        OwnedDeps<MockStorage, cosmwasm_std::testing::MockApi, cosmwasm_std::testing::MockQuerier>;

    fn sample_admins(deps: &TestDeps) -> EuclidAdmin {
        EuclidAdmin::new(
            deps.api.addr_make("general_admin"),
            deps.api.addr_make("fee_admin"),
            deps.api.addr_make("migration_admin"),
        )
    }

    #[test]
    fn default_sets_all_admins_to_same_value() {
        let deps = mock_dependencies();
        let admin = deps.api.addr_make("same_admin");
        let admins = EuclidAdmin::default(admin.clone());

        assert_eq!(admins.general_admin, admin);
        assert_eq!(admins.fee_admin, admin);
        assert_eq!(admins.migration_admin, admin);
    }

    #[test]
    fn verify_update_access_accepts_matching_role_sender() {
        let deps = mock_dependencies();
        let admins = sample_admins(&deps);

        assert!(admins
            .verify_update_access(&admins.general_admin, &AdminType::GeneralAdmin)
            .is_ok());
        assert!(admins
            .verify_update_access(&admins.fee_admin, &AdminType::FeeAdmin)
            .is_ok());
        assert!(admins
            .verify_update_access(&admins.migration_admin, &AdminType::MigrationAdmin)
            .is_ok());
    }

    #[test]
    fn verify_update_access_rejects_non_matching_sender() {
        let deps = mock_dependencies();
        let admins = sample_admins(&deps);
        let not_fee_admin = deps.api.addr_make("not_fee_admin");
        let err = admins
            .verify_update_access(&not_fee_admin, &AdminType::FeeAdmin)
            .unwrap_err();

        match err {
            ContractError::UnauthorizedWithMsg { msg } => {
                assert!(msg.contains(&format!("only {} can update fee admin", admins.fee_admin)))
            }
            _ => panic!("unexpected error variant"),
        }
    }

    #[test]
    fn update_admin_updates_general_admin_without_messages() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let general_admin = deps.api.addr_make("general_admin");
        let fee_admin = deps.api.addr_make("fee_admin");
        let migration_admin = deps.api.addr_make("migration_admin");
        let expected_new_general_admin = deps.api.addr_make("new_general_admin");

        let admins = EuclidAdmin::new(general_admin, fee_admin, migration_admin);
        let deps_mut = deps.as_mut();

        let (updated, response) = update_admin(
            &admins,
            &deps_mut,
            &env,
            &admins.general_admin,
            expected_new_general_admin.to_string(),
            AdminType::GeneralAdmin,
        )
        .unwrap();

        assert_eq!(updated.general_admin, expected_new_general_admin);
        assert_eq!(updated.fee_admin, admins.fee_admin);
        assert_eq!(updated.migration_admin, admins.migration_admin);
        assert!(response.messages.is_empty());
    }

    #[test]
    fn update_admin_updates_migration_admin_and_emits_update_admin_msg() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let expected_new_migration_admin = deps.api.addr_make("new_migration_admin");
        let admins = sample_admins(&deps);
        let deps_mut = deps.as_mut();

        let (updated, response) = update_admin(
            &admins,
            &deps_mut,
            &env,
            &admins.migration_admin,
            expected_new_migration_admin.to_string(),
            AdminType::MigrationAdmin,
        )
        .unwrap();

        assert_eq!(
            updated.migration_admin,
            expected_new_migration_admin.clone()
        );
        assert_eq!(response.messages.len(), 1);
        assert_eq!(
            response.messages[0].msg,
            CosmosMsg::Wasm(WasmMsg::UpdateAdmin {
                contract_addr: env.contract.address.to_string(),
                admin: expected_new_migration_admin.to_string(),
            })
        );
    }

    #[test]
    fn display_formats_all_roles() {
        let deps = mock_dependencies();
        let admins = sample_admins(&deps);
        assert_eq!(
            admins.to_string(),
            format!(
                "general_admin: {}, fee_admin: {}, migration_admin: {}",
                admins.general_admin, admins.fee_admin, admins.migration_admin
            )
        );
    }
}
