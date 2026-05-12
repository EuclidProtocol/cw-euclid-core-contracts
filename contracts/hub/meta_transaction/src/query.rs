use cosmwasm_std::Deps;
use euclid::error::ContractError;
use euclid::msgs::meta_transaction::msg::{NonceRelayedResponse, StateResponse};

use crate::state::{ADMIN, NONCES, STATE};

pub fn get_state(deps: &Deps) -> Result<StateResponse, ContractError> {
    let state = STATE.load(deps.storage)?;
    let admin = ADMIN.load(deps.storage)?;
    Ok(StateResponse {
        router_contract: state.router_contract,
        admin,
    })
}

pub fn get_nonce(deps: &Deps, nonce: String) -> Result<NonceRelayedResponse, ContractError> {
    let nonce = NONCES.load(deps.storage, (nonce.clone(), nonce.clone()))?;
    Ok(NonceRelayedResponse { height: nonce })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::instantiate;
    use crate::state::NONCES;
    use cosmwasm_std::testing::{message_info, mock_dependencies, mock_env};
    use cosmwasm_std::{Addr, Uint256};
    use euclid::msgs::meta_transaction::msg::InstantiateMsg;

    type MockDeps = cosmwasm_std::OwnedDeps<
        cosmwasm_std::testing::MockStorage,
        cosmwasm_std::testing::MockApi,
        cosmwasm_std::testing::MockQuerier,
    >;

    fn initialized() -> MockDeps {
        let mut deps = mock_dependencies();
        let sender = deps.api.addr_make("sender");
        let info = message_info(&sender, &[]);
        instantiate(
            deps.as_mut(),
            mock_env(),
            info,
            InstantiateMsg {
                router_contract: Addr::unchecked("router"),
            },
        )
        .unwrap();
        deps
    }

    #[test]
    fn test_query_get_state() {
        let deps = initialized();
        let resp = get_state(&deps.as_ref()).unwrap();
        assert_eq!(resp.router_contract, Addr::unchecked("router"));
    }

    #[test]
    fn test_query_nonce_relayed_missing_key_returns_error() {
        let deps = initialized();
        let res = get_nonce(&deps.as_ref(), "nonexistent_nonce".to_string());
        assert!(res.is_err(), "expected error for missing nonce");
    }

    #[test]
    fn test_query_nonce_relayed_present_returns_height() {
        let mut deps = initialized();
        let nonce_key = "mynonce".to_string();
        NONCES
            .save(
                deps.as_mut().storage,
                (nonce_key.clone(), nonce_key.clone()),
                &Uint256::from(42u128),
            )
            .unwrap();

        let resp = get_nonce(&deps.as_ref(), nonce_key).unwrap();
        assert_eq!(resp.height, Uint256::from(42u128));
    }

    // -------------------------------------------------------------------------
    // get_state returns the full StateResponse including admin fields
    // -------------------------------------------------------------------------

    #[test]
    fn test_query_get_state_includes_admin() {
        use crate::state::ADMIN;

        let deps = initialized();
        let resp = get_state(&deps.as_ref()).unwrap();

        // The fixture instantiates with addr_make("sender") as the sole admin.
        let expected_admin = ADMIN.load(&deps.storage).unwrap();
        assert_eq!(resp.admin, expected_admin);

        // All three roles should be the same address set during instantiation.
        assert_eq!(resp.admin.general_admin, resp.admin.fee_admin);
        assert_eq!(resp.admin.general_admin, resp.admin.migration_admin);
    }

    // -------------------------------------------------------------------------
    // contract::query dispatch — both variants reach the right handler
    // -------------------------------------------------------------------------

    #[test]
    fn test_query_dispatch_get_state() {
        use crate::contract::query;
        use cosmwasm_std::from_json;
        use euclid::msgs::meta_transaction::msg::{QueryMsg, StateResponse};

        let deps = initialized();
        let bin = query(deps.as_ref(), mock_env(), QueryMsg::GetState {}).unwrap();
        let resp: StateResponse = from_json(&bin).unwrap();
        assert_eq!(resp.router_contract, Addr::unchecked("router"));
    }

    #[test]
    fn test_query_dispatch_nonce_relayed_missing_returns_error() {
        use crate::contract::query;
        use euclid::msgs::meta_transaction::msg::QueryMsg;

        let deps = initialized();
        let res = query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::NonceRelayed {
                nonce: "no_such_nonce".to_string(),
            },
        );
        assert!(
            res.is_err(),
            "expected error for missing nonce via dispatch"
        );
    }

    #[test]
    fn test_query_dispatch_nonce_relayed_present_returns_height() {
        use crate::contract::query;
        use cosmwasm_std::from_json;
        use euclid::msgs::meta_transaction::msg::{NonceRelayedResponse, QueryMsg};

        let mut deps = initialized();
        let nonce_key = "dispatch_nonce".to_string();
        NONCES
            .save(
                deps.as_mut().storage,
                (nonce_key.clone(), nonce_key.clone()),
                &Uint256::from(99u128),
            )
            .unwrap();

        let bin = query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::NonceRelayed { nonce: nonce_key },
        )
        .unwrap();
        let resp: NonceRelayedResponse = from_json(&bin).unwrap();
        assert_eq!(resp.height, Uint256::from(99u128));
    }
}
