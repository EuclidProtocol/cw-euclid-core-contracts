use cosmwasm_schema::cw_serde;
use cosmwasm_std::Addr;

use crate::error::ContractError;

#[cw_serde]
pub struct EuclidAdmin {
    pub general_admin: Addr,
    pub fee_admin: Addr,
    pub migration_admin: Addr,
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
