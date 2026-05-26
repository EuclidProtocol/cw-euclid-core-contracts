#![cfg(not(target_arch = "wasm32"))]

//! Slice 2 integration test: CP/Stable add-liquidity end-to-end through
//! pool_factory.
//!
//! Setup model: a chain that already has a CP pool (created via the legacy
//! pre-refactor path) is upgraded to delegate add-liquidity to pool_factory.
//! The bootstrap helper instantiates pool_factory, points main factory at it,
//! and replays the existing PAIR_TO_VLP / VLP_TO_LP_TOKEN entries into
//! pool_factory via `MigrateAcceptPoolState`. From that point on,
//! `factory.add_liquidity()` takes the delegated path:
//!
//! 1. main factory deposits funds to escrow up-front,
//! 2. delegates to `pool_factory::OnAddLiquidity`,
//! 3. pool_factory builds the outbound packet and returns it via
//!    `Response::data`; main factory's reply handler dispatches it,
//! 4. ack returns to main factory, forwarded to `pool_factory::OnPoolAck`,
//! 5. on success pool_factory issues `ProxyMintLpToken`; on failure it
//!    issues `ProxyReleaseEscrow` per non-voucher token.

#[cfg(test)]
mod tests {
    use cosmwasm_std::{Uint128, Uint256};
    use cw_orch::prelude::*;
    use cw_orch_interchain::prelude::InterchainEnv;
    use euclid::msgs::cross_chain_config::CrossChainConfig;
    use euclid::msgs::escrow::QueryMsgFns as EscrowQueryMsgFns;
    use euclid::msgs::factory::ExecuteMsgFns as FactoryExecuteMsgFns;
    use euclid::msgs::factory::QueryMsgFns as FactoryQueryMsgFns;
    use euclid::msgs::lp_token::msg::QueryMsgFns as LpTokenQueryMsgFns;
    use euclid::msgs::pool_factory::QueryMsgFns as PoolFactoryQueryMsgFns;
    use euclid::msgs::vlp::base::PoolConfig;
    use euclid::token::{
        PairWithDenomAndAmount, Token, TokenType, TokenWithDenom, TokenWithDenomAndAmount,
    };
    use rstest::rstest;

    use crate::helpers::chains::{get_escrow, get_lp_token, setup_interchain, setup_router};
    use crate::helpers::factory::faucet;
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

