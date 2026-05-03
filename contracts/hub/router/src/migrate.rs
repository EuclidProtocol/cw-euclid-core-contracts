use crate::contract::{CONTRACT_NAME, CONTRACT_VERSION};
use cosmwasm_std::{entry_point, DepsMut, Env, Response};
use cw2::set_contract_version;
use euclid::{error::ContractError, msgs::router::MigrateMsg};

// No data migration needed for voucher normalization:
// - Deprecated ESCROW_BALANCES/TOKEN_DENOMS: left in storage, no longer read by new code
// - PendingReleaseVoucher, RELEASE_FEES, DEFAULT_RELEASE_FEE: Uint128→Uint256 backward compatible
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::new().add_attribute("method", "migrate"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env};
    use cw2::get_contract_version;

    #[test]
    fn test_migrate_sets_contract_version() {
        let mut deps = mock_dependencies();
        migrate(deps.as_mut(), mock_env(), MigrateMsg {}).unwrap();
        let version = get_contract_version(deps.as_ref().storage).unwrap();
        assert_eq!(version.contract, CONTRACT_NAME);
        assert_eq!(version.version, CONTRACT_VERSION);
    }
}
