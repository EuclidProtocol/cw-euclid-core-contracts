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
        testing::{mock_dependencies, mock_env},
        CosmosMsg,
    };

    fn sample_admins() -> EuclidAdmin {
        EuclidAdmin::new(
            Addr::unchecked("general_admin"),
            Addr::unchecked("fee_admin"),
            Addr::unchecked("migration_admin"),
        )
    }

    #[test]
    fn default_sets_all_admins_to_same_value() {
        let admin = Addr::unchecked("same_admin");
        let admins = EuclidAdmin::default(admin.clone());

        assert_eq!(admins.general_admin, admin);
        assert_eq!(admins.fee_admin, admin);
        assert_eq!(admins.migration_admin, admin);
    }

    #[test]
    fn verify_update_access_accepts_matching_role_sender() {
        let admins = sample_admins();

        assert!(admins
            .verify_update_access(&Addr::unchecked("general_admin"), &AdminType::GeneralAdmin)
            .is_ok());
        assert!(admins
            .verify_update_access(&Addr::unchecked("fee_admin"), &AdminType::FeeAdmin)
            .is_ok());
        assert!(admins
            .verify_update_access(
                &Addr::unchecked("migration_admin"),
                &AdminType::MigrationAdmin
            )
            .is_ok());
    }

    #[test]
    fn verify_update_access_rejects_non_matching_sender() {
        let admins = sample_admins();
        let err = admins
            .verify_update_access(&Addr::unchecked("not_fee_admin"), &AdminType::FeeAdmin)
            .unwrap_err();

        match err {
            ContractError::UnauthorizedWithMsg { msg } => {
                assert!(msg.contains("only fee_admin can update fee admin"))
            }
            _ => panic!("unexpected error variant"),
        }
    }

    #[test]
    fn update_admin_updates_general_admin_without_messages() {
        let mut deps = mock_dependencies();
        let deps_mut = deps.as_mut();
        let env = mock_env();
        let admins = sample_admins();

        let (updated, response) = update_admin(
            &admins,
            &deps_mut,
            &env,
            &Addr::unchecked("general_admin"),
            "new_general_admin".to_string(),
            AdminType::GeneralAdmin,
        )
        .unwrap();

        assert_eq!(updated.general_admin, Addr::unchecked("new_general_admin"));
        assert_eq!(updated.fee_admin, Addr::unchecked("fee_admin"));
        assert_eq!(updated.migration_admin, Addr::unchecked("migration_admin"));
        assert!(response.messages.is_empty());
    }

    #[test]
    fn update_admin_updates_migration_admin_and_emits_update_admin_msg() {
        let mut deps = mock_dependencies();
        let deps_mut = deps.as_mut();
        let env = mock_env();
        let admins = sample_admins();

        let (updated, response) = update_admin(
            &admins,
            &deps_mut,
            &env,
            &Addr::unchecked("migration_admin"),
            "new_migration_admin".to_string(),
            AdminType::MigrationAdmin,
        )
        .unwrap();

        assert_eq!(
            updated.migration_admin,
            Addr::unchecked("new_migration_admin")
        );
        assert_eq!(response.messages.len(), 1);
        assert_eq!(
            response.messages[0].msg,
            CosmosMsg::Wasm(WasmMsg::UpdateAdmin {
                contract_addr: env.contract.address.to_string(),
                admin: "new_migration_admin".to_string(),
            })
        );
    }

    #[test]
    fn display_formats_all_roles() {
        let admins = sample_admins();
        assert_eq!(
            admins.to_string(),
            "general_admin: general_admin, fee_admin: fee_admin, migration_admin: migration_admin"
        );
    }
}
