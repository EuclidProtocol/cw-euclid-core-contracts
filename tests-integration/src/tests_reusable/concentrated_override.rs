#![cfg(not(target_arch = "wasm32"))]

//! SC-23 Issue 8 Slice 3 — CLP per-wallet Euclid-fee override, integration matrix.
//!
//! CLP pools are created with a protocol cut (`euclid_fee_bps`) of 0, so an
//! override is a no-op on a default pool. These tests first raise the cut via
//! `UpdateFee` (the CLP's fee admin is the router's general admin, which is the
//! test deployer) so the override has something to waive, then verify:
//!   - the protocol's slice is waived/reduced exactly,
//!   - the LP's per-unit accrual is unaffected,
//!   - the trader's quote improves,
//!   - simulation matches execution, and
//!   - `CollectFees` still pays the LP under an override.

use cosmwasm_std::{Addr, Uint128, Uint256};
use cw_orch::mock::MockBase;
use cw_orch::prelude::*;
use euclid::cross_chain_user::CrossChainUser;
use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
use euclid::msgs::router::execute::{ExecuteMsgFns as RouterExecuteMsgFns, ManageRouterState};
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
use euclid::msgs::router::query::{
    QueryMsg as RouterQueryMsg, QuerySimulateSwap, SimulateSwapResponse,
};
use euclid::msgs::virtual_balance::msg::QueryMsgFns as VirtualBalanceQueryMsgFns;
use euclid::msgs::vlp::base::{GetSwapQueryResponse, PoolKey, VlpSimulateSwapMsg};
use euclid::msgs::vlp::concentrated::msg::{
    ExecuteMsg as ConcentratedExecuteMsg, QueryMsg as ConcentratedQueryMsg, TotalFeesResponse,
};
use euclid::normalize::normalize_token_to_voucher;
use euclid::swap::NextSwapPair;
use euclid::token::Token;
use euclid::voucher::BalanceKey;
use rstest::rstest;

use crate::helpers::chains::{get_concentrated_vlp, get_virtual_balance};
use crate::helpers::factory::{
    collect_concentrated_fees, create_concentrated_pool, list_position_ids,
};
use crate::tests_reusable::concentrated_create_pool::{pair_with_amounts, setup_concentrated_env};
use crate::tests_reusable::constants::{
    FACTORY_CHAIN_ID_EVM, FACTORY_CHAIN_ID_IBC, FACTORY_CHAIN_ID_LOCAL,
};
use crate::tests_reusable::factory_register::FactorySetupMode;
use crate::tests_reusable::factory_swap::swap_request;

/// Protocol cut applied to the CLP for these tests. Capped at `MAX_FEE_BPS`
/// (1000 bps = 10%), the largest cut `UpdateFee` accepts.
const PROTOCOL_CUT_BPS: u64 = 1_000;

type Vlp = concentrated_vlp::ConcentratedVlpContract<cw_orch::mock::MockBase>;

fn clp_vlp(router: &router::RouterContract<MockBase>, pool_key: &PoolKey) -> Vlp {
    let addr = router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp;
    get_concentrated_vlp(router.environment(), &Addr::unchecked(addr))
}

/// Raise the CLP protocol cut so the override has a slice to waive. The CLP's
/// fee admin is the router's general admin (the deployer), which is the vlp
/// handle's default sender.
fn set_protocol_cut(vlp: &Vlp, euclid_fee_bps: u64) {
    vlp.execute(
        &ConcentratedExecuteMsg::UpdateFee {
            lp_fee_bps: None,
            euclid_fee_bps: Some(euclid_fee_bps),
            recipient: None,
        },
        &[],
    )
    .unwrap();
}

fn simulate_clp(
    vlp: &Vlp,
    asset_in: &Token,
    asset_amount: Uint256,
    euclid_fee_override: Option<u64>,
) -> GetSwapQueryResponse {
    vlp.query(&ConcentratedQueryMsg::SimulateSwap(VlpSimulateSwapMsg {
        asset: asset_in.clone(),
        asset_amount,
        swaps: vec![],
        euclid_fee_override,
    }))
    .unwrap()
}

fn fee_totals(vlp: &Vlp) -> TotalFeesResponse {
    vlp.query(&ConcentratedQueryMsg::TotalFeesCollected {})
        .unwrap()
}

fn voucher_balance(
    router: &router::RouterContract<MockBase>,
    sender: &CrossChainUser,
    token: &Token,
) -> Uint256 {
    let virtual_balance = get_virtual_balance(
        router.environment(),
        &router.get_state().unwrap().virtual_balance_address,
    );
    virtual_balance
        .get_balance(BalanceKey {
            cross_chain_user: sender.clone(),
            token_id: token.to_string(),
        })
        .unwrap()
        .amount
}

/// Per-unit-liquidity invariant: the LP's accrual must not change with the
/// override (same liquidity, same pool state), allowing for sub-unit rounding
/// from the differing fee tiers. 0.01% tolerance is ~1000x looser than the
/// real (sub-ppm) divergence but immune to step-rounding noise.
fn assert_lp_unaffected(reference: Uint256, candidate: Uint256) {
    let diff = reference.abs_diff(candidate);
    assert!(
        diff.checked_mul(Uint256::from(10_000u128)).unwrap() <= reference,
        "LP accrual must be unaffected by the override: reference={reference}, \
         candidate={candidate}, diff={diff}"
    );
}