    /// Happy path: create a CP pool via legacy, upgrade to pool_factory
    /// delegation, then add liquidity through the delegated flow. Asserts
    /// the delegated path completes end-to-end across all three chain modes.
    #[rstest]
    #[case::native(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
    #[case::ibc(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
    #[case::evm(FactorySetupMode::Evm, FACTORY_CHAIN_ID_EVM)]
    fn test_pool_factory_cp_add_liquidity_through_delegation(
        #[case] mode: FactorySetupMode,
        #[case] factory_chain_id: &str,
    ) {
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, factory_chain_id);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![factory_chain_id]).unwrap();

        // Phase 1: legacy factory bootstrap — pool_factory not yet wired.
        let factory =
            setup_factory(&interchain, factory_chain_id, &router).expect("setup main factory");
        let _ = mode;

        // Register denoms + create a CP pool through the legacy path so main
        // factory ends up with a fully-populated pool registry and an LP
        // cw20 token deployed.
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

        // Phase 2: deploy pool_factory, point main factory at it, and
        // replay the existing pool registry into pool_factory.
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

        // Sanity: both surfaces report the pool now.
        assert!(factory.query_pool_factory_address().unwrap().initialised);
        let pf_vlp = pool_factory.get_vlp(pair.clone()).unwrap();
        assert!(
            pf_vlp.vlp_address.is_some(),
            "pool_factory must have VLP after migration"
        );

        // Phase 3: add liquidity through main factory's delegated path.
        // Snapshot escrow + LP balances before so we can verify the deltas.
        let escrow_a = get_escrow(&factory, token_a.token.as_str());
        let escrow_b = get_escrow(&factory, token_b.token.as_str());
        let before_escrow_a = escrow_a.state().unwrap().total_amount;
        let before_escrow_b = escrow_b.state().unwrap().total_amount;

        let lp_token_addr = factory
            .get_lp_token(pf_vlp.vlp_address.clone().unwrap())
            .unwrap()
            .token_address;
        let lp_token_contract = get_lp_token(factory.environment(), &lp_token_addr);
        let user_addr = factory.environment().sender.to_string();
        let before_lp = lp_token_contract
            .balance(user_addr.clone())
            .unwrap()
            .balance;

        let add_pair = make_pair(&token_a, &token_b, 1_000);
        let mut funds = vec![];
        for token in add_pair.get_vec_token_info() {
            faucet(
                factory.environment(),
                factory.environment().sender.as_str(),
                Uint128::try_from(token.amount).unwrap().u128(),
                token.token_type.clone(),
                &mut funds,
            );
        }
        let tx_response = factory
            .execute(
                &euclid::msgs::factory::ExecuteMsg::AddLiquidity {
                    pair_with_denom_and_amount: add_pair.clone(),
                    slippage_tolerance_bps: 500,
                    cross_chain_config: CrossChainConfig::default(),
                },
                &funds,
            )
            .expect("delegated add_liquidity must succeed up to dispatch");

        // The delegated dispatch emits a `method=add_liquidity_request_delegated`
        // attribute on main factory — verify the gate flipped before relay.
        let saw_delegated = tx_response.events.iter().any(|ev| {
            ev.attributes
                .iter()
                .any(|attr| attr.key == "method" && attr.value == "add_liquidity_request_delegated")
        });
        assert!(
            saw_delegated,
            "expected add_liquidity to take the delegated path post-migration"
        );

        let factory_chain_uid = &factory.get_state().unwrap().chain_uid;
        relay_factory_router_factory(tx_response.events, &factory, &router, factory_chain_uid)
            .expect("relay round-trip");

        // Post-ack assertions: escrow balances increased by the added liquidity
        // (deposit happened up-front via main factory) and the user's LP
        // balance increased (mint happened via `ProxyMintLpToken`).
        let after_escrow_a = escrow_a.state().unwrap().total_amount;
        let after_escrow_b = escrow_b.state().unwrap().total_amount;
        assert_eq!(
            after_escrow_a - before_escrow_a,
            Uint256::from(1_000u128),
            "escrow A balance should grow by added liquidity"
        );
        assert_eq!(
            after_escrow_b - before_escrow_b,
            Uint256::from(1_000u128),
            "escrow B balance should grow by added liquidity"
        );
        let after_lp = lp_token_contract.balance(user_addr).unwrap().balance;
        assert!(
            after_lp > before_lp,
            "user LP balance must increase after delegated add_liquidity"
        );
    }

    /// Slippage-exceeded path: a skewed add-liquidity request should be
    /// rejected by the hub. On the IBC/EVM path the failure surfaces via an
    /// error ack, and pool_factory's `OnPoolAck` failure branch refunds the
    /// user by emitting `ProxyReleaseEscrow` for each non-voucher token.
    #[rstest]
    #[case::ibc(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
    #[case::evm(FactorySetupMode::Evm, FACTORY_CHAIN_ID_EVM)]
    fn test_pool_factory_cp_add_liquidity_slippage_refund(
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

        let chain = interchain.get_chain(factory_chain_id).unwrap();
        let pool_factory = pool_factory::PoolFactoryContract::new(chain.clone());
        pool_factory.upload().unwrap();
        pool_factory
            .instantiate(
                &euclid::msgs::pool_factory::InstantiateMsg {
                    main_factory_address: factory.address().unwrap().to_string(),
                },
                None,
                &[],
            )
            .unwrap();
        factory
            .set_pool_factory(pool_factory.address().unwrap().to_string())
            .unwrap();
        let pair = create_pair.get_pair().unwrap();
        migrate_pool_state_to_pool_factory(&factory, &pool_factory, &[pair.clone()]).unwrap();

        // Snapshot escrow balances before the skewed request.
        let escrow_a = get_escrow(&factory, token_a.token.as_str());
        let escrow_b = get_escrow(&factory, token_b.token.as_str());
        let before_escrow_a = escrow_a.state().unwrap().total_amount;
        let before_escrow_b = escrow_b.state().unwrap().total_amount;

        // Build a skewed add-liquidity payload that the hub will reject for
        // slippage. amount_a/amount_b ratio diverges from the pool's 1:1.
        let skewed = PairWithDenomAndAmount {
            token_1: TokenWithDenomAndAmount {
                token: token_a.token.clone(),
                token_type: token_a.token_type.clone(),
                amount: Uint256::from(1_000u128),
            },
            token_2: TokenWithDenomAndAmount {
                token: token_b.token.clone(),
                token_type: token_b.token_type.clone(),
                amount: Uint256::from(5_000u128),
            },
        };
        let mut funds = vec![];
        for token in skewed.get_vec_token_info() {
            faucet(
                factory.environment(),
                factory.environment().sender.as_str(),
                Uint128::try_from(token.amount).unwrap().u128(),
                token.token_type.clone(),
                &mut funds,
            );
        }
        let tx_response = factory
            .execute(
                &euclid::msgs::factory::ExecuteMsg::AddLiquidity {
                    pair_with_denom_and_amount: skewed,
                    slippage_tolerance_bps: 100,
                    cross_chain_config: CrossChainConfig::default(),
                },
                &funds,
            )
            .expect("delegated add_liquidity dispatch");
        let factory_chain_uid = &factory.get_state().unwrap().chain_uid;
        let _ =
            relay_factory_router_factory(tx_response.events, &factory, &router, factory_chain_uid);

        // Refund happened via ProxyReleaseEscrow: escrow totals returned to
        // their pre-request values.
        let after_escrow_a = escrow_a.state().unwrap().total_amount;
        let after_escrow_b = escrow_b.state().unwrap().total_amount;
        assert_eq!(
            after_escrow_a, before_escrow_a,
            "escrow A balance must net to zero after slippage refund"
        );
        assert_eq!(
            after_escrow_b, before_escrow_b,
            "escrow B balance must net to zero after slippage refund"
        );
    }
}
