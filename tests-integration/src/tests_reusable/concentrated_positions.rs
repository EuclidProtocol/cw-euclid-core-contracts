#![cfg(not(target_arch = "wasm32"))]

use concentrated_vlp::math::liquidity_amounts::get_liquidity_for_amounts;
use concentrated_vlp::math::tick_math::get_sqrt_ratio_at_tick;
use cosmwasm_std::{Addr, Uint128, Uint256};
use cw_orch::prelude::*;
use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
use euclid::msgs::vlp::concentrated::msg::QueryMsgFns as ConcentratedQueryMsgFns;
use rstest::rstest;
use std::collections::HashSet;

use crate::helpers::chains::get_concentrated_vlp;
use crate::helpers::factory::{
    add_concentrated_liquidity, create_concentrated_pool, get_position_token, list_position_ids,
    remove_concentrated_liquidity,
};
use crate::tests_reusable::concentrated_create_pool::{
    pair_with_amounts, setup_concentrated_env, setup_concentrated_env_ext,
};
use crate::tests_reusable::concentrated_swap::execute_concentrated_swap;
use crate::tests_reusable::factory_register::FactorySetupMode;
use euclid::liquidity::{MAX_TICK, MIN_TICK};

fn first_position_id(factory: &factory::FactoryContract<cw_orch::mock::MockBase>) -> Uint128 {
    let ids = list_position_ids(factory).unwrap();
    assert!(!ids.is_empty(), "expected at least one position");
    Uint128::new(ids[0].parse::<u128>().unwrap())
}

fn pool_lp_shares(
    factory: &factory::FactoryContract<cw_orch::mock::MockBase>,
    router: &router::RouterContract<cw_orch::mock::MockBase>,
    pool_key: euclid::msgs::vlp::base::PoolKey,
) -> Uint128 {
    let vlp_address = router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp;
    let vlp = get_concentrated_vlp(router.environment(), &Addr::unchecked(vlp_address));
    let chain_uid = factory.get_state().unwrap().chain_uid;
    let pool: euclid::msgs::vlp::concentrated::msg::ConcentratedPoolResponse =
        vlp.pool(chain_uid, pool_key).unwrap();
    pool.lp_shares
}

// Cross-VM coverage: testing/euclid-tests/tests/protocol/concentrated/positions.rs::clp_position_lifecycle_on_all — EVM + Cosmos
#[rstest]
fn test_add_liquidity_mints_position_nft(
    #[values(FactorySetupMode::Native, FactorySetupMode::Ibc, FactorySetupMode::Evm)]
    mode: FactorySetupMode,
    #[values(
        10_000_000_u128,
        1_000_000_000_000_000_000_u128,
        10_000_000_000_000_000_000_u128
    )]
    initial_amount_a: u128,
    #[values(
        10_000_000_u128,
        1_000_000_000_000_000_000_u128,
        10_000_000_000_000_000_000_u128
    )]
    initial_amount_b: u128,
) {
    use euclid::utils::pagination::Pagination;

    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, mode.chain_id());
    let pair = pair_with_amounts(&token_a, &token_b, initial_amount_a, initial_amount_b);
    let _pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

    let position_token = get_position_token(&factory).unwrap();
    let tokens = position_token
        .query::<euclid::msgs::position_token::TokensResponse>(
            &euclid::msgs::position_token::QueryMsg::AllTokens {
                pagination: Pagination::default(),
            },
        )
        .unwrap()
        .tokens;
    assert_eq!(tokens.len(), 1, "initial add should mint exactly one NFT");

    let owner = position_token
        .query::<euclid::msgs::position_token::OwnerOfResponse>(
            &euclid::msgs::position_token::QueryMsg::OwnerOf {
                token_id: tokens[0].clone(),
            },
        )
        .unwrap()
        .owner;
    assert_eq!(owner, factory.environment().sender.to_string());
}

