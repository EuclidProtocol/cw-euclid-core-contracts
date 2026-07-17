#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{Uint128, Uint256};
use cw_orch::{mock::MockBase, prelude::*};
use cw_orch_interchain::prelude::InterchainEnv;
use euclid::msgs::cross_chain_config::CrossChainConfig;
use euclid::msgs::factory::ExecuteMsg as FactoryExecuteMsg;
use euclid::msgs::factory::QueryMsgFns as FactoryQueryMsgFns;
use euclid::msgs::pool_factory::QueryMsgFns as PoolFactoryQueryMsgFns;
use euclid::msgs::vlp::base::{PoolKey, PoolType};
use euclid::token::{
    PairWithDenomAndAmount, Token, TokenType, TokenWithDenom, TokenWithDenomAndAmount,
};
use factory::FactoryContract;
use router::RouterContract;

use crate::helpers::factory::faucet;
use crate::helpers::pool_factory::setup_factory_with_pool_factory;
use crate::helpers::relayer::relay_factory_router_factory;

/// Helper to drive the delegated CLP pool creation flow end-to-end and let
/// the caller verify pool_factory's state directly (the helper used by the
/// legacy CLP tests asserts on main factory's registry, which would not be
/// populated under delegation since the ack lands on pool_factory).
fn create_clp_via_delegation(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    pair_with_denom: PairWithDenomAndAmount,
    fee_tier_bps: u64,
    tick_spacing: u64,
    slippage_tolerance_bps: u64,
    initial_tick: Option<i64>,
) -> Result<PoolKey, CwOrchError> {
    let chain = factory.environment();
    let mut funds = vec![];
    for token in pair_with_denom.get_vec_token_info() {
        faucet(
            chain,
            chain.sender.as_str(),
            Uint128::try_from(token.amount).unwrap().u128(),
            token.token_type.clone(),
            &mut funds,
        );
    }
    let tx_response = factory.execute(
        &FactoryExecuteMsg::RequestConcentratedPoolCreation {
            pair_with_denom_and_amount: pair_with_denom.clone(),
            fee_tier_bps,
            tick_spacing,
            slippage_tolerance_bps,
            initial_tick,
            cross_chain_config: CrossChainConfig::default(),
        },
        &funds,
    )?;
    let factory_chain_uid = &factory.get_state().unwrap().chain_uid;
    relay_factory_router_factory(tx_response.events, factory, router, factory_chain_uid)?;

    let pair = pair_with_denom.get_pair().unwrap();
    Ok(PoolKey {
        pair,
        pool_type: PoolType::Concentrated {
            fee_tier_bps,
            tick_spacing,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helpers::chains::{setup_interchain, setup_router};
    use crate::tests_reusable::constants::{
        FACTORY_CHAIN_ID_EVM, FACTORY_CHAIN_ID_IBC, FACTORY_CHAIN_ID_LOCAL, ROUTER_CHAIN_ID,
    };
    use crate::tests_reusable::factory_register::FactorySetupMode;
    use crate::tests_reusable::factory_register_denom::register_denom;
    use rstest::rstest;

    fn native_token(name: &str) -> TokenWithDenom {
        TokenWithDenom {
            token: Token::create(name.to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: name.to_string(),
                decimals: Some(6),
            },
        }
    }

    /// Slice 4 round-trip: with `pool_factory` wired and main factory's
    /// `POOL_FACTORY_INITIALISED == true`, a `RequestConcentratedPoolCreation`
    /// call against main factory takes the delegated path. Pool factory builds
    /// the outbound `RouterReceiveMsg::RequestConcentratedPoolCreation`
    /// packet and returns it via `Response::data`; main factory's reply
    /// handler dispatches it, and the ack lands back on pool_factory which
    /// records the VLP into `CONCENTRATED_VLPS`.
    ///
    /// Position-NFT mint and per-token escrow funding remain main-factory-side
    /// carry-overs in Slice 4 (see POOL_FACTORY_REFACTOR_ISSUES.md Slice 5/8
    /// notes); this test verifies the VLP-side wiring is end-to-end.
    // Cross-VM coverage: none (CosmWasm-only)
    #[rstest]
    #[case::native(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
    #[case::ibc(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
    #[case::evm(FactorySetupMode::Evm, FACTORY_CHAIN_ID_EVM)]
    fn test_pool_factory_clp_create_through_delegation(
        #[case] mode: FactorySetupMode,
        #[case] factory_chain_id: &str,
    ) {
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, factory_chain_id);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![factory_chain_id]).unwrap();

        let (factory, pool_factory) =
            setup_factory_with_pool_factory(&interchain, factory_chain_id, &router, mode).unwrap();

        let pool_factory_resp = factory.query_pool_factory_address().unwrap();
        assert!(
            pool_factory_resp.initialised,
            "main factory must report pool_factory as initialised"
        );

        let token_a = native_token("clpdela");
        let token_b = native_token("clpdelb");
        register_denom(&factory, &router, token_a.clone()).unwrap();
        register_denom(&factory, &router, token_b.clone()).unwrap();

        let pair = PairWithDenomAndAmount {
            token_1: TokenWithDenomAndAmount {
                token: token_a.token.clone(),
                token_type: token_a.token_type.clone(),
                amount: Uint256::from(20_000u128),
            },
            token_2: TokenWithDenomAndAmount {
                token: token_b.token.clone(),
                token_type: token_b.token_type.clone(),
                amount: Uint256::from(20_000u128),
            },
        };

        // 500-bps fee tier / 10 tick spacing — matches main factory's
        // `validate_concentrated_fee_and_spacing`.
        let pool_key =
            create_clp_via_delegation(&factory, &router, pair.clone(), 500, 10, 100, Some(0))
                .expect("delegated CLP pool creation must succeed end-to-end");
        assert!(matches!(pool_key.pool_type, PoolType::Concentrated { .. }));

        // pool_factory must record the VLP on its concentrated registry.
        let pf_vlp = pool_factory.get_concentrated_vlp(pool_key.clone()).unwrap();
        assert!(
            pf_vlp.vlp_address.is_some(),
            "pool_factory must record CLP VLP into CONCENTRATED_VLPS after ack"
        );

        // Second CLP pool with a different fee tier must succeed and register
        // distinctly — exercises the per-PoolKey-keyed CONCENTRATED_VLPS map.
        let pool_key_2 =
            create_clp_via_delegation(&factory, &router, pair, 3_000, 60, 100, Some(0))
                .expect("second delegated CLP pool creation must succeed");
        let pf_vlp_2 = pool_factory
            .get_concentrated_vlp(pool_key_2.clone())
            .unwrap();
        assert!(pf_vlp_2.vlp_address.is_some());
        assert_ne!(
            pf_vlp.vlp_address, pf_vlp_2.vlp_address,
            "different fee tiers must map to different CLP VLPs on pool_factory",
        );
    }

    /// Sanity test: with `pool_factory` not wired, the legacy in-factory CLP
    /// creation path continues to work. Guards against the new gate accidentally
    /// short-circuiting fresh chains before the bootstrap call.
    // Cross-VM coverage: none (CosmWasm-only)
    #[test]
    fn test_concentrated_pool_creation_legacy_path_when_pool_factory_not_set() {
        use crate::helpers::factory::create_concentrated_pool_with_tick;
        use crate::tests_reusable::factory_register::setup_factory;
        let sender = "sender_for_all_chains";
        let factory_chain_id = FACTORY_CHAIN_ID_LOCAL;
        let interchain = setup_interchain(sender, factory_chain_id);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![factory_chain_id]).unwrap();
        let factory = setup_factory(&interchain, factory_chain_id, &router).unwrap();

        let pool_factory_resp = factory.query_pool_factory_address().unwrap();
        assert!(!pool_factory_resp.initialised);

        let token_a = native_token("clplega");
        let token_b = native_token("clplegb");
        register_denom(&factory, &router, token_a.clone()).unwrap();
        register_denom(&factory, &router, token_b.clone()).unwrap();

        let pair = PairWithDenomAndAmount {
            token_1: TokenWithDenomAndAmount {
                token: token_a.token,
                token_type: token_a.token_type,
                amount: Uint256::from(20_000u128),
            },
            token_2: TokenWithDenomAndAmount {
                token: token_b.token,
                token_type: token_b.token_type,
                amount: Uint256::from(20_000u128),
            },
        };
        let pool_key =
            create_concentrated_pool_with_tick(&factory, &router, pair, 500, 10, 100, Some(0))
                .expect("legacy CLP pool creation should still work");
        assert!(matches!(pool_key.pool_type, PoolType::Concentrated { .. }));
        // Sanity: main factory still records the VLP on the legacy registry.
        let resp = factory.get_concentrated_vlp(PoolKey {
            pair: pool_key.pair.clone(),
            pool_type: pool_key.pool_type,
        });
        assert!(resp.is_ok());
    }
}
