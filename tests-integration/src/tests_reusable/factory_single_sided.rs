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

    fn native_token(name: &str) -> TokenWithDenom {
        TokenWithDenom {
            token: Token::create(name.to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: name.to_string(),
                decimals: Some(6),
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

    /// Scenario A: stable pool happy path. Native USDC into a USDC/USDT stable
    /// VLP. The orchestrator is pool-type agnostic — the same single-sided flow
    /// that works for constant-product must work here unchanged.
    #[rstest]
    fn test_single_sided_stable_pool_happy_path(
        #[values(FactorySetupMode::Native, FactorySetupMode::Ibc)] mode: FactorySetupMode,
    ) {
        let factory_chain_id = mode.chain_id();
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, factory_chain_id);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![factory_chain_id]).unwrap();
        let factory = setup_factory(&interchain, factory_chain_id, &router).unwrap();

        let usdc = native_token("uusdc");
        let usdt = native_token("uusdt");

        register_denom(&factory, &router, usdc.clone()).unwrap();
        register_denom(&factory, &router, usdt.clone()).unwrap();

        // Seed the pool with deep, balanced reserves so the single-sided swap
        // is well within tolerance.
        let seed = Uint256::from(1_000_000u128 * 1_000_000u128); // 1M units (6 dp)
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

        // Single-sided deposit: 10_000 USDC, swap 4_000 to USDT, expect a
        // small but non-zero LP mint.
        let amount_in = Uint256::from(10_000u128 * 1_000_000u128);
        let swap_amount = Uint256::from(4_000u128 * 1_000_000u128);
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
    /// user, and no residual state remains.
    ///
    /// With Issue 2 merged, this also verifies the partner-fee portion is
    /// refunded (no retention at the factory, no transfer to the partner).
    #[rstest]
    fn test_single_sided_ack_failure_refunds_user(
        #[values(FactorySetupMode::Native, FactorySetupMode::Ibc)] mode: FactorySetupMode,
    ) {
        let factory_chain_id = mode.chain_id();
        let sender_name = "sender_for_all_chains";
        let interchain = setup_interchain(sender_name, factory_chain_id);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![factory_chain_id]).unwrap();
        let factory = setup_factory(&interchain, factory_chain_id, &router).unwrap();
        let user = factory.environment().sender.clone();

        let token_a = native_token("uusdc");
        let token_b = native_token("uweth");

        register_denom(&factory, &router, token_a.clone()).unwrap();
        register_denom(&factory, &router, token_b.clone()).unwrap();

        let seed = Uint256::from(1_000_000u128 * 1_000_000u128);
        let pair = PairWithDenomAndAmount {
            token_1: TokenWithDenomAndAmount {
                token: token_a.token.clone(),
                token_type: token_a.token_type.clone(),
                amount: seed,
            },
            token_2: TokenWithDenomAndAmount {
                token: token_b.token.clone(),
                token_type: token_b.token_type.clone(),
                amount: seed,
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
        let amount_in = Uint256::from(10_000u128 * 1_000_000u128);
        let swap_amount = Uint256::from(4_000u128 * 1_000_000u128);
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
}
