#[cfg(test)]
mod tests {

    use crate::contract::{execute, instantiate};
    use crate::state::{BALANCES, STATE};

    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{Addr, DepsMut, Response, Uint128};
    use euclid::chain::{ChainUid, CrossChainUser};
    use euclid::error::ContractError;
    use euclid::msgs::vcoin::{
        ExecuteBurn, ExecuteMint, ExecuteMsg, ExecuteTransfer, InstantiateMsg, State,
    };
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
    fn test_mint_burn_transfer() {
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
            cross_chain_user: cross_chain_user.clone(),
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
        let snapshot_balance = BALANCES.load(&deps.storage, key).unwrap();

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
        let snapshot_balance = BALANCES.load(&deps.storage, key).unwrap();

        assert_eq!(expected_snapshot_balance, snapshot_balance);

        // Zero burn amount
        let msg = ExecuteMsg::Burn(ExecuteBurn {
            amount: Uint128::zero(),
            balance_key: balance_key.clone(),
        });

        let err = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap_err();
        assert_eq!(ContractError::ZeroAssetAmount {}, err);

        // Transfer //

        let cross_chain_user_2 = CrossChainUser {
            chain_uid: ChainUid::create("1".to_string()).unwrap(),
            address: "cross_chain_user_address_2".to_string(),
        };

        let balance_key_2 = BalanceKey {
            cross_chain_user: cross_chain_user_2.clone(),
            token_id: "token1".to_string(),
        };

        let msg = ExecuteMsg::Transfer(ExecuteTransfer {
            amount: Uint128::new(2_u128),
            token_id: "token1".to_string(),
            from: cross_chain_user,
            to: cross_chain_user_2,
        });

        let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
        let expected_snapshot_balance_user_1 = Uint128::new(3_u128);
        let expected_snapshot_balance_user_2 = Uint128::new(2_u128);

        let key = balance_key.clone().to_serialized_balance_key();
        let key_2 = balance_key_2.clone().to_serialized_balance_key();

        let snapshot_balance = BALANCES.load(&deps.storage, key).unwrap();
        let snapshot_balance_2 = BALANCES.load(&deps.storage, key_2).unwrap();

        assert_eq!(expected_snapshot_balance_user_1, snapshot_balance);
        assert_eq!(expected_snapshot_balance_user_2, snapshot_balance_2);
    }
}