// Cross-VM coverage: testing/euclid-tests/tests/protocol/concentrated/positions.rs::clp_position_lifecycle_on_all — EVM + Cosmos
#[rstest]
fn test_increase_liquidity_updates_same_position(
    #[values(FactorySetupMode::Native, FactorySetupMode::Ibc, FactorySetupMode::Evm)]
    mode: FactorySetupMode,
    #[values(
        10_000_000_u128,
        1_000_000_000_000_000_000_u128,
        10_000_000_000_000_000_000_u128
    )]
    initial_amount_a: u128,
    #[values(
        10_000_000_u128,
        1_000_000_000_000_000_000_u128,
        10_000_000_000_000_000_000_u128
    )]
    initial_amount_b: u128,
    #[values(5_000_u128)] increase_amount: u128,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, mode.chain_id());
    let pair = pair_with_amounts(&token_a, &token_b, initial_amount_a, initial_amount_b);
    let pool_key = create_concentrated_pool(&factory, &router, pair.clone(), 500, 10, 100).unwrap();

    let initial_position_id = first_position_id(&factory);
    let vlp_address = router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp;
    let vlp = get_concentrated_vlp(router.environment(), &Addr::unchecked(vlp_address));
    let initial_position: euclid::msgs::vlp::concentrated::msg::PositionResponse =
        vlp.position(initial_position_id).unwrap();

    let increase_amount_a = initial_amount_a.div_ceil(increase_amount);
    let increase_amount_b = initial_amount_b.div_ceil(increase_amount);

    // The VLP operates in voucher units (24 decimals). For 6-decimal tokens
    // the normalization factor is 10^18. Compute expected liquidity from
    // voucher-unit amounts to match what the VLP will calculate.
    let voucher_scale = 10u128.pow(18);
    let voucher_increase_a = increase_amount_a.checked_mul(voucher_scale).unwrap();
    let voucher_increase_b = increase_amount_b.checked_mul(voucher_scale).unwrap();

    let slot0 = vlp.slot_0().unwrap();
    let sqrt_price_x96 = slot0.sqrt_price_x96;
    let sqrt_lower_x96 = get_sqrt_ratio_at_tick(initial_position.lower_tick_index).unwrap();
    let sqrt_upper_x96 = get_sqrt_ratio_at_tick(initial_position.upper_tick_index).unwrap();
    let liquidity = get_liquidity_for_amounts(
        sqrt_price_x96,
        sqrt_lower_x96,
        sqrt_upper_x96,
        Uint128::new(voucher_increase_a),
        Uint128::new(voucher_increase_b),
    )
    .unwrap();

    add_concentrated_liquidity(
        &factory,
        &router,
        pair_with_amounts(&token_a, &token_b, increase_amount_a, increase_amount_b),
        pool_key,
        initial_position.lower_tick_index,
        initial_position.upper_tick_index,
        Some(initial_position_id),
        100,
    )
    .unwrap();

    let ids = list_position_ids(&factory).unwrap();
    assert_eq!(ids.len(), 1, "increase should not mint a new position NFT");
    assert_eq!(ids[0], initial_position_id.to_string());

    let after_add: euclid::msgs::vlp::concentrated::msg::PositionResponse =
        vlp.position(initial_position_id).unwrap();
    assert_eq!(
        after_add.liquidity,
        initial_position.liquidity + liquidity,
        "liquidity should increase by the amount of liquidity added"
    );
    assert_eq!(
        after_add.lower_tick_index, initial_position.lower_tick_index,
        "lower tick index should not change"
    );
    assert_eq!(
        after_add.upper_tick_index, initial_position.upper_tick_index,
        "upper tick index should not change"
    );
}

