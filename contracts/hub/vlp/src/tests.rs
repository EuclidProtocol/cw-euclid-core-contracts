#[cfg(test)]
mod tests {

    use std::collections::HashMap;

    use crate::contract::{execute, instantiate};

    use crate::state::{State, BALANCES, CHAIN_LP_TOKENS, STATE};

    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{coins, DepsMut, Response, Uint128};
    use euclid::chain::{ChainUid, CrossChainUser};

    use euclid::error::ContractError;
    use euclid::fee::{DenomFees, Fee, TotalFees};
    use euclid::msgs::vlp::{ExecuteMsg, InstantiateMsg};

    use euclid::token::{Pair, Token};

    fn init(deps: DepsMut) -> Response {
        let msg = InstantiateMsg {
            router: "router".to_string(),
            virtual_balance: "virtual_balance".to_string(),
            pair: Pair {
                token_1: Token::create("token1".to_string()).unwrap(),
                token_2: Token::create("token2".to_string()).unwrap(),
            },
            fee: Fee {
                lp_fee_bps: 1,
                euclid_fee_bps: 1,
                recipient: CrossChainUser {
                    chain_uid: ChainUid::create("1".to_string()).unwrap(),
                    address: "addr".to_string(),
                },
            },
            execute: None,
            admin: "admin".to_string(),
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
            pair: Pair {
                token_1: Token::create("token1".to_string()).unwrap(),
                token_2: Token::create("token2".to_string()).unwrap(),
            },
            router: "router".to_string(),
            virtual_balance: "virtual_balance".to_string(),
            fee: Fee {
                lp_fee_bps: 1,
                euclid_fee_bps: 1,
                recipient: CrossChainUser {
                    chain_uid: ChainUid::create("1".to_string()).unwrap(),
                    address: "addr".to_string(),
                },
            },
            total_fees_collected: TotalFees {
                lp_fees: DenomFees {
                    totals: HashMap::default(),
                },
                euclid_fees: DenomFees {
                    totals: HashMap::default(),
                },
            },
            last_updated: 0,
            total_lp_tokens: Uint128::zero(),
            admin: "admin".to_string(),
        };
        let state = STATE.load(&deps.storage).unwrap();
        assert_eq!(state, expected_state);

        let balance_1 = BALANCES.load(&deps.storage, state.pair.token_1).unwrap();
        let expected_balance_1 = Uint128::zero();

        assert_eq!(expected_balance_1, balance_1);

        let balance_2 = BALANCES.load(&deps.storage, state.pair.token_2).unwrap();
        let expected_balance_2 = Uint128::zero();

        assert_eq!(balance_2, expected_balance_2);
    }

    #[test]
    fn test_execute_register_pool() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        init(deps.as_mut());

        let sender = CrossChainUser {
            chain_uid: ChainUid::create("1".to_string()).unwrap(),
            address: "sender_address".to_string(),
        };

        let pair = Pair {
            token_1: Token::create("token1".to_string()).unwrap(),
            token_2: Token::create("token2".to_string()).unwrap(),
        };

        let msg = ExecuteMsg::RegisterPool {
            sender,
            pair,
            tx_id: "1".to_string(),
        };
        let info = mock_info("router", &coins(1000, "earth"));

        // Execute the register_pool function
        let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
        assert_eq!(res.messages.len(), 0); // Ensure no extra messages are sent

        let state = CHAIN_LP_TOKENS.load(&deps.storage, ChainUid::create("1".to_string()).unwrap()).unwrap();
        assert_eq!(state, Uint128::zero())
    }

    #[test]
    fn test_update_fee() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(deps.as_mut());

        let msg = ExecuteMsg::UpdateFee { lp_fee_bps: Some(5), euclid_fee_bps: Some(4), recipient: Some(CrossChainUser {
            chain_uid: ChainUid::create("2".to_string()).unwrap(),
            address: "addr_2".to_string(),
        }) };
        let info = mock_info("not_admin", &[]);

        let err = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {  });

        let info = mock_info("admin", &[]);
        execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let fee = STATE.load(&deps.storage).unwrap().fee;
        assert_eq!(fee, Fee { lp_fee_bps: 5, euclid_fee_bps: 4, recipient: CrossChainUser {
            chain_uid: ChainUid::create("2".to_string()).unwrap(),
            address: "addr_2".to_string(),
        } });

        // Exceed max bps
        let msg = ExecuteMsg::UpdateFee { lp_fee_bps: Some(5000), euclid_fee_bps: Some(4), recipient: Some(CrossChainUser {
            chain_uid: ChainUid::create("2".to_string()).unwrap(),
            address: "addr_2".to_string(),
        }) };

        let err = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap_err();
        assert_eq!(err,  ContractError::new("LP Fee cannot exceed maximum limit"));

        let msg = ExecuteMsg::UpdateFee { lp_fee_bps: Some(50), euclid_fee_bps: Some(4000), recipient: Some(CrossChainUser {
            chain_uid: ChainUid::create("2".to_string()).unwrap(),
            address: "addr_2".to_string(),
        }) };

        let err = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap_err();
        assert_eq!(err,  ContractError::new("Euclid Fee cannot exceed maximum limit"));
    }
}
