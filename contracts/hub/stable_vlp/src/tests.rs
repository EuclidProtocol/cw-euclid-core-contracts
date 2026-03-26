#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests {
    use crate::{
        contract::{execute, instantiate},
        state::{ADMIN, BALANCES, CHAIN_LP_TOKENS, STATE},
    };
    use cosmwasm_std::{
        coins,
        testing::{message_info, mock_dependencies, mock_env, MockQuerier},
        Addr, Decimal256, Response, Uint256, Uint64,
    };
    use euclid::{
        admin::EuclidAdmin,
        chain::ChainUid,
        cross_chain_user::CrossChainUser,
        error::ContractError,
        fee::{DenomFees, Fee, TotalFees},
        msgs::vlp::{
            base::{State, VlpRegisterPoolMsg},
            stable::msg::{ExecuteMsg, InstantiateMsg},
        },
        token::{Pair, Token},
    };
    use euclid_pool::stable_math::compute_stable_swap;
    use rstest::rstest;
    use std::collections::HashMap;

    fn init(
        deps: &mut cosmwasm_std::OwnedDeps<
            cosmwasm_std::MemoryStorage,
            cosmwasm_std::testing::MockApi,
            MockQuerier,
        >,
    ) -> Response {
        let router = deps.api.addr_make("router");
        let admin = EuclidAdmin::default(deps.api.addr_make("admin"));
        let msg = InstantiateMsg {
            router: Addr::unchecked("router"),
            virtual_balance_contract: Addr::unchecked("virtual_balance_contract"),
            pair: Pair {
                token_1: Token::create("token1".to_string()).unwrap(),
                token_2: Token::create("token2".to_string()).unwrap(),
            },
            fee: Fee::new(
                1,
                1,
                CrossChainUser::new(
                    ChainUid::create("1".to_string()).unwrap(),
                    "addr".to_string(),
                ),
            ),
            execute: None,
            admin,
            amp_factor: Some(Uint64::from(1000u64)),
        };

        let info = message_info(&router, &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap()
    }

    #[test]
    fn test_init() {
        let mut deps = mock_dependencies();
        let res = init(&mut deps);
        assert_eq!(0, res.messages.len());
        let router = deps.api.addr_make("router");
        let admin = EuclidAdmin::default(deps.api.addr_make("admin"));
        let expected_state = State {
            pair: Pair {
                token_1: Token::create("token1".to_string()).unwrap(),
                token_2: Token::create("token2".to_string()).unwrap(),
            },
            router,
            virtual_balance_contract: Addr::unchecked("virtual_balance_contract"),
            fee: Fee::new(
                1,
                1,
                CrossChainUser::new(
                    ChainUid::create("1".to_string()).unwrap(),
                    "addr".to_string(),
                ),
            ),
            total_fees_collected: TotalFees {
                lp_fees: DenomFees {
                    totals: HashMap::default(),
                },
                euclid_fees: DenomFees {
                    totals: HashMap::default(),
                },
            },
            last_updated: 0,
            total_lp_tokens: Uint256::zero(),
        };
        let state = STATE.load(&deps.storage).unwrap();
        assert_eq!(state, expected_state);
        let saved_admin = ADMIN.load(&deps.storage).unwrap();
        assert_eq!(saved_admin, admin);

        let balance_1 = BALANCES.load(&deps.storage, state.pair.token_1).unwrap();
        let expected_balance_1 = Uint256::zero();

        assert_eq!(expected_balance_1, balance_1);

        let balance_2 = BALANCES.load(&deps.storage, state.pair.token_2).unwrap();
        let expected_balance_2 = Uint256::zero();

        assert_eq!(balance_2, expected_balance_2);
    }

    #[test]
    fn test_execute_register_pool() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        init(&mut deps);

        let sender = CrossChainUser::new(
            ChainUid::create("1".to_string()).unwrap(),
            "sender_address".to_string(),
        );

        let pair = Pair {
            token_1: Token::create("token1".to_string()).unwrap(),
            token_2: Token::create("token2".to_string()).unwrap(),
        };

        let msg = ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
            sender,
            pair,
            tx_id: "1".to_string(),
        });
        let router = deps.api.addr_make("router");
        let info = message_info(&router, &coins(1000, "earth"));

        // Execute the register_pool function
        let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
        assert_eq!(res.messages.len(), 0); // Ensure no extra messages are sent

        let state = CHAIN_LP_TOKENS
            .load(&deps.storage, ChainUid::create("1".to_string()).unwrap())
            .unwrap();
        assert_eq!(state, Uint256::zero())
    }

    #[test]
    fn test_update_fee() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let msg = ExecuteMsg::UpdateFee {
            lp_fee_bps: Some(5),
            euclid_fee_bps: Some(4),
            recipient: Some(CrossChainUser::new(
                ChainUid::create("2".to_string()).unwrap(),
                "addr_2".to_string(),
            )),
        };
        let not_admin = deps.api.addr_make("not_admin");
        let info = message_info(&not_admin, &[]);

        let err = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});

        let admin = deps.api.addr_make("admin");
        let info = message_info(&admin, &[]);
        execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let fee = STATE.load(&deps.storage).unwrap().fee;
        assert_eq!(
            fee,
            Fee::new(
                5,
                4,
                CrossChainUser::new(
                    ChainUid::create("2".to_string()).unwrap(),
                    "addr_2".to_string(),
                )
            )
        );

        // Exceed max bps
        let msg = ExecuteMsg::UpdateFee {
            lp_fee_bps: Some(5000),
            euclid_fee_bps: Some(4),
            recipient: Some(CrossChainUser::new(
                ChainUid::create("2".to_string()).unwrap(),
                "addr_2".to_string(),
            )),
        };

        let err = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap_err();
        assert_eq!(
            err,
            ContractError::new("LP Fee cannot exceed maximum limit")
        );

        let msg = ExecuteMsg::UpdateFee {
            lp_fee_bps: Some(50),
            euclid_fee_bps: Some(4000),
            recipient: Some(CrossChainUser::new(
                ChainUid::create("2".to_string()).unwrap(),
                "addr_2".to_string(),
            )),
        };

        let err = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap_err();
        assert_eq!(
            err,
            ContractError::new("Euclid Fee cannot exceed maximum limit")
        );
    }
}