// Cross-VM coverage: testing/euclid-tests/tests/protocol/concentrated/positions.rs::clp_position_lifecycle_on_all — EVM + Cosmos
#[rstest]
fn test_partial_decrease_keeps_position(
    #[values(FactorySetupMode::Native, FactorySetupMode::Ibc, FactorySetupMode::Evm)]
    mode: FactorySetupMode,
    #[values(
        10_000_000_u128,
        1_000_000_000_000_000_000_u128,
        10_000_000_000_000_000_000_u128
    )]
    initial_amount_a: u128,
    #[values(
        10_000_000_u128,
        1_000_000_000_000_000_000_u128,
        10_000_000_000_000_000_000_u128
    )]
    initial_amount_b: u128,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, mode.chain_id());
    let pair = pair_with_amounts(&token_a, &token_b, initial_amount_a, initial_amount_b);
    let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

    let position_id = first_position_id(&factory);
    let total_lp_shares = pool_lp_shares(&factory, &router, pool_key.clone());
    let partial = Uint128::new((total_lp_shares.u128() / 2).max(1));
    remove_concentrated_liquidity(&factory, &router, pool_key, position_id, partial).unwrap();

    let ids = list_position_ids(&factory).unwrap();
    assert!(
        ids.iter().any(|id| id == &position_id.to_string()),
        "position NFT should remain after partial remove",
    );
}

// Cross-VM coverage: testing/euclid-tests/tests/protocol/concentrated/positions.rs::clp_position_lifecycle_on_all — EVM + Cosmos
#[rstest]
fn test_full_remove_burns_position(
    #[values(FactorySetupMode::Native, FactorySetupMode::Ibc, FactorySetupMode::Evm)]
    mode: FactorySetupMode,
    #[values(
        10_000_000_u128,
        1_000_000_000_000_000_000_u128,
        10_000_000_000_000_000_000_u128
    )]
    initial_amount_a: u128,
    #[values(
        10_000_000_u128,
        1_000_000_000_000_000_000_u128,
        10_000_000_000_000_000_000_u128
    )]
    initial_amount_b: u128,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, mode.chain_id());
    let pair = pair_with_amounts(&token_a, &token_b, initial_amount_a, initial_amount_b);
    let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

    let position_id = first_position_id(&factory);
    let total_lp_shares = pool_lp_shares(&factory, &router, pool_key.clone());
    remove_concentrated_liquidity(&factory, &router, pool_key, position_id, total_lp_shares)
        .unwrap();

    let ids = list_position_ids(&factory).unwrap();
    assert!(ids.is_empty(), "position NFT must be burned on full remove");
}

// Cross-VM coverage:
//   testing/euclid-tests/tests/protocol/concentrated/authorization.rs::clp_non_owner_cannot_remove
//   testing/euclid-tests/tests/protocol/concentrated/authorization.rs::clp_non_owner_cannot_increase
#[test]
fn test_only_owner_can_modify_or_collect() {
    let mode = FactorySetupMode::Native;
    let (_interchain, mut factory, router, token_a, token_b) =
        setup_concentrated_env(mode, mode.chain_id());
    let pair = pair_with_amounts(&token_a, &token_b, 20_000, 20_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair.clone(), 500, 10, 100).unwrap();
    let position_id = first_position_id(&factory);

    let intruder = factory.environment().addr_make("intruder");
    factory.set_sender(&intruder);

    let add_err = add_concentrated_liquidity(
        &factory,
        &router,
        pair_with_amounts(&token_a, &token_b, 1_000, 1_000),
        pool_key.clone(),
        -120,
        120,
        Some(position_id),
        100,
    )
    .unwrap_err();
    assert!(
        !add_err.to_string().is_empty(),
        "expected unauthorized add liquidity to fail",
    );

    let remove_err =
        remove_concentrated_liquidity(&factory, &router, pool_key, position_id, Uint128::new(1))
            .unwrap_err();
    assert!(
        !remove_err.to_string().is_empty(),
        "expected unauthorized remove liquidity to fail",
    );
}

