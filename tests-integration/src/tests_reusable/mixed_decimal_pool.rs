#![cfg(not(target_arch = "wasm32"))]

#[cfg(test)]
mod tests {
    use crate::helpers::chains::{get_lp_token, setup_interchain, setup_router};
    use crate::helpers::relayer::relay_factory_router_factory;
    use crate::tests_reusable::constants::ROUTER_CHAIN_ID;
    use crate::tests_reusable::factory_add_liquidity::{add_liquidity, deposit_token};
    use crate::tests_reusable::factory_create_pool::create_pool;
    use crate::tests_reusable::factory_register::{setup_factory, FactorySetupMode};
    use crate::tests_reusable::factory_register_denom::register_denom;
    use crate::tests_reusable::factory_swap::swap_request;
    use crate::tests_reusable::test_macros::decimal_pair;
    use cosmwasm_std::{to_json_binary, Uint128, Uint256};
    use cw_orch::mock::MockBase;
    use cw_orch::prelude::*;
    use cw_orch_interchain::prelude::InterchainEnv;
    use euclid::chain::ChainUid;
    use euclid::cross_chain_user::CrossChainUser;
    use euclid::msgs::cross_chain_config::CrossChainConfig;
    use euclid::msgs::factory::cw20::FactoryCw20HookMsg;
    use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
    use euclid::msgs::lp_token::msg::QueryMsgFns as LpTokenQueryMsgFns;
    use euclid::msgs::vlp::base::PoolConfig;

    use euclid::swap::NextSwapPair;
    use euclid::token::{
        Pair, PairWithDenomAndAmount, Token, TokenType, TokenWithDenom, TokenWithDenomAndAmount,
    };
    use factory::FactoryContract;
    use router::RouterContract;
    use rstest::rstest;
    use rstest_reuse::apply;

    fn setup_env(
        factory_chain_id: &str,
    ) -> (
        cw_orch_interchain::mock::MockInterchainEnv,
        RouterContract<MockBase>,
        FactoryContract<MockBase>,
    ) {
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, factory_chain_id);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![factory_chain_id]).unwrap();
        let factory = setup_factory(&interchain, factory_chain_id, &router).unwrap();
        (interchain, router, factory)
    }

    #[cfg_attr(not(feature = "full_decimals"), apply(decimal_pair))]
    #[cfg_attr(feature = "full_decimals", apply(decimal_pair_full))]
    fn test_mixed_decimal_pool_e2e(
        #[values(FactorySetupMode::Native, FactorySetupMode::Ibc, FactorySetupMode::Evm)]
        mode: FactorySetupMode,
        decimals_a: u32,
        decimals_b: u32,
    ) {
        let factory_chain_id = mode.chain_id();
        let (_interchain, router, factory) = setup_env(factory_chain_id);
        let chain_uid = ChainUid::create(factory_chain_id.to_string()).unwrap();

        let token_a = TokenWithDenom {
            token: Token::create("uusdc".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "uusdc".to_string(),
                decimals: Some(decimals_a),
            },
        };
        let token_b = TokenWithDenom {
            token: Token::create("uweth".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "uweth".to_string(),
                decimals: Some(decimals_b),
            },
        };

        register_denom(&factory, &router, token_a.clone()).unwrap();
        register_denom(&factory, &router, token_b.clone()).unwrap();

        let decimal_a_multiplier = Uint256::from(10u128).pow(decimals_a);
        let decimal_b_multiplier = Uint256::from(10u128).pow(decimals_b);

        let amount_a = Uint256::from(10_000u128)
            .checked_mul(decimal_a_multiplier)
            .unwrap();
        let amount_b = Uint256::from(10_000u128)
            .checked_mul(decimal_b_multiplier)
            .unwrap();

        deposit_token(&factory, &router, token_a.clone(), amount_a, vec![]).unwrap();
        deposit_token(&factory, &router, token_b.clone(), amount_b, vec![]).unwrap();

        // Step 1: Create pool
        let pair_with_denom = PairWithDenomAndAmount {
            token_1: TokenWithDenomAndAmount {
                token: token_a.token.clone(),
                token_type: token_a.token_type.clone(),
                amount: amount_a,
            },
            token_2: TokenWithDenomAndAmount {
                token: token_b.token.clone(),
                token_type: token_b.token_type.clone(),
                amount: amount_b,
            },
        };
        create_pool(
            &factory,
            &router,
            pair_with_denom,
            500,
            PoolConfig::ConstantProduct {},
        )
        .unwrap();

        let pool_pair = Pair::new(token_a.token.clone(), token_b.token.clone()).unwrap();

        // Step 2: Add more liquidity
        let add_pair = PairWithDenomAndAmount {
            token_1: TokenWithDenomAndAmount {
                token: token_a.token.clone(),
                token_type: token_a.token_type.clone(),
                amount: Uint256::from(5_000u128)
                    .checked_mul(decimal_a_multiplier)
                    .unwrap(),
            },
            token_2: TokenWithDenomAndAmount {
                token: token_b.token.clone(),
                token_type: token_b.token_type.clone(),
                amount: Uint256::from(5_000u128)
                    .checked_mul(decimal_b_multiplier)
                    .unwrap(),
            },
        };
        add_liquidity(&factory, &router, add_pair, 500).unwrap();

        // Step 3: Swap token_a → token_b
        swap_request(
            &factory,
            &router,
            token_a.clone(),
            token_b.token.clone(),
            Uint256::from(1_000u128),
            Uint256::from(1u128),
            vec![NextSwapPair {
                token_in: token_a.token.clone(),
                token_out: token_b.token.clone(),
                pool_key: None,
                test_fail: None,
            }],
            vec![],
            None,
        )
        .unwrap();

        // Step 4: Remove liquidity
        let vlp_response = factory.get_vlp(pool_pair.clone()).unwrap();
        let lp_token_response = factory
            .get_lp_token(vlp_response.vlp_address.clone())
            .unwrap();
        let lp_token_contract =
            get_lp_token(factory.environment(), &lp_token_response.token_address);

        let lp_balance = lp_token_contract
            .balance(factory.environment().sender.clone())
            .unwrap()
            .balance;
        assert!(lp_balance > Uint128::zero(), "Should have LP tokens");

        let lp_to_remove = lp_balance / Uint128::from(2u128);
        let factory_chain_uid = &factory.get_state().unwrap().chain_uid;

        let remove_msg = FactoryCw20HookMsg::RemoveLiquidity {
            pair: pool_pair.clone(),
            recipient: CrossChainUser::new(
                chain_uid.clone(),
                factory.environment().sender.to_string(),
            ),
            cross_chain_config: CrossChainConfig::default(),
        };

        let remove_tx = lp_token_contract
            .execute(
                &euclid::msgs::lp_token::msg::ExecuteMsg::Send {
                    contract: factory.address().unwrap().to_string(),
                    amount: Uint256::from(lp_to_remove.u128()),
                    msg: to_json_binary(&remove_msg).unwrap(),
                },
                &[],
            )
            .unwrap();

        relay_factory_router_factory(remove_tx.events, &factory, &router, factory_chain_uid)
            .unwrap();

        // Verify LP balance decreased
        let lp_after = lp_token_contract
            .balance(factory.environment().sender.clone())
            .unwrap()
            .balance;
        assert_eq!(lp_after, lp_balance - lp_to_remove);
    }
}
