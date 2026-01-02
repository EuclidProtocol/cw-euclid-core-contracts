#[cfg(test)]
mod tests {
    use crate::{
        chain::{ChainUid, CrossChainUser},
        fee::{DenomFees, Fee, TotalFees},
        pool::{
            add_liquidity, calculate_amount_from_shares, calculate_lp_allocation, register_pool,
            remove_liquidity, update_fee, update_state, State,
        },
        token::{Pair, PairWithAmount, Token},
    };
    use cosmwasm_std::testing::{message_info, mock_dependencies, mock_env};
    use cosmwasm_std::Uint128;
    use cw_storage_plus::{Item, Map};
    use std::collections::HashMap;

    #[test]
    fn test_calculate_lp_allocation() {
        // Test initial liquidity provision (empty pool)
        let token_1_amount = Uint128::new(1000);
        let token_2_amount = Uint128::new(1000);
        let total_liquidity_1 = Uint128::zero();
        let total_liquidity_2 = Uint128::zero();
        let total_lp_supply = Uint128::zero();

        let lp_tokens = calculate_lp_allocation(
            token_1_amount,
            token_2_amount,
            total_liquidity_1,
            total_liquidity_2,
            total_lp_supply,
        )
        .unwrap();

        // For initial provision, LP tokens should be sqrt(token1 * token2) - MINIMUM_LIQUIDITY
        assert_eq!(lp_tokens, Uint128::new(1000));

        let amount_1 = calculate_amount_from_shares(
            token_1_amount + total_liquidity_1,
            lp_tokens,
            total_lp_supply + lp_tokens,
        )
        .unwrap();
        assert_eq!(
            amount_1, token_1_amount,
            "Token 1 amount from shares is not correct"
        );

        let amount_2 = calculate_amount_from_shares(
            token_2_amount + total_liquidity_2,
            lp_tokens,
            total_lp_supply + lp_tokens,
        )
        .unwrap();
        assert_eq!(
            amount_2, token_2_amount,
            "Token 2 amount from shares is not correct"
        );

        // Test subsequent liquidity provision
        let token_1_amount = Uint128::new(100);
        let token_2_amount = Uint128::new(100);
        let total_liquidity_1 = Uint128::new(1000);
        let total_liquidity_2 = Uint128::new(1000);
        let total_lp_supply = Uint128::new(1000); // From previous provision

        let lp_tokens = calculate_lp_allocation(
            token_1_amount,
            token_2_amount,
            total_liquidity_1,
            total_liquidity_2,
            total_lp_supply,
        )
        .unwrap();

        // For subsequent provisions, LP tokens should be proportional to liquidity added
        assert_eq!(lp_tokens, Uint128::new(100));

        let amount_1 = calculate_amount_from_shares(
            token_1_amount + total_liquidity_1,
            lp_tokens,
            total_lp_supply + lp_tokens,
        )
        .unwrap();

        assert_eq!(
            amount_1, token_1_amount,
            "Token 1 amount from shares is not correct"
        );
        let amount_2 = calculate_amount_from_shares(
            token_2_amount + total_liquidity_2,
            lp_tokens,
            total_lp_supply + lp_tokens,
        )
        .unwrap();
        assert_eq!(
            amount_2, token_2_amount,
            "Token 2 amount from shares is not correct"
        );

        // Test uneven liquidity provision
        let token_1_amount = Uint128::new(200);
        let token_2_amount = Uint128::new(100);
        let total_liquidity_1 = Uint128::new(2000);
        let total_liquidity_2 = Uint128::new(1000);
        let total_lp_supply = Uint128::new(1990);

        let lp_tokens = calculate_lp_allocation(
            token_1_amount,
            token_2_amount,
            total_liquidity_1,
            total_liquidity_2,
            total_lp_supply,
        )
        .unwrap();

        // Should use the minimum ratio to prevent dilution
        assert_eq!(lp_tokens, Uint128::new(199));

        let amount_1 = calculate_amount_from_shares(
            token_1_amount + total_liquidity_1,
            lp_tokens,
            total_lp_supply + lp_tokens,
        )
        .unwrap();
        assert_eq!(
            amount_1, token_1_amount,
            "Token 1 amount from shares is not correct"
        );

        let amount_2 = calculate_amount_from_shares(
            token_2_amount + total_liquidity_2,
            lp_tokens,
            total_lp_supply + lp_tokens,
        )
        .unwrap();
        assert_eq!(
            amount_2, token_2_amount,
            "Token 2 amount from shares is not correct"
        );
    }