// Cross-VM coverage: none (CosmWasm-only)
#[rstest]
fn test_multiple_positions_different_ranges_are_independent(
    #[values(FactorySetupMode::Native, FactorySetupMode::Ibc, FactorySetupMode::Evm)]
    mode: FactorySetupMode,
    #[values(
        10_000_000_u128,
        1_000_000_000_000_000_000_u128,
        10_000_000_000_000_000_000_u128
    )]
    initial_amount_a: u128,
    #[values(
        10_000_000_u128,
        1_000_000_000_000_000_000_u128,
        10_000_000_000_000_000_000_u128
    )]
    initial_amount_b: u128,
    #[values(5_000_u128)] second_amount: u128,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, mode.chain_id());
    let pair = pair_with_amounts(&token_a, &token_b, initial_amount_a, initial_amount_b);
    let pool_key = create_concentrated_pool(&factory, &router, pair.clone(), 500, 10, 100).unwrap();

    let first_ids = list_position_ids(&factory).unwrap();
    assert_eq!(first_ids.len(), 1);
    let first_id = Uint128::new(first_ids[0].parse::<u128>().unwrap());

    let lp_before_second = pool_lp_shares(&factory, &router, pool_key.clone());
    add_concentrated_liquidity(
        &factory,
        &router,
        // Current tick is centered around 0 after pool creation.
        // This range is entirely below spot, so one side will be mostly unused.
        pair_with_amounts(&token_a, &token_b, second_amount, second_amount),
        pool_key.clone(),
        -240,
        -120,
        None,
        10_000,
    )
    .unwrap();
    let lp_after_second = pool_lp_shares(&factory, &router, pool_key.clone());
    let second_position_liquidity = lp_after_second.checked_sub(lp_before_second).unwrap();
    assert!(
        second_position_liquidity > Uint128::zero(),
        "second range should mint non-zero liquidity",
    );

    let second_ids = list_position_ids(&factory).unwrap();
    assert_eq!(
        second_ids.len(),
        2,
        "adding with None position_id should mint a new NFT"
    );

    let first_id_set: HashSet<&str> = first_ids.iter().map(String::as_str).collect();
    let second_id = second_ids
        .iter()
        .find(|id| !first_id_set.contains(id.as_str()))
        .expect("must contain a newly minted position id");
    let second_id = Uint128::new(second_id.parse::<u128>().unwrap());

    remove_concentrated_liquidity(
        &factory,
        &router,
        pool_key,
        second_id,
        second_position_liquidity,
    )
    .unwrap();

    let final_ids = list_position_ids(&factory).unwrap();
    assert_eq!(
        final_ids.len(),
        1,
        "removing second position should not remove first"
    );
    assert_eq!(final_ids[0], first_id.to_string());
}

