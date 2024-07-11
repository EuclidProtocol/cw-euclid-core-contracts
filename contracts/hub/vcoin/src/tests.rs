#[cfg(test)]
mod tests {

    use crate::contract::{execute, instantiate};
    use crate::state::{SNAPSHOT_BALANCES, STATE};

    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{coins, Addr, DepsMut, Response, Uint128};
    use euclid::chain::{ChainUid, CrossChainUser};
    use euclid::error::ContractError;
    use euclid::msgs::vcoin::{ExecuteBurn, ExecuteMint, ExecuteMsg, InstantiateMsg, State};
    use euclid::vcoin::BalanceKey;

    fn init(deps: DepsMut) -> Response {
        let msg = InstantiateMsg {
            router: Addr::unchecked("router"),
            admin: None,
        };
        let info = mock_info("router", &[]);
        instantiate(deps, mock_env(), info, msg).unwrap()
    }

    #[test]
    fn test_init() {
        let mut deps = mock_dependencies();
        let res = init(deps.as_mut());
        assert_eq!(0, res.messages.len());

        let expected_state = State {
            router: "router".to_string(),
            admin: Addr::unchecked("router"),
        };
        let state = STATE.load(&deps.storage).unwrap();
        assert_eq!(state, expected_state);
    }

    #[test]
    fn test_mint_and_burn() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(deps.as_mut());

        // Unauthorized sender
        let info = mock_info("not_router", &[]);
        let cross_chain_user = CrossChainUser {
            chain_uid: ChainUid::create("1".to_string()).unwrap(),
            address: "cross_chain_user_address".to_string(),
        };
        let balance_key = BalanceKey {
            cross_chain_user,
            token_id: "token1".to_string(),
        };

        let msg = ExecuteMsg::Mint(ExecuteMint {
            amount: Uint128::new(10_u128),
            balance_key: balance_key.clone(),
        });

        let err = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap_err();
        assert_eq!(ContractError::Unauthorized {}, err);

        let info = mock_info("router", &[]);

        let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let expected_snapshot_balance = Uint128::new(10_u128);

        let key = balance_key.clone().to_serialized_balance_key();
        let snapshot_balance = SNAPSHOT_BALANCES.load(&deps.storage, key).unwrap();

        assert_eq!(expected_snapshot_balance, snapshot_balance);

        // Invalid zero amount
        let msg = ExecuteMsg::Mint(ExecuteMint {
            amount: Uint128::zero(),
            balance_key: balance_key.clone(),
        });

        let err = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap_err();
        assert_eq!(ContractError::ZeroAssetAmount {}, err);

        // Burn //

        let msg = ExecuteMsg::Burn(ExecuteBurn {
            amount: Uint128::new(5_u128),
            balance_key: balance_key.clone(),
        });

        // Unauthorized sender
        let info = mock_info("not_router", &[]);
        let err = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap_err();
        assert_eq!(ContractError::Unauthorized {}, err);

        let info = mock_info("router", &[]);
        let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let expected_snapshot_balance = Uint128::new(5_u128);

        let key = balance_key.clone().to_serialized_balance_key();
        let snapshot_balance = SNAPSHOT_BALANCES.load(&deps.storage, key).unwrap();

        assert_eq!(expected_snapshot_balance, snapshot_balance);

        // Zero burn amount
        let msg = ExecuteMsg::Burn(ExecuteBurn {
            amount: Uint128::zero(),
            balance_key: balance_key.clone(),
        });

        let err = execute(deps.as_mut(), env.clone(), info, msg).unwrap_err();
        assert_eq!(ContractError::ZeroAssetAmount {}, err);
    }
}
