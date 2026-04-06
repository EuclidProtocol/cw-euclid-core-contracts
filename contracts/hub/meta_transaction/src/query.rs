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
    use cosmwasm_std::{Addr, Uint128};
    use euclid::msgs::meta_transaction::msg::InstantiateMsg;

    type MockDeps = cosmwasm_std::OwnedDeps<
        cosmwasm_std::MemoryStorage,
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
                &Uint128::new(42),
            )
            .unwrap();

        let resp = get_nonce(&deps.as_ref(), nonce_key).unwrap();
        assert_eq!(resp.height, Uint128::new(42));
    }
}
