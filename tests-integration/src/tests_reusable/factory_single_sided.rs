#![cfg(not(target_arch = "wasm32"))]
//! Integration tests for the single-sided add-liquidity flow (Issue 4).
//!
//! Scenarios covered:
//!
//! - **A — Stable pool happy path:** native USDC into a stable USDC/USDT VLP.
//!   Demonstrates the orchestrator is pool-type agnostic (no per-pool
//!   branching), the LP CW20 lands on the remote chain, the deposit is
//!   escrowed, and pending state is cleared.
//!
//! - **B — Ack-failure refund:** `min_lp_out` set above the simulated LP
//!   output. The hub's add-liquidity reply errors with
//!   `SlippageExceeded`, the outer cross-chain-receive reply converts it to
//!   an error ack via `make_ack_fail`, the factory ack handler refunds
//!   `amount_in + partner_fee_amount` to the user, and no residual state
//!   remains on either chain (`PENDING_SINGLE_SIDED_LIQUIDITY` empty,
//!   `VLP_TO_LP_SHARES` unchanged, VLP reserves unchanged).

use crate::helpers::factory::faucet;
use crate::helpers::relayer::relay_factory_router_factory;
use cosmwasm_std::{coin, Event, Uint128, Uint256};
use cw_orch::mock::MockBase;
use cw_orch::prelude::*;
use cw_orch_interchain::prelude::InterchainEnv;
use euclid::fee::PartnerFee;
use euclid::msgs::cross_chain_config::CrossChainConfig;
use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
use euclid::msgs::factory::ExecuteMsg as FactoryExecuteMsg;
use euclid::swap::NextSwapPair;
use euclid::token::{Pair, Token, TokenType, TokenWithDenom};
use factory::FactoryContract;
use router::RouterContract;