/// Covers full-range position creation on a pool using the finest tick
/// spacing (1) and smallest fee tier (100 bps), with mixed native+smart
/// token kinds, highly asymmetric seed amounts, and tight slippage.
/// Mirrors a "full-range LP" entry scenario.
///
/// Amounts are chosen so that after voucher normalization (6 dec tokens
/// are multiplied by 10^18) they remain within Uint128 bounds.
// Cross-VM coverage: none (CosmWasm-only)
#[rstest]
fn test_add_full_range_position_with_fine_spacing(
    #[values(FactorySetupMode::Native, FactorySetupMode::Ibc, FactorySetupMode::Evm)]
    mode: FactorySetupMode,
    #[values("native", "smart")] kind_a: &str,
    #[values("native", "smart")] kind_b: &str,
    #[values(
        (1_000_000, 500_000_000_000, MIN_TICK, MAX_TICK),
        (1_000_000, 500_000_000_000, 115100, 115150)
    )]
    add_info: (u128, u128, i64, i64),
) {
    let (add_a, add_b, add_lower_tick, add_upper_tick) = add_info;
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env_ext(mode, mode.chain_id(), kind_a, kind_b);

    // Asymmetric reserves: token_a is small, token_b is large but safe
    // after voucher normalization (10^12 * 10^18 = 10^30, fits Uint128).
    let initial_reserve_a = 10_000_000;
    let initial_reserve_b = 1_000_000_000_000_u128;

    let swap_amount_a = 1_829_677_u128;

    let pair = pair_with_amounts(&token_a, &token_b, initial_reserve_a, initial_reserve_b);

    let fee_tier_bps = 100;
    let tick_spacing = 1;
    let pool_creation_slippage_bps = 3500;

    let pool_key = create_concentrated_pool(
        &factory,
        &router,
        pair,
        fee_tier_bps,
        tick_spacing,
        pool_creation_slippage_bps,
    )
    .unwrap();

    let vlp_addr = Addr::unchecked(router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp);
    let vlp = get_concentrated_vlp(router.environment(), &vlp_addr);

    let initial_slot0 = vlp.slot_0().unwrap();

    // execute_concentrated_swap only works with native/voucher tokens as
    // asset_in (smart tokens fail deposit). Swap with whichever side is
    // native; if both are smart, skip the swap entirely.
    let can_swap_a = !token_a.token_type.is_smart();
    let can_swap_b = !token_b.token_type.is_smart();
    if can_swap_a {
        let amount_out = execute_concentrated_swap(
            &factory,
            &router,
            pool_key.clone(),
            token_a.clone(),
            token_b.clone().token,
            Uint256::from(swap_amount_a),
        );
        assert!(
            amount_out > Uint256::zero(),
            "swap should produce non-zero output"
        );
    } else if can_swap_b {
        // When swapping B->A, use a proportionally larger amount since
        // token_b has much larger reserves.
        let swap_amount_b = initial_reserve_b / 5;
        let amount_out = execute_concentrated_swap(
            &factory,
            &router,
            pool_key.clone(),
            token_b.clone(),
            token_a.clone().token,
            Uint256::from(swap_amount_b),
        );
        assert!(
            amount_out > Uint256::zero(),
            "swap should produce non-zero output"
        );
    }
    // If both tokens are smart, no swap is performed; we still test that
    // adding a full-range position works at the initial price.

    let initial_ids = list_position_ids(&factory).unwrap();
    let initial_id_set: HashSet<&str> = initial_ids.iter().map(String::as_str).collect();

    let new_slot0 = vlp.slot_0().unwrap();

    if can_swap_a || can_swap_b {
        assert!(
            new_slot0.tick != initial_slot0.tick,
            "tick should change after swap with info {:?}",
            new_slot0
        );
    }
    // Full-range add with position_id=None must mint a new NFT spanning
    // the absolute min/max ticks (±887272, divisible by tick_spacing=1).
    // Use generous slippage since the swap moved the price away from the
    // initial ratio, so the add amounts may not match perfectly.
    add_concentrated_liquidity(
        &factory,
        &router,
        pair_with_amounts(&token_a, &token_b, add_a, add_b),
        pool_key.clone(),
        add_lower_tick,
        add_upper_tick,
        None,
        10_000,
    )
    .unwrap();

    let after_ids = list_position_ids(&factory).unwrap();
    assert_eq!(after_ids.len(), initial_ids.len() + 1);

    let new_id = after_ids
        .iter()
        .find(|id| !initial_id_set.contains(id.as_str()))
        .expect("new position id must be minted");
    let new_id = Uint128::new(new_id.parse::<u128>().unwrap());

    let vlp_addr = Addr::unchecked(router.get_vlp_by_pool_key(pool_key).unwrap().vlp);
    let vlp = get_concentrated_vlp(router.environment(), &vlp_addr);
    let pos = vlp.position(new_id).unwrap();
    assert_eq!(pos.lower_tick_index, add_lower_tick);
    assert_eq!(pos.upper_tick_index, add_upper_tick);
    assert!(
        pos.liquidity > Uint128::zero(),
        "full-range position must have non-zero liquidity",
    );
}