    #[test]
    fn test_big_token_amount() {
        // Test with large token amounts
        let token_1_amount = Uint128::new(5_000_000_000_000_000_000_000_000);
        let token_2_amount = Uint128::new(5_000_000_000_000_000_000_000_000);
        let total_liquidity_1 = Uint128::new(0);
        let total_liquidity_2 = Uint128::new(0);
        let total_lp_supply = Uint128::new(0);

        let lp_tokens = calculate_lp_allocation(
            token_1_amount,
            token_2_amount,
            total_liquidity_1,
            total_liquidity_2,
            total_lp_supply,
        )
        .unwrap();

        // For initial provision with large amounts
        assert_eq!(lp_tokens, Uint128::new(5_000_000_000_000_000_000_000_000));

        // Test subsequent large liquidity provision
        let token_1_amount = Uint128::new(5_000_000_000_000_000_000_000_000);
        let token_2_amount = Uint128::new(5_000_000_000_000_000_000_000_000);
        let total_liquidity_1 = Uint128::new(5_000_000_000_000_000_000_000_000);
        let total_liquidity_2 = Uint128::new(5_000_000_000_000_000_000_000_000);
        let total_lp_supply = Uint128::new(5_000_000_000_000_000_000_000_000);

        let lp_tokens = calculate_lp_allocation(
            token_1_amount,
            token_2_amount,
            total_liquidity_1,
            total_liquidity_2,
            total_lp_supply,
        )
        .unwrap();

        // For subsequent provisions with large amounts
        assert_eq!(lp_tokens, Uint128::new(5_000_000_000_000_000_000_000_000));
    }

    #[test]
    fn test_paused_blocks_actions_except_update_state() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let router = deps.api.addr_make("router");
        let admin = deps.api.addr_make("admin");

        let state_storage: Item<State> = Item::new("state");
        let chain_lp_tokens: Map<ChainUid, Uint128> = Map::new("chain_lp_tokens");
        let balances: Map<Token, Uint128> = Map::new("balances");
        let collateral_lp_tokens: Item<Uint128> = Item::new("collateral_lp_tokens");

        let pair = Pair {
            token_1: Token::create("token1".to_string()).unwrap(),
            token_2: Token::create("token2".to_string()).unwrap(),
        };

        let state = State {
            pair: pair.clone(),
            router: router.to_string(),
            virtual_balance: "vb".to_string(),
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
            last_updated: env.block.time.seconds(),
            total_lp_tokens: Uint128::zero(),
            paused: true,
            admin: admin.to_string(),
        };

        state_storage.save(deps.as_mut().storage, &state).unwrap();

        let sender = CrossChainUser::new(
            ChainUid::create("1".to_string()).unwrap(),
            "sender_address".to_string(),
        );

        // register_pool should fail when paused
        let err = register_pool(
            deps.as_mut(),
            env.clone(),
            message_info(&router, &[]),
            &state_storage,
            &chain_lp_tokens,
            None,
            sender.clone(),
            pair.clone(),
            "tx".to_string(),
        )
        .unwrap_err();
        assert_eq!(err, crate::error::ContractError::ContractPaused {});

        // add_liquidity should fail when paused
        let liquidity = PairWithAmount::new(
            pair.token_1.with_amount(Uint128::new(100)),
            pair.token_2.with_amount(Uint128::new(100)),
        )
        .unwrap();

        let err = add_liquidity(
            deps.as_mut(),
            env.clone(),
            message_info(&router, &[]),
            &state_storage,
            &balances,
            &chain_lp_tokens,
            &collateral_lp_tokens,
            sender.clone(),
            liquidity,
            10,
            "tx_add".to_string(),
        )
        .unwrap_err();
        assert_eq!(err, crate::error::ContractError::ContractPaused {});

        // remove_liquidity should fail when paused
        let err = remove_liquidity(
            deps.as_mut(),
            env.clone(),
            message_info(&router, &[]),
            &state_storage,
            &balances,
            &chain_lp_tokens,
            sender.clone(),
            Uint128::new(50),
            "tx_remove".to_string(),
        )
        .unwrap_err();
        assert_eq!(err, crate::error::ContractError::ContractPaused {});

        // update_fee should fail when paused
        let err = update_fee(
            deps.as_mut(),
            message_info(&admin, &[]),
            &state_storage,
            Some(2),
            Some(2),
            Some(sender.clone()),
        )
        .unwrap_err();
        assert_eq!(err, crate::error::ContractError::ContractPaused {});

        // update_state should remain allowed even when paused
        update_state(
            deps.as_mut(),
            message_info(&admin, &[]),
            &state_storage,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(false),
        )
        .unwrap();

        let updated = state_storage.load(&deps.storage).unwrap();
        assert!(!updated.paused);
    }
}