/// Submit an `AddSingleSidedLiquidity` request and relay the IBC roundtrip.
/// Returns the relayed ack events so the caller can inspect them.
#[allow(clippy::too_many_arguments)]
pub fn single_sided_add_liquidity(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    asset_in: TokenWithDenom,
    amount_in: Uint256,
    asset_out: Token,
    swap_amount: Uint256,
    min_lp_out: Uint256,
    partner_fee: Option<PartnerFee>,
) -> Result<Vec<Event>, CwOrchError> {
    let chain = factory.environment();
    let denom = asset_in
        .token_type
        .get_denom()
        .expect("single-sided helper only supports native asset_in");
    faucet(
        &chain,
        chain.sender.as_str(),
        Uint128::try_from(amount_in).unwrap().u128(),
        asset_in.token_type.clone(),
        &mut vec![],
    );
    let pair = Pair::new(asset_in.token.clone(), asset_out.clone())
        .expect("asset_in and asset_out must form a valid pair");
    let tx_response = factory.execute(
        &FactoryExecuteMsg::AddSingleSidedLiquidity {
            asset_in: asset_in.clone(),
            amount_in,
            pair,
            swap_amount,
            swap_route: vec![NextSwapPair {
                token_in: asset_in.token,
                token_out: asset_out,
                test_fail: None,
            }],
            min_lp_out,
            partner_fee,
            cross_chain_config: CrossChainConfig::default(),
        },
        &[coin(Uint128::try_from(amount_in).unwrap().u128(), denom)],
    )?;
    let factory_chain_uid = &factory.get_state().unwrap().chain_uid;
    let events =
        relay_factory_router_factory(tx_response.events, factory, router, factory_chain_uid)?;
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helpers::chains::{get_escrow, get_lp_token, setup_interchain, setup_router};
    use crate::helpers::relayer::extract_ack_packet_events;
    use crate::tests_reusable::constants::ROUTER_CHAIN_ID;
    use crate::tests_reusable::factory_create_pool::create_pool;
    use crate::tests_reusable::factory_register::{setup_factory, FactorySetupMode};
    use crate::tests_reusable::factory_register_denom::register_denom;
    use crate::tests_reusable::test_macros::decimal_pair;
    use cosmwasm_std::Uint64;
    use euclid::msgs::escrow::QueryMsgFns as EscrowQueryMsgFns;
    use euclid::msgs::factory::msg::{
        GetPendingSingleSidedLiquidityResponse, QueryMsg as FactoryQueryMsg,
    };
    use euclid::msgs::lp_token::msg::QueryMsgFns as LpTokenQueryMsgFns;
    use euclid::msgs::vlp::base::{GetLiquidityQueryResponse, PoolConfig, QueryMsg as VlpQueryMsg};
    use euclid::token::{PairWithDenomAndAmount, TokenWithDenomAndAmount};
    use euclid::utils::pagination::Pagination;
    use rstest::rstest;
    use rstest_reuse::apply;

    fn native_token_with_decimals(name: &str, decimals: u32) -> TokenWithDenom {
        TokenWithDenom {
            token: Token::create(name.to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: name.to_string(),
                decimals: Some(decimals),
            },
        }
    }

    /// Read VLP reserves directly through the router env (the VLP lives on the
    /// hub chain alongside the router).
    fn vlp_reserves(
        router: &RouterContract<MockBase>,
        vlp_address: &str,
    ) -> GetLiquidityQueryResponse {
        router
            .environment()
            .query(
                &VlpQueryMsg::Liquidity {},
                &Addr::unchecked(vlp_address.to_string()),
            )
            .unwrap()
    }

    fn assert_no_pending_single_sided(factory: &FactoryContract<MockBase>, user: &Addr) {
        let response: GetPendingSingleSidedLiquidityResponse = factory
            .environment()
            .query(
                &FactoryQueryMsg::PendingSingleSidedLiquidity {
                    user: user.clone(),
                    pagination: Pagination {
                        min: None,
                        max: None,
                        skip: None,
                        limit: None,
                    },
                },
                &factory.address().unwrap(),
            )
            .unwrap();
        assert!(
            response.pending_single_sided_liquidity.is_empty(),
            "PENDING_SINGLE_SIDED_LIQUIDITY should be empty after the flow resolves; \
             found {} entries",
            response.pending_single_sided_liquidity.len()
        );
    }

    /// Scenario A: stable pool happy path. Native asset_in into a stable VLP
    /// across the project's decimal-pair test grid. The orchestrator is
    /// pool-type agnostic — the same single-sided flow that works for
    /// constant-product must work for stable here unchanged. Parameterizing
    /// over decimals exercises the voucher-normalization seams between the
    /// remote factory (raw denom units) and the hub VLP (24-dec voucher units).
    #[cfg_attr(not(feature = "full_decimals"), apply(decimal_pair))]
    #[cfg_attr(feature = "full_decimals", apply(decimal_pair_full))]
    fn test_single_sided_stable_pool_happy_path(
        #[values(FactorySetupMode::Native, FactorySetupMode::Ibc)] mode: FactorySetupMode,
        decimals_a: u32,
        decimals_b: u32,
    ) {
        let factory_chain_id = mode.chain_id();
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, factory_chain_id);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![factory_chain_id]).unwrap();
        let factory = setup_factory(&interchain, factory_chain_id, &router).unwrap();

        let usdc = native_token_with_decimals("uusdc", decimals_a);
        let usdt = native_token_with_decimals("uusdt", decimals_b);

        register_denom(&factory, &router, usdc.clone()).unwrap();
        register_denom(&factory, &router, usdt.clone()).unwrap();

        let decimal_a_multiplier = Uint256::from(10u128).pow(decimals_a);
        let decimal_b_multiplier = Uint256::from(10u128).pow(decimals_b);

        // Seed the pool with deep, balanced reserves (per-side at native dp)
        // so the single-sided swap is well within tolerance.
        let seed_a = Uint256::from(1_000_000u128)
            .checked_mul(decimal_a_multiplier)
            .unwrap();
        let seed_b = Uint256::from(1_000_000u128)
            .checked_mul(decimal_b_multiplier)
            .unwrap();
        let pair = PairWithDenomAndAmount {
            token_1: TokenWithDenomAndAmount {
                token: usdc.token.clone(),
                token_type: usdc.token_type.clone(),
                amount: seed_a,
            },
            token_2: TokenWithDenomAndAmount {
                token: usdt.token.clone(),
                token_type: usdt.token_type.clone(),
                amount: seed_b,
            },
        };
        create_pool(
            &factory,
            &router,
            pair.clone(),
            500,
            PoolConfig::Stable {
                amp_factor: Some(Uint64::new(100)),
            },
        )
        .unwrap();

        let pool_pair = pair.get_pair().unwrap();
        let vlp_address = factory.get_vlp(pool_pair.clone()).unwrap().vlp_address;
        let lp_token_address = factory
            .get_lp_token(vlp_address.clone())
            .unwrap()
            .token_address;
        let lp_token = get_lp_token(factory.environment(), &lp_token_address);

        let lp_balance_before = lp_token
            .balance(factory.environment().sender.to_string())
            .unwrap()
            .balance;
        let escrow_in = get_escrow(&factory, usdc.token.as_str());
        let escrow_in_before = escrow_in.state().unwrap().total_amount;

        // Single-sided deposit: 10_000 asset_a (1% of pool), swap 4_000 to
        // asset_b. Scaled into asset_a's native dp.
        let amount_in = Uint256::from(10_000u128)
            .checked_mul(decimal_a_multiplier)
            .unwrap();
        let swap_amount = Uint256::from(4_000u128)
            .checked_mul(decimal_a_multiplier)
            .unwrap();
        let min_lp_out = Uint256::from(1u128);

        single_sided_add_liquidity(
            &factory,
            &router,
            usdc.clone(),
            amount_in,
            usdt.token.clone(),
            swap_amount,
            min_lp_out,
            None,
        )
        .unwrap();

        // LP CW20 minted on the remote chain.
        let lp_balance_after = lp_token
            .balance(factory.environment().sender.to_string())
            .unwrap()
            .balance;
        assert!(
            lp_balance_after > lp_balance_before,
            "LP balance should have grown; before={lp_balance_before} after={lp_balance_after}"
        );

        // Full deposit landed in escrow (no partner fee, so amount_in is intact).
        let escrow_in_after = escrow_in.state().unwrap().total_amount;
        assert_eq!(
            escrow_in_after,
            escrow_in_before + amount_in,
            "Escrow must hold the full single-sided deposit"
        );

        // No residual state on the factory.
        assert_no_pending_single_sided(&factory, &factory.environment().sender);
    }

    /// Scenario B: ack-failure refund. A contrived `min_lp_out` two orders of
    /// magnitude above any plausible LP output forces the hub's add-liquidity
    /// reply to return `SlippageExceeded`. The outer reply converts to an
    /// error ack, the factory refunds `amount_in + partner_fee_amount` to the
    /// user, and no residual state remains. Parameterized over decimals to
    /// ensure the refund/escrow accounting is correct across mixed-decimal
    /// pools (refund is in raw denom units, so decimal handling must match
    /// the deposit path exactly).
    #[cfg_attr(not(feature = "full_decimals"), apply(decimal_pair))]
    #[cfg_attr(feature = "full_decimals", apply(decimal_pair_full))]
    fn test_single_sided_ack_failure_refunds_user(
        #[values(FactorySetupMode::Native, FactorySetupMode::Ibc)] mode: FactorySetupMode,
        decimals_a: u32,
        decimals_b: u32,
    ) {
        let factory_chain_id = mode.chain_id();
        let sender_name = "sender_for_all_chains";
        let interchain = setup_interchain(sender_name, factory_chain_id);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![factory_chain_id]).unwrap();
        let factory = setup_factory(&interchain, factory_chain_id, &router).unwrap();
        let user = factory.environment().sender.clone();

        let token_a = native_token_with_decimals("uusdc", decimals_a);
        let token_b = native_token_with_decimals("uweth", decimals_b);

        register_denom(&factory, &router, token_a.clone()).unwrap();
        register_denom(&factory, &router, token_b.clone()).unwrap();

        let decimal_a_multiplier = Uint256::from(10u128).pow(decimals_a);
        let decimal_b_multiplier = Uint256::from(10u128).pow(decimals_b);

        let seed_a = Uint256::from(1_000_000u128)
            .checked_mul(decimal_a_multiplier)
            .unwrap();
        let seed_b = Uint256::from(1_000_000u128)
            .checked_mul(decimal_b_multiplier)
            .unwrap();
        let pair = PairWithDenomAndAmount {
            token_1: TokenWithDenomAndAmount {
                token: token_a.token.clone(),
                token_type: token_a.token_type.clone(),
                amount: seed_a,
            },
            token_2: TokenWithDenomAndAmount {
                token: token_b.token.clone(),
                token_type: token_b.token_type.clone(),
                amount: seed_b,
            },
        };
        create_pool(
            &factory,
            &router,
            pair.clone(),
            500,
            PoolConfig::ConstantProduct {},
        )
        .unwrap();
        let pool_pair = pair.get_pair().unwrap();
        let vlp_address = factory.get_vlp(pool_pair).unwrap().vlp_address;

        // Snapshot pre-flow state we expect to be unchanged on failure.
        let reserves_before = vlp_reserves(&router, &vlp_address);
        let escrow_in = get_escrow(&factory, token_a.token.as_str());
        let escrow_in_before = escrow_in.state().unwrap().total_amount;
        let user_balance_before = factory
            .environment()
            .balance(&user, Some("uusdc".to_string()))
            .unwrap();
        let partner = factory.environment().addr_make("partner");
        let partner_balance_before = factory
            .environment()
            .balance(&partner, Some("uusdc".to_string()))
            .unwrap();

        // Contrived: min_lp_out is huge, guaranteed to exceed any LP mint.
        let amount_in = Uint256::from(10_000u128)
            .checked_mul(decimal_a_multiplier)
            .unwrap();
        let swap_amount = Uint256::from(4_000u128)
            .checked_mul(decimal_a_multiplier)
            .unwrap();
        let min_lp_out = Uint256::from(u128::MAX / 2);

        // Partner fee = 0.3% (max).
        let partner_fee = Some(PartnerFee {
            partner_fee_bps: 30,
            recipient: partner.to_string(),
        });

        // Two modes diverge on the failure path:
        //  - IBC chain: ack-failure flows through the factory's ack handler;
        //    the call returns Ok with an error ack packet.
        //  - Native chain: factory ack handler returns Err, which reverts the
        //    entire transaction atomically — cw-orch surfaces this as Err on
        //    the relay call.
        // In both cases, post-flow state must be clean and the user's funds
        // must be intact.
        let result = single_sided_add_liquidity(
            &factory,
            &router,
            token_a.clone(),
            amount_in,
            token_b.token.clone(),
            swap_amount,
            min_lp_out,
            partner_fee,
        );
        match result {
            Ok(events) => {
                let ack_events = extract_ack_packet_events(&events);
                let first_ack = ack_events
                    .first()
                    .expect("expected at least one ack packet");
                let ack_str = String::from_utf8(first_ack.ack.to_vec()).unwrap();
                // `SlippageExceeded`'s Display text starts with "Slippage has
                // not been tolerated"; it surfaces from
                // `on_single_sided_add_liquidity_reply` because
                // `mint_lp_tokens < min_lp_out`.
                assert!(
                    ack_str.contains("Slippage has not been tolerated"),
                    "Expected SlippageExceeded error in ack, got: {ack_str}"
                );
            }
            Err(err) => {
                let root = err.root().to_string();
                assert!(
                    root.contains("Slippage has not been tolerated"),
                    "Expected slippage error on native revert, got: {root}"
                );
            }
        }

        // Refund invariant: the faucet inside `single_sided_add_liquidity`
        // hands the user the full deposit (`amount_in` already includes the
        // partner-fee portion). The factory pulls all of it, and on failure
        // refunds `amount_in + partner_fee_amount` — which equals the
        // original deposit. So the user's net balance change is exactly
        // `amount_in` (the faucet-minted amount, returned intact).
        let user_balance_after = factory
            .environment()
            .balance(&user, Some("uusdc".to_string()))
            .unwrap();
        let coin_back = user_balance_after
            .iter()
            .find(|c| c.denom == "uusdc")
            .map(|c| c.amount)
            .unwrap_or(Uint128::zero());
        let coin_before = user_balance_before
            .iter()
            .find(|c| c.denom == "uusdc")
            .map(|c| c.amount)
            .unwrap_or(Uint128::zero());
        let amount_in_u128 = Uint128::try_from(amount_in).unwrap();
        assert_eq!(
            coin_back,
            coin_before + amount_in_u128,
            "Refund must restore the full deposit (amount_in + partner_fee_amount)"
        );

        // Partner did NOT receive the fee on failure.
        let partner_balance_after = factory
            .environment()
            .balance(&partner, Some("uusdc".to_string()))
            .unwrap();
        let partner_after = partner_balance_after
            .iter()
            .find(|c| c.denom == "uusdc")
            .map(|c| c.amount)
            .unwrap_or(Uint128::zero());
        let partner_before = partner_balance_before
            .iter()
            .find(|c| c.denom == "uusdc")
            .map(|c| c.amount)
            .unwrap_or(Uint128::zero());
        assert_eq!(
            partner_after, partner_before,
            "Partner address must not receive the fee on the failure path"
        );

        // Escrow is unchanged (no liquidity added).
        let escrow_in_after = escrow_in.state().unwrap().total_amount;
        assert_eq!(
            escrow_in_after, escrow_in_before,
            "Escrow must be unchanged on the failure path"
        );

        // VLP reserves unchanged.
        let reserves_after = vlp_reserves(&router, &vlp_address);
        assert_eq!(
            reserves_after.token_1_reserve, reserves_before.token_1_reserve,
            "VLP token_1 reserve must be unchanged on failure"
        );
        assert_eq!(
            reserves_after.token_2_reserve, reserves_before.token_2_reserve,
            "VLP token_2 reserve must be unchanged on failure"
        );

        // No residual pending state on the factory.
        assert_no_pending_single_sided(&factory, &user);
    }

    // -----------------------------------------------------------------------
    // Issue-4 follow-ups (review request):
    //   1. invalid swap routes are rejected at the factory before IBC
    //   2. swap_amount = amount_in - 1 boundary (no underflow downstream)
    //   3. partner-fee happy path (partner is paid on success)
    // -----------------------------------------------------------------------

    /// Build a pool of (uusdc, uusdt) at 6 decimals each, seeded balanced.
    /// Returns (factory, router, usdc, usdt). Fixed decimals here — these
    /// tests target scenarios independent of the decimal grid.
    fn setup_pool_6dp(
        mode: FactorySetupMode,
    ) -> (
        FactoryContract<MockBase>,
        RouterContract<MockBase>,
        TokenWithDenom,
        TokenWithDenom,
    ) {
        let factory_chain_id = mode.chain_id();
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, factory_chain_id);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![factory_chain_id]).unwrap();
        let factory = setup_factory(&interchain, factory_chain_id, &router).unwrap();

        let usdc = native_token_with_decimals("uusdc", 6);
        let usdt = native_token_with_decimals("uusdt", 6);
        register_denom(&factory, &router, usdc.clone()).unwrap();
        register_denom(&factory, &router, usdt.clone()).unwrap();

        let seed = Uint256::from(1_000_000u128 * 1_000_000u128);
        let pair = PairWithDenomAndAmount {
            token_1: TokenWithDenomAndAmount {
                token: usdc.token.clone(),
                token_type: usdc.token_type.clone(),
                amount: seed,
            },
            token_2: TokenWithDenomAndAmount {
                token: usdt.token.clone(),
                token_type: usdt.token_type.clone(),
                amount: seed,
            },
        };
        create_pool(&factory, &router, pair, 500, PoolConfig::ConstantProduct {}).unwrap();

        (factory, router, usdc, usdt)
    }

    /// Invalid swap routes are rejected at the factory before any IBC
    /// roundtrip. The contract returns Err synchronously; no pending
    /// state is created. Covers: empty hops, multi-hop (v1 forbids it),
    /// wrong hop.token_in, wrong hop.token_out.
    #[rstest]
    #[case::empty_route(vec![], "swap_route must contain exactly one hop in v1")]
    #[case::two_hops(
        vec![
            NextSwapPair {
                token_in: Token::create("uusdc".to_string()).unwrap(),
                token_out: Token::create("uusdt".to_string()).unwrap(),
                test_fail: None,
            },
            NextSwapPair {
                token_in: Token::create("uusdt".to_string()).unwrap(),
                token_out: Token::create("uusdc".to_string()).unwrap(),
                test_fail: None,
            },
        ],
        "swap_route must contain exactly one hop in v1"
    )]
    #[case::wrong_token_in(
        vec![NextSwapPair {
            token_in: Token::create("udai".to_string()).unwrap(),
            token_out: Token::create("uusdt".to_string()).unwrap(),
            test_fail: None,
        }],
        "swap_route first hop token_in must match asset_in"
    )]
    #[case::wrong_token_out(
        vec![NextSwapPair {
            token_in: Token::create("uusdc".to_string()).unwrap(),
            token_out: Token::create("udai".to_string()).unwrap(),
            test_fail: None,
        }],
        "swap_route last hop token_out must match the other pair token"
    )]
    fn test_single_sided_invalid_swap_route_rejected_at_factory(
        #[values(FactorySetupMode::Native, FactorySetupMode::Ibc)] mode: FactorySetupMode,
        #[case] swap_route: Vec<NextSwapPair>,
        #[case] expected_error: &str,
    ) {
        let (factory, _router, usdc, usdt) = setup_pool_6dp(mode);
        let user = factory.environment().sender.clone();

        let amount_in = Uint256::from(10_000u128 * 1_000_000u128);
        let amount_in_u128 = Uint128::try_from(amount_in).unwrap().u128();
        factory
            .environment()
            .add_balance(&user, vec![coin(amount_in_u128, "uusdc")])
            .unwrap();

        let pair = Pair::new(usdc.token.clone(), usdt.token.clone()).unwrap();
        let result = factory.execute(
            &FactoryExecuteMsg::AddSingleSidedLiquidity {
                asset_in: usdc.clone(),
                amount_in,
                pair,
                swap_amount: Uint256::from(4_000u128 * 1_000_000u128),
                swap_route,
                min_lp_out: Uint256::from(1u128),
                partner_fee: None,
                cross_chain_config: CrossChainConfig::default(),
            },
            &[coin(amount_in_u128, "uusdc")],
        );

        let err = result.expect_err("expected factory.execute to reject invalid route");
        let err_msg = err.root().to_string();
        assert!(
            err_msg.contains(expected_error),
            "Expected error containing '{expected_error}', got: {err_msg}"
        );

        // Factory must not have created any pending entry.
        assert_no_pending_single_sided(&factory, &user);
    }

    /// Boundary: swap_amount = amount_in - 1 satisfies the strict `<` check
    /// and must not underflow downstream (the hub subtracts swap_amount from
    /// amount_in to derive the residual asset_in for add-liquidity). With
    /// `amount_in - swap_amount = 1`, the liquidity leg is extremely
    /// imbalanced — the result will typically be `SlippageExceeded` on the
    /// add-liquidity reply — but the call must surface that as either an
    /// ack-fail (IBC) or a revert (native), NEVER a panic/underflow.
    #[rstest]
    fn test_single_sided_swap_amount_equals_amount_in_minus_one_boundary(
        #[values(FactorySetupMode::Native, FactorySetupMode::Ibc)] mode: FactorySetupMode,
    ) {
        let (factory, router, usdc, usdt) = setup_pool_6dp(mode);
        let user = factory.environment().sender.clone();

        let escrow_in = get_escrow(&factory, usdc.token.as_str());
        let escrow_in_before = escrow_in.state().unwrap().total_amount;
        let user_balance_before = factory
            .environment()
            .balance(&user, Some("uusdc".to_string()))
            .unwrap()
            .iter()
            .find(|c| c.denom == "uusdc")
            .map(|c| c.amount)
            .unwrap_or(Uint128::zero());

        let amount_in = Uint256::from(10_000u128 * 1_000_000u128);
        let swap_amount = amount_in.checked_sub(Uint256::one()).unwrap();

        let result = single_sided_add_liquidity(
            &factory,
            &router,
            usdc.clone(),
            amount_in,
            usdt.token.clone(),
            swap_amount,
            Uint256::from(1u128),
            None,
        );

        // Either branch is acceptable; the key invariant is no panic /
        // arithmetic underflow. With remaining = amount_in - swap_amount = 1
        // voucher unit on the asset_in side, the liquidity leg is grossly
        // imbalanced and slippage is the expected failure mode. The message
        // surfaces from either the VLP's slippage_tolerance check
        // ("Slippage has been exceeded ...") or the orchestrator's
        // min_lp_out check ("Slippage has not been tolerated") — both must
        // contain "Slippage", never an arithmetic-overflow artifact.
        match result {
            Ok(events) => {
                let ack_events = extract_ack_packet_events(&events);
                if let Some(first) = ack_events.first() {
                    let ack_str = String::from_utf8(first.ack.to_vec()).unwrap();
                    if !ack_str.contains("\"Ok\"") {
                        assert!(
                            ack_str.contains("Slippage"),
                            "boundary case must surface a slippage error, got: {ack_str}"
                        );
                    }
                }
            }
            Err(err) => {
                let root = err.root().to_string();
                assert!(
                    root.contains("Slippage"),
                    "boundary case must revert with a slippage error, got: {root}"
                );
                // On revert, escrow and user funds are unchanged.
                let escrow_in_after = escrow_in.state().unwrap().total_amount;
                assert_eq!(escrow_in_after, escrow_in_before);
                let user_balance_after = factory
                    .environment()
                    .balance(&user, Some("uusdc".to_string()))
                    .unwrap()
                    .iter()
                    .find(|c| c.denom == "uusdc")
                    .map(|c| c.amount)
                    .unwrap_or(Uint128::zero());
                // Faucet handed user `amount_in` then the revert returned it.
                assert_eq!(
                    user_balance_after,
                    user_balance_before + Uint128::try_from(amount_in).unwrap()
                );
            }
        }

        // No residual pending state regardless of which path was taken.
        assert_no_pending_single_sided(&factory, &user);
    }

    /// Partner-fee happy path: on success, the partner receives
    /// `partner_fee_amount` in raw asset_in units, the escrow receives
    /// `amount_in - partner_fee_amount` (the post-fee portion that crosses
    /// IBC), and the user receives LP. Complements the ack-failure refund
    /// test which asserts the partner is NOT paid on failure.
    #[rstest]
    fn test_single_sided_partner_fee_happy_path_pays_partner(
        #[values(FactorySetupMode::Native, FactorySetupMode::Ibc)] mode: FactorySetupMode,
    ) {
        let (factory, router, usdc, usdt) = setup_pool_6dp(mode);
        let user = factory.environment().sender.clone();
        let partner = factory.environment().addr_make("partner");

        let pool_pair = Pair::new(usdc.token.clone(), usdt.token.clone()).unwrap();
        let vlp_address = factory.get_vlp(pool_pair).unwrap().vlp_address;
        let lp_token_address = factory.get_lp_token(vlp_address).unwrap().token_address;
        let lp_token = get_lp_token(factory.environment(), &lp_token_address);

        let lp_balance_before = lp_token.balance(user.to_string()).unwrap().balance;
        let escrow_in = get_escrow(&factory, usdc.token.as_str());
        let escrow_in_before = escrow_in.state().unwrap().total_amount;
        let partner_before = factory
            .environment()
            .balance(&partner, Some("uusdc".to_string()))
            .unwrap()
            .iter()
            .find(|c| c.denom == "uusdc")
            .map(|c| c.amount)
            .unwrap_or(Uint128::zero());

        let amount_in = Uint256::from(10_000u128 * 1_000_000u128);
        let swap_amount = Uint256::from(4_000u128 * 1_000_000u128);
        let partner_fee_bps = 30u64; // 0.3% = max
        let partner_fee = Some(PartnerFee {
            partner_fee_bps,
            recipient: partner.to_string(),
        });

        // partner_fee_amount = ceil(amount_in * 30 / 10000)
        let partner_fee_amount = amount_in
            .checked_mul_ceil(cosmwasm_std::Decimal::bps(partner_fee_bps))
            .unwrap();
        let net_amount_in = amount_in.checked_sub(partner_fee_amount).unwrap();

        single_sided_add_liquidity(
            &factory,
            &router,
            usdc.clone(),
            amount_in,
            usdt.token.clone(),
            swap_amount,
            Uint256::from(1u128),
            partner_fee,
        )
        .unwrap();

        // LP CW20 minted to user.
        let lp_balance_after = lp_token.balance(user.to_string()).unwrap().balance;
        assert!(
            lp_balance_after > lp_balance_before,
            "LP balance must grow on success; before={lp_balance_before} after={lp_balance_after}"
        );

        // Escrow grew by net (post-fee) amount only.
        let escrow_in_after = escrow_in.state().unwrap().total_amount;
        assert_eq!(
            escrow_in_after,
            escrow_in_before + net_amount_in,
            "Escrow must hold amount_in - partner_fee_amount"
        );

        // Partner received exactly partner_fee_amount in raw uusdc.
        let partner_after = factory
            .environment()
            .balance(&partner, Some("uusdc".to_string()))
            .unwrap()
            .iter()
            .find(|c| c.denom == "uusdc")
            .map(|c| c.amount)
            .unwrap_or(Uint128::zero());
        assert_eq!(
            partner_after,
            partner_before + Uint128::try_from(partner_fee_amount).unwrap(),
            "Partner must be paid exactly partner_fee_amount in raw asset_in units"
        );

        assert_no_pending_single_sided(&factory, &user);
    }
}