/// Exercises the unused-token slippage check on a full-range add after a
/// swap moves the price. The ratio of `add_a` to `add_b` controls how much
/// of each side is left unused, which is what `slippage_tolerance_bps` bounds.
/// Each case is a `(amounts, slippage_bps, slippage_fail)` triple: the add
/// must either succeed or fail with "Slippage tolerance exceeded".
///
/// Amounts are kept safe for voucher normalization (6 dec tokens * 10^18).
// Cross-VM coverage: testing/euclid-tests/tests/protocol/concentrated/positions.rs::clp_add_slippage_too_tight_errors
#[rstest]
fn test_add_concentrated_liquidity_slippage_errors(
    #[values(FactorySetupMode::Native)] mode: FactorySetupMode,
    #[values(
        // Symmetric amounts with max slippage: the pool is asymmetric
        // (token_b >> token_a) so one side will have large leftover,
        // but 100% slippage allows it.
        ((1_000_000_u128, 1_000_000_u128), 10_000_u64, false),
        // Same symmetric amounts with tight slippage (1%): the price
        // mismatch causes most of one side to be unused, exceeding tolerance.
        ((1_000_000_u128, 1_000_000_u128), 100_u64, true),
    )]
    case: ((u128, u128), u64, bool),
) {
    let ((add_a, add_b), add_slippage_bps, slippage_fail) = case;

    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, mode.chain_id());

    // Asymmetric reserves safe after voucher normalization:
    // 10^12 * 10^18 = 10^30 (fits Uint128).
    let initial_reserve_a = 10_000_000;
    let initial_reserve_b = 1_000_000_000_000_u128;

    let swap_amount_a = 1_829_677_u128;

    let pair = pair_with_amounts(&token_a, &token_b, initial_reserve_a, initial_reserve_b);

    let fee_tier_bps = 100;
    let tick_spacing = 1;
    let pool_slippage_bps = 3500;

    let pool_key = create_concentrated_pool(
        &factory,
        &router,
        pair,
        fee_tier_bps,
        tick_spacing,
        pool_slippage_bps,
    )
    .unwrap();

    let vlp_addr = Addr::unchecked(router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp);
    let vlp = get_concentrated_vlp(router.environment(), &vlp_addr);
    let initial_slot0 = vlp.slot_0().unwrap();

    let amount_out = execute_concentrated_swap(
        &factory,
        &router,
        pool_key.clone(),
        token_a.clone(),
        token_b.clone().token,
        Uint256::from(swap_amount_a),
    );
    assert!(
        amount_out > Uint256::zero(),
        "swap should produce non-zero output"
    );

    let new_slot0 = vlp.slot_0().unwrap();
    assert!(
        new_slot0.tick < initial_slot0.tick,
        "tick should have moved down after A->B swap, got {new_slot0:?}"
    );

    let initial_ids = list_position_ids(&factory).unwrap();
    let initial_id_set: HashSet<&str> = initial_ids.iter().map(String::as_str).collect();

    let result = add_concentrated_liquidity(
        &factory,
        &router,
        pair_with_amounts(&token_a, &token_b, add_a, add_b),
        pool_key.clone(),
        MIN_TICK,
        MAX_TICK,
        None,
        add_slippage_bps,
    );

    if slippage_fail {
        let err = format!(
            "{:?}",
            result.expect_err("expected slippage error, got success")
        );
        assert!(
            err.contains("Slippage tolerance exceeded"),
            "expected slippage error, got: {err}"
        );
        let after_ids = list_position_ids(&factory).unwrap();
        assert_eq!(
            after_ids.len(),
            initial_ids.len(),
            "no NFT should be minted when the add fails"
        );
    } else {
        result.expect("expected add to succeed");
        let after_ids = list_position_ids(&factory).unwrap();
        assert_eq!(after_ids.len(), initial_ids.len() + 1);
        let new_id = after_ids
            .iter()
            .find(|id| !initial_id_set.contains(id.as_str()))
            .expect("new position id must be minted");
        let new_id = Uint128::new(new_id.parse::<u128>().unwrap());
        let pos = vlp.position(new_id).unwrap();
        assert_eq!(pos.lower_tick_index, MIN_TICK);
        assert_eq!(pos.upper_tick_index, MAX_TICK);
        assert!(
            pos.liquidity > Uint128::zero(),
            "successful add must produce non-zero liquidity"
        );
    }
}

