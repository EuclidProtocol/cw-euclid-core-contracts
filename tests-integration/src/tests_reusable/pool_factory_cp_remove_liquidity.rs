#![cfg(not(target_arch = "wasm32"))]

//! Slice 3 integration test: CP/Stable remove-liquidity end-to-end through
//! pool_factory.
//!
//! Setup model: legacy CP pool creation seeds main factory's pool registry
//! and mints initial LP tokens to the user. The bootstrap helper then
//! deploys pool_factory, points main factory at it, and replays the existing
//! `PAIR_TO_VLP` / `VLP_TO_LP_TOKEN` entries via `MigrateAcceptPoolState`.
//! From that point on, the user-initiated `cw20::Send` against the LP token
//! takes the delegated path:
//!
//! 1. main factory's `FactoryCw20HookMsg::RemoveLiquidity` validates the
//!    request, holds the LP tokens (they arrived via the cw20 hook),
//! 2. delegates to `pool_factory::OnRemoveLiquidity`,
//! 3. pool_factory builds the outbound `RouterReceiveMsg::RemoveLiquidity`
//!    packet and returns it via `Response::data`; main factory's reply
//!    handler dispatches it,
//! 4. ack returns to main factory, forwarded to `pool_factory::OnPoolAck`,
//! 5. on success pool_factory issues `ProxyBurnLpToken`; on failure it
//!    issues `ProxyTransferLpToken` to return the LP back to the user.

#[cfg(test)]
mod tests {
    use cosmwasm_std::{to_json_binary, Uint128, Uint256};
    use cw_orch::prelude::*;
    use cw_orch_interchain::prelude::InterchainEnv;
    use euclid::cross_chain_user::CrossChainUser;
    use euclid::msgs::cross_chain_config::CrossChainConfig;
    use euclid::msgs::factory::cw20::FactoryCw20HookMsg;
    use euclid::msgs::factory::ExecuteMsgFns as FactoryExecuteMsgFns;
    use euclid::msgs::factory::QueryMsgFns as FactoryQueryMsgFns;
    use euclid::msgs::lp_token::msg::QueryMsgFns as LpTokenQueryMsgFns;
    use euclid::msgs::vlp::base::PoolConfig;
    use euclid::token::{
        PairWithDenomAndAmount, Token, TokenType, TokenWithDenom, TokenWithDenomAndAmount,
    };
    use rstest::rstest;

    use crate::helpers::chains::{get_lp_token, setup_interchain, setup_router};
    use crate::helpers::pool_factory::migrate_pool_state_to_pool_factory;
    use crate::helpers::relayer::relay_factory_router_factory;
    use crate::tests_reusable::constants::{
        FACTORY_CHAIN_ID_EVM, FACTORY_CHAIN_ID_IBC, FACTORY_CHAIN_ID_LOCAL, ROUTER_CHAIN_ID,
    };
    use crate::tests_reusable::factory_create_pool::create_pool;
    use crate::tests_reusable::factory_register::{setup_factory, FactorySetupMode};
    use crate::tests_reusable::factory_register_denom::register_denom;

    fn make_token(name: &str) -> TokenWithDenom {
        TokenWithDenom {
            token: Token::create(name.to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: name.to_string(),
                decimals: Some(6),
            },
        }
    }

    fn make_pair(
        token_a: &TokenWithDenom,
        token_b: &TokenWithDenom,
        amount: u128,
    ) -> PairWithDenomAndAmount {
        PairWithDenomAndAmount {
            token_1: TokenWithDenomAndAmount {
                token: token_a.token.clone(),
                token_type: token_a.token_type.clone(),
                amount: Uint256::from(amount),
            },
            token_2: TokenWithDenomAndAmount {
                token: token_b.token.clone(),
                token_type: token_b.token_type.clone(),
                amount: Uint256::from(amount),
            },
        }
    }