// ---------------------------------------------------------------------------
// Test 1 — VLP-level simulate invariants (read-only, identical pool state).
// ---------------------------------------------------------------------------

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
#[case(FactorySetupMode::Evm, FACTORY_CHAIN_ID_EVM)]
fn test_clp_override_simulate_waives_protocol_lp_unaffected(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 40_000, 40_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();
    let vlp = clp_vlp(&router, &pool_key);
    set_protocol_cut(&vlp, PROTOCOL_CUT_BPS);

    let decimals = token_a.token_type.get_decimals().unwrap();
    let amount = normalize_token_to_voucher(Uint256::from(2_000u128), decimals).unwrap();

    let full = simulate_clp(&vlp, &token_a.token, amount, None);
    let zeroed = simulate_clp(&vlp, &token_a.token, amount, Some(0));
    let half = simulate_clp(&vlp, &token_a.token, amount, Some(PROTOCOL_CUT_BPS / 2));

    // Protocol slice: full by default, fully waived at Some(0), partial at the
    // midpoint.
    assert!(
        full.euclid_fee > Uint256::zero(),
        "default cut must collect a protocol fee"
    );
    assert_eq!(
        zeroed.euclid_fee,
        Uint256::zero(),
        "Some(0) must waive the protocol fee entirely"
    );
    assert!(
        half.euclid_fee > Uint256::zero() && half.euclid_fee < full.euclid_fee,
        "midpoint override must partially reduce the protocol fee: full={}, half={}",
        full.euclid_fee,
        half.euclid_fee
    );

    // LP accrual is unchanged across all override values (same liquidity).
    assert_lp_unaffected(full.lp_fee, zeroed.lp_fee);
    assert_lp_unaffected(full.lp_fee, half.lp_fee);

    // Trader's quote strictly improves as the override shrinks the fee.
    assert!(
        zeroed.amount_out > full.amount_out,
        "Some(0) must improve the trader's quote: full={}, zeroed={}",
        full.amount_out,
        zeroed.amount_out
    );
    assert!(
        half.amount_out > full.amount_out && half.amount_out < zeroed.amount_out,
        "midpoint quote must sit between: full={}, half={}, zeroed={}",
        full.amount_out,
        half.amount_out,
        zeroed.amount_out
    );
}

// ---------------------------------------------------------------------------
// Test 2 — execution end-to-end: quote matches execution, protocol fee waived,
// LP still collects.
// ---------------------------------------------------------------------------

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_clp_override_execution_matches_quote_and_waives_protocol(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 40_000, 40_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();
    let vlp = clp_vlp(&router, &pool_key);
    set_protocol_cut(&vlp, PROTOCOL_CUT_BPS);

    // Initial full-range position created at pool creation, owned by the wallet.
    let position_id = Uint128::new(
        list_position_ids(&factory)
            .unwrap()
            .last()
            .unwrap()
            .parse::<u128>()
            .unwrap(),
    );

    let chain_uid = factory.get_state().unwrap().chain_uid;
    let wallet = CrossChainUser::new(chain_uid, factory.environment().sender.to_string());

    // Full exemption for this wallet (fee-admin gated; router deployer is admin).
    router
        .manage_router_state(ManageRouterState::SetEuclidFeeOverride {
            user: wallet.clone(),
            euclid_fee_bps: Some(0),
        })
        .unwrap();

    let amount_in = Uint256::from(2_000u128);
    let decimals = token_a.token_type.get_decimals().unwrap();
    let voucher_amount_in = normalize_token_to_voucher(amount_in, decimals).unwrap();
    let route = vec![NextSwapPair {
        token_in: token_a.token.clone(),
        token_out: token_b.token.clone(),
        pool_key: Some(pool_key.clone()),
        test_fail: None,
    }];

    let simulate = |sender: Option<CrossChainUser>| -> Uint256 {
        let resp: SimulateSwapResponse = router
            .query(&RouterQueryMsg::SimulateSwap(QuerySimulateSwap {
                asset_in: token_a.token.clone(),
                amount_in: voucher_amount_in,
                asset_out: token_b.token.clone(),
                min_amount_out: Uint256::from(1u128),
                swaps: route.clone(),
                sender,
            }))
            .unwrap();
        resp.amount_out
    };
    let sim_with = simulate(Some(wallet.clone()));
    let sim_without = simulate(None);
    assert!(
        sim_with > sim_without,
        "override must improve the CLP quote: with={sim_with}, without={sim_without}"
    );

    let before_out = voucher_balance(&router, &wallet, &token_b.token);
    swap_request(
        &factory,
        &router,
        token_a.clone(),
        token_b.token.clone(),
        amount_in,
        Uint256::from(1u128),
        route,
        vec![],
        None,
    )
    .unwrap();
    let executed = voucher_balance(&router, &wallet, &token_b.token)
        .checked_sub(before_out)
        .unwrap();

    // Execution resolves the same override and must match the override-applied
    // simulation exactly.
    assert_eq!(
        executed, sim_with,
        "executed CLP output must match the override-applied simulation"
    );

    // Protocol fee fully waived; LP still earned a fee on the input token.
    let totals = fee_totals(&vlp);
    assert_eq!(
        totals
            .total_fees
            .euclid_fees
            .get_fee(token_a.token.to_string().as_str()),
        Uint256::zero(),
        "override must waive the protocol fee on the executed swap"
    );
    assert_eq!(
        totals
            .total_fees
            .euclid_fees
            .get_fee(token_b.token.to_string().as_str()),
        Uint256::zero(),
        "no protocol fee should accrue on the output token either"
    );
    assert!(
        totals
            .total_fees
            .lp_fees
            .get_fee(token_a.token.to_string().as_str())
            > Uint256::zero(),
        "LP must still earn its fee under a full Euclid-fee exemption"
    );

    // The LP can still collect its fees under an override.
    collect_concentrated_fees(&factory, &router, pool_key, position_id, wallet)
        .expect("collect fees should succeed under an override");
}