/// Regression: after many small swaps crossing a position boundary,
/// ACTIVE_LIQUIDITY must still match the sum of in-range positions.
/// Without the tick boundary rounding fix, the price can round back to a
/// just-crossed tick, desyncing the liquidity state.
// Cross-VM coverage: none (CosmWasm-only)
#[rstest]
fn test_active_liquidity_consistent_after_boundary_crossings(
    #[values(FactorySetupMode::Native)] mode: FactorySetupMode,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, mode.chain_id());

    // Small initial amounts → low liquidity → price moves easily
    let pair = pair_with_amounts(&token_a, &token_b, 1_000, 1_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

    let vlp_addr = Addr::unchecked(router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp);
    let vlp = get_concentrated_vlp(router.environment(), &vlp_addr);

    // Add a narrow position around current tick
    let slot0 = vlp.slot_0().unwrap();
    let lower = ((slot0.tick - 50) / 10) * 10;
    let upper = ((slot0.tick + 50) / 10) * 10;

    let pair2 = pair_with_amounts(&token_a, &token_b, 500, 500);
    add_concentrated_liquidity(
        &factory,
        &router,
        pair2,
        pool_key.clone(),
        lower,
        upper,
        None,
        10_000,
    )
    .expect("add narrow position should succeed");

    // Many small swaps back and forth to repeatedly cross the position boundary
    for _ in 0..10 {
        let _ = execute_concentrated_swap(
            &factory,
            &router,
            pool_key.clone(),
            token_b.clone(),
            token_a.token.clone(),
            Uint256::from(300u128),
        );
        let _ = execute_concentrated_swap(
            &factory,
            &router,
            pool_key.clone(),
            token_a.clone(),
            token_b.token.clone(),
            Uint256::from(300u128),
        );
    }

    // Verify ACTIVE_LIQUIDITY matches sum of in-range positions
    let slot0 = vlp.slot_0().unwrap();
    let position_ids = list_position_ids(&factory).unwrap();

    let mut in_range_liquidity: u128 = 0;
    for id_str in &position_ids {
        let id = Uint128::new(id_str.parse::<u128>().unwrap());
        let pos = vlp.position(id).unwrap();
        if pos.lower_tick_index <= slot0.tick && slot0.tick < pos.upper_tick_index {
            in_range_liquidity += pos.liquidity.u128();
        }
    }

    // Allow 1-tick boundary tolerance (V3 sets tick = crossed_tick - 1
    // even if price hasn't moved below that tick's sqrt_ratio)
    let price_tick =
        concentrated_vlp::math::tick_math::get_tick_at_sqrt_ratio(slot0.sqrt_price_x96).unwrap();
    let mut in_range_by_price: u128 = 0;
    for id_str in &position_ids {
        let id = Uint128::new(id_str.parse::<u128>().unwrap());
        let pos = vlp.position(id).unwrap();
        if pos.lower_tick_index <= price_tick && price_tick < pos.upper_tick_index {
            in_range_by_price += pos.liquidity.u128();
        }
    }

    assert!(
        slot0.liquidity.u128() == in_range_liquidity || slot0.liquidity.u128() == in_range_by_price,
        "ACTIVE_LIQUIDITY ({}) should match in-range positions \
         by tick ({}, tick={}) or by price ({}, price_tick={})",
        slot0.liquidity,
        in_range_liquidity,
        slot0.tick,
        in_range_by_price,
        price_tick,
    );
}