    /// Happy path: create a CP pool (which mints initial LP to the user) via
    /// legacy, upgrade to pool_factory delegation, then remove liquidity
    /// through the delegated flow. Asserts LP balance decreases by exactly
    /// the amount sent in the cw20 hook.
    // Cross-VM coverage: none (CosmWasm-only)
    #[rstest]
    #[case::native(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
    #[case::ibc(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
    #[case::evm(FactorySetupMode::Evm, FACTORY_CHAIN_ID_EVM)]
    fn test_pool_factory_cp_remove_liquidity_through_delegation(
        #[case] mode: FactorySetupMode,
        #[case] factory_chain_id: &str,
    ) {
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, factory_chain_id);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![factory_chain_id]).unwrap();

        let factory =
            setup_factory(&interchain, factory_chain_id, &router).expect("setup main factory");
        let _ = mode;

        // Legacy CP pool creation mints initial LP tokens to the user.
        let token_a = make_token("tokena");
        let token_b = make_token("tokenb");
        register_denom(&factory, &router, token_a.clone()).unwrap();
        register_denom(&factory, &router, token_b.clone()).unwrap();
        let create_pair = make_pair(&token_a, &token_b, 10_000);
        create_pool(
            &factory,
            &router,
            create_pair.clone(),
            500,
            PoolConfig::ConstantProduct {},
        )
        .expect("legacy pool creation");

        // Bootstrap pool_factory and migrate the pool registry.
        let chain = interchain.get_chain(factory_chain_id).unwrap();
        let pool_factory = pool_factory::PoolFactoryContract::new(chain.clone());
        pool_factory.upload().expect("upload pool_factory");
        pool_factory
            .instantiate(
                &euclid::msgs::pool_factory::InstantiateMsg {
                    main_factory_address: factory.address().unwrap().to_string(),
                },
                None,
                &[],
            )
            .expect("instantiate pool_factory");
        factory
            .set_pool_factory(pool_factory.address().unwrap().to_string())
            .expect("set_pool_factory");
        let pair = create_pair.get_pair().unwrap();
        migrate_pool_state_to_pool_factory(&factory, &pool_factory, &[pair.clone()])
            .expect("migrate pool state");

        // Snapshot LP balance prior to removal.
        let factory_chain_uid = &factory.get_state().unwrap().chain_uid;
        let vlp_address = factory.get_vlp(pair.clone()).unwrap().vlp_address;
        let lp_token_address = factory.get_lp_token(vlp_address).unwrap().token_address;
        let lp_token_contract = get_lp_token(factory.environment(), &lp_token_address);
        let user_addr = factory.environment().sender.clone();
        let before_lp = lp_token_contract
            .balance(user_addr.to_string())
            .unwrap()
            .balance;
        assert!(
            before_lp > Uint128::zero(),
            "legacy pool creation must mint initial LP to user"
        );

        let lp_to_remove = before_lp / Uint128::from(2u128);
        let remove_msg = FactoryCw20HookMsg::RemoveLiquidity {
            pair: pair.clone(),
            recipient: CrossChainUser::new(factory_chain_uid.clone(), user_addr.to_string()),
            cross_chain_config: CrossChainConfig::default(),
        };
        let send_tx = lp_token_contract
            .execute(
                &euclid::msgs::lp_token::msg::ExecuteMsg::Send {
                    contract: factory.address().unwrap().to_string(),
                    amount: Uint256::from(lp_to_remove.u128()),
                    msg: to_json_binary(&remove_msg).unwrap(),
                },
                &[],
            )
            .expect("delegated remove_liquidity dispatch");

        // Verify the delegated path was taken — `method=remove_liquidity_request_delegated`.
        let saw_delegated = send_tx.events.iter().any(|ev| {
            ev.attributes.iter().any(|attr| {
                attr.key == "method" && attr.value == "remove_liquidity_request_delegated"
            })
        });
        assert!(
            saw_delegated,
            "expected remove_liquidity to take the delegated path post-migration"
        );

        relay_factory_router_factory(send_tx.events, &factory, &router, factory_chain_uid)
            .expect("relay round-trip");

        // On success the LP balance decreases by exactly `lp_to_remove`
        // (ProxyBurnLpToken burns from main factory's holding).
        let after_lp = lp_token_contract
            .balance(user_addr.to_string())
            .unwrap()
            .balance;
        assert_eq!(
            after_lp,
            before_lp - lp_to_remove,
            "user LP balance must decrease by the removed amount"
        );
    }
}
