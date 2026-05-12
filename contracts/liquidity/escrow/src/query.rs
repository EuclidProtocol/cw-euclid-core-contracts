use cosmwasm_std::{to_json_binary, Binary, Deps};
use euclid::{
    error::ContractError,
    msgs::escrow::{
        AllowedDenomsResponse, AllowedTokenResponse, DenomBalanceResponse, StateResponse,
        TokenIdResponse,
    },
    token::TokenType,
};

use crate::state::{ALLOWED_DENOMS, DENOM_TO_AMOUNT, STATE};

// New escrow query functions

// Returns the token id
pub fn query_token_id(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&TokenIdResponse {
        token_id: state.token_id.to_string(),
    })?)
}

// Returns allowed tokens
pub fn query_token_allowed(deps: Deps, denom: TokenType) -> Result<Binary, ContractError> {
    let registered_denom = ALLOWED_DENOMS.may_load(deps.storage)?.unwrap_or_default();
    let response = AllowedTokenResponse {
        allowed: registered_denom.contains(&denom),
    };

    Ok(to_json_binary(&response)?)
}

// Returns the allowed denoms
pub fn query_allowed_denoms(deps: Deps) -> Result<Binary, ContractError> {
    let denoms = ALLOWED_DENOMS.may_load(deps.storage)?.unwrap_or_default();
    let response = AllowedDenomsResponse { denoms };

    Ok(to_json_binary(&response)?)
}

pub fn query_denom_balance(deps: Deps, denom: String) -> Result<Binary, ContractError> {
    let amount = DENOM_TO_AMOUNT
        .may_load(deps.storage, denom.clone())?
        .unwrap_or_default();
    Ok(to_json_binary(&DenomBalanceResponse { denom, amount })?)
}

// Returns the allowed denoms
pub fn query_state(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    let response = StateResponse {
        token: state.token_id,
        factory_address: state.factory_address,
        total_amount: state.total_amount,
    };

    Ok(to_json_binary(&response)?)
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{from_json, testing::mock_env, Uint256};
    use euclid::{
        msgs::escrow::{
            AllowedDenomsResponse, AllowedTokenResponse, ExecuteMsg, QueryMsg, StateResponse,
            TokenIdResponse,
        },
        token::TokenType,
    };
    use rstest::rstest;

    use crate::{
        contract::{execute, query},
        testing::{
            fixtures::{initialized, with_deposit},
            helpers::{init_no_denom, native_denom, token, MockDeps, TOKEN_ID},
        },
    };

    use cosmwasm_std::testing::{message_info, mock_dependencies};

    #[rstest]
    fn test_query_state(initialized: MockDeps) {
        let factory = initialized.api.addr_make("factory");
        let res: StateResponse =
            from_json(query(initialized.as_ref(), mock_env(), QueryMsg::State {}).unwrap())
                .unwrap();

        assert_eq!(res.token, token());
        assert_eq!(res.factory_address, factory);
        assert_eq!(res.total_amount, Uint256::zero());
    }

    #[rstest]
    fn test_query_state_reflects_deposit(with_deposit: MockDeps) {
        let res: StateResponse =
            from_json(query(with_deposit.as_ref(), mock_env(), QueryMsg::State {}).unwrap())
                .unwrap();
        assert_eq!(res.total_amount, Uint256::from(1_000u128));
    }

    #[rstest]
    fn test_query_token_id(initialized: MockDeps) {
        let res: TokenIdResponse =
            from_json(query(initialized.as_ref(), mock_env(), QueryMsg::TokenId {}).unwrap())
                .unwrap();
        assert_eq!(res.token_id, TOKEN_ID);
    }

    #[rstest]
    fn test_query_token_allowed_returns_true_for_allowed(initialized: MockDeps) {
        let res: AllowedTokenResponse = from_json(
            query(
                initialized.as_ref(),
                mock_env(),
                QueryMsg::TokenAllowed {
                    denom: native_denom(),
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert!(res.allowed);
    }

    #[rstest]
    fn test_query_token_allowed_returns_false_for_unknown(initialized: MockDeps) {
        let res: AllowedTokenResponse = from_json(
            query(
                initialized.as_ref(),
                mock_env(),
                QueryMsg::TokenAllowed {
                    denom: TokenType::Native {
                        denom: "never_added".to_string(),
                        decimals: None,
                    },
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert!(!res.allowed);
    }

    #[rstest]
    fn test_query_token_allowed_returns_false_after_disallow(mut initialized: MockDeps) {
        let factory = initialized.api.addr_make("factory");
        let info = message_info(&factory, &[]);
        execute(
            initialized.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::DisallowDenom {
                denom: native_denom(),
            },
        )
        .unwrap();

        let res: AllowedTokenResponse = from_json(
            query(
                initialized.as_ref(),
                mock_env(),
                QueryMsg::TokenAllowed {
                    denom: native_denom(),
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert!(!res.allowed);
    }

    #[test]
    fn test_query_allowed_denoms_empty_initially() {
        let mut deps = mock_dependencies();
        init_no_denom(&mut deps);

        let res: AllowedDenomsResponse =
            from_json(query(deps.as_ref(), mock_env(), QueryMsg::AllowedDenoms {}).unwrap())
                .unwrap();
        assert!(res.denoms.is_empty());
    }

    #[rstest]
    fn test_query_allowed_denoms_reflects_additions(mut initialized: MockDeps) {
        let factory = initialized.api.addr_make("factory");
        let info = message_info(&factory, &[]);
        let denom2 = TokenType::Native {
            denom: "uatom".to_string(),
            decimals: None,
        };
        execute(
            initialized.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::AddAllowedDenom {
                denom: denom2.clone(),
            },
        )
        .unwrap();

        let res: AllowedDenomsResponse =
            from_json(query(initialized.as_ref(), mock_env(), QueryMsg::AllowedDenoms {}).unwrap())
                .unwrap();

        assert_eq!(res.denoms.len(), 2);
        assert!(res.denoms.contains(&native_denom()));
        assert!(res.denoms.contains(&denom2));
    }

    #[rstest]
    fn test_query_allowed_denoms_decrements_after_disallow(mut initialized: MockDeps) {
        let factory = initialized.api.addr_make("factory");
        let info = message_info(&factory, &[]);
        execute(
            initialized.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::DisallowDenom {
                denom: native_denom(),
            },
        )
        .unwrap();

        let res: AllowedDenomsResponse =
            from_json(query(initialized.as_ref(), mock_env(), QueryMsg::AllowedDenoms {}).unwrap())
                .unwrap();
        assert!(res.denoms.is_empty());
    }
}
