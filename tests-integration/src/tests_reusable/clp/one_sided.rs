#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{Addr, Uint128};
use cw_orch::prelude::*;
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
use euclid::msgs::vlp::concentrated::msg::{
    PositionResponse, QueryMsg as ConcentratedQueryMsg, Slot0Response,
};
use rstest::rstest;
use rstest_reuse::apply;

use super::utils::{last_position_id, raw_units, scaled_pair, setup_clp};
use crate::helpers::chains::get_concentrated_vlp;
use crate::helpers::factory::{
    add_concentrated_liquidity, create_concentrated_pool, list_position_ids,
    remove_concentrated_liquidity,
};
use crate::tests_reusable::concentrated_create_pool::pair_with_amounts;
use crate::tests_reusable::factory_register::FactorySetupMode;
use crate::tests_reusable::test_macros::clp_matrix;

#[cfg(test)]
mod tests {
    use super::*;

    /// Position below current tick needs only token_1. Providing exactly 0
    /// for token_0 (a true one-sided add, no dust required) with max slippage
    /// should succeed and use only token_1.
    #[apply(clp_matrix)]
    fn test_one_sided_below_tick_only_token_1(mode: FactorySetupMode, decimal_pair: (u32, u32)) {
        let (decimals_a, decimals_b) = decimal_pair;
        let (_interchain, factory, router, token_a, token_b) =
            setup_clp(mode, decimals_a, decimals_b);
        let pair = scaled_pair(&token_a, &token_b, 30, decimals_a, 30, decimals_b);
        let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

        let vlp_addr = Addr::unchecked(router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp);
        let vlp = get_concentrated_vlp(router.environment(), &vlp_addr);
        let slot0: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();

        let lower = ((slot0.tick - 500) / 10) * 10;
        let upper = ((slot0.tick - 100) / 10) * 10;

        let add_resp = add_concentrated_liquidity(
            &factory,
            &router,
            pair_with_amounts(&token_a, &token_b, 0, raw_units(10, decimals_b)),
            pool_key.clone(),
            lower,
            upper,
            None,
            10_000,
        )
        .unwrap();

        assert!(!add_resp.liquidity_delta.is_zero(), "should mint liquidity");
        assert_eq!(
            add_resp.used_token_1,
            Uint128::zero(),
            "token_0 should not be used for below-tick position",
        );
        assert!(
            add_resp.used_token_2 > Uint128::zero(),
            "token_1 should be used for below-tick position",
        );
    }

    /// Position above current tick needs only token_0. Providing exactly 0
    /// for token_1 (a true one-sided add, no dust required) with max slippage
    /// should succeed and use only token_0.
    #[apply(clp_matrix)]
    fn test_one_sided_above_tick_only_token_0(mode: FactorySetupMode, decimal_pair: (u32, u32)) {
        let (decimals_a, decimals_b) = decimal_pair;
        let (_interchain, factory, router, token_a, token_b) =
            setup_clp(mode, decimals_a, decimals_b);
        let pair = scaled_pair(&token_a, &token_b, 30, decimals_a, 30, decimals_b);
        let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

        let vlp_addr = Addr::unchecked(router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp);
        let vlp = get_concentrated_vlp(router.environment(), &vlp_addr);
        let slot0: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();

        let lower = ((slot0.tick + 100) / 10) * 10;
        let upper = ((slot0.tick + 500) / 10) * 10;

        let add_resp = add_concentrated_liquidity(
            &factory,
            &router,
            pair_with_amounts(&token_a, &token_b, raw_units(10, decimals_a), 0),
            pool_key.clone(),
            lower,
            upper,
            None,
            10_000,
        )
        .unwrap();

        assert!(!add_resp.liquidity_delta.is_zero(), "should mint liquidity");
        assert!(
            add_resp.used_token_1 > Uint128::zero(),
            "token_0 should be used for above-tick position",
        );
        assert_eq!(
            add_resp.used_token_2,
            Uint128::zero(),
            "token_1 should not be used for above-tick position",
        );
    }

    /// Both legs zero is rejected at the factory (ZeroAssetAmount): there is
    /// nothing to add. A single zero leg is legal (one-sided add, above).
    #[apply(clp_matrix)]
    fn test_one_sided_both_zero_amounts_rejected(mode: FactorySetupMode, decimal_pair: (u32, u32)) {
        let (decimals_a, decimals_b) = decimal_pair;
        let (_interchain, factory, router, token_a, token_b) =
            setup_clp(mode, decimals_a, decimals_b);
        let pair = scaled_pair(&token_a, &token_b, 30, decimals_a, 30, decimals_b);
        let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

        let vlp_addr = Addr::unchecked(router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp);
        let vlp = get_concentrated_vlp(router.environment(), &vlp_addr);
        let slot0: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();

        let lower = ((slot0.tick - 500) / 10) * 10;
        let upper = ((slot0.tick - 100) / 10) * 10;

        let err = add_concentrated_liquidity(
            &factory,
            &router,
            pair_with_amounts(&token_a, &token_b, 0, 0),
            pool_key,
            lower,
            upper,
            None,
            10_000,
        );
        let err = err.expect_err("both-zero add must be rejected at the factory");
        // Anchor on the variant's own display string (cw-orch stringifies the
        // contract error) so a reworded message cannot detach this assertion.
        let expected = euclid::error::ContractError::ZeroAssetAmount {}.to_string();
        assert!(
            format!("{err:?}").contains(&expected),
            "expected ZeroAssetAmount ({expected}), got: {err:?}",
        );
    }

    /// Providing both tokens for a below-tick position with tight slippage
    /// should fail because token_0 is entirely unused.
    #[apply(clp_matrix)]
    fn test_one_sided_below_tick_both_tokens_tight_slippage_fails(
        mode: FactorySetupMode,
        decimal_pair: (u32, u32),
    ) {
        let (decimals_a, decimals_b) = decimal_pair;
        let (_interchain, factory, router, token_a, token_b) =
            setup_clp(mode, decimals_a, decimals_b);
        let pair = scaled_pair(&token_a, &token_b, 30, decimals_a, 30, decimals_b);
        let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

        let vlp_addr = Addr::unchecked(router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp);
        let vlp = get_concentrated_vlp(router.environment(), &vlp_addr);
        let slot0: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();

        let lower = ((slot0.tick - 500) / 10) * 10;
        let upper = ((slot0.tick - 100) / 10) * 10;

        let err = add_concentrated_liquidity(
            &factory,
            &router,
            scaled_pair(&token_a, &token_b, 5, decimals_a, 5, decimals_b),
            pool_key,
            lower,
            upper,
            None,
            100,
        );
        assert!(
            err.is_err(),
            "below-tick position with both tokens and tight slippage should fail",
        );
    }

    /// Providing both tokens for an above-tick position with tight slippage
    /// should fail because token_1 is entirely unused.
    #[apply(clp_matrix)]
    fn test_one_sided_above_tick_both_tokens_tight_slippage_fails(
        mode: FactorySetupMode,
        decimal_pair: (u32, u32),
    ) {
        let (decimals_a, decimals_b) = decimal_pair;
        let (_interchain, factory, router, token_a, token_b) =
            setup_clp(mode, decimals_a, decimals_b);
        let pair = scaled_pair(&token_a, &token_b, 30, decimals_a, 30, decimals_b);
        let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

        let vlp_addr = Addr::unchecked(router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp);
        let vlp = get_concentrated_vlp(router.environment(), &vlp_addr);
        let slot0: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();

        let lower = ((slot0.tick + 100) / 10) * 10;
        let upper = ((slot0.tick + 500) / 10) * 10;

        let err = add_concentrated_liquidity(
            &factory,
            &router,
            scaled_pair(&token_a, &token_b, 5, decimals_a, 5, decimals_b),
            pool_key,
            lower,
            upper,
            None,
            100,
        );
        assert!(
            err.is_err(),
            "above-tick position with both tokens and tight slippage should fail",
        );
    }

    /// Providing both tokens with max slippage (100%) should succeed for OOR
    /// positions, unused token is fully refunded.
    #[apply(clp_matrix)]
    fn test_one_sided_below_tick_both_tokens_max_slippage_succeeds(
        mode: FactorySetupMode,
        decimal_pair: (u32, u32),
    ) {
        let (decimals_a, decimals_b) = decimal_pair;
        let (_interchain, factory, router, token_a, token_b) =
            setup_clp(mode, decimals_a, decimals_b);
        let pair = scaled_pair(&token_a, &token_b, 30, decimals_a, 30, decimals_b);
        let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

        let vlp_addr = Addr::unchecked(router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp);
        let vlp = get_concentrated_vlp(router.environment(), &vlp_addr);
        let slot0: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();

        let lower = ((slot0.tick - 500) / 10) * 10;
        let upper = ((slot0.tick - 100) / 10) * 10;

        let add_resp = add_concentrated_liquidity(
            &factory,
            &router,
            scaled_pair(&token_a, &token_b, 5, decimals_a, 5, decimals_b),
            pool_key,
            lower,
            upper,
            None,
            10_000,
        )
        .unwrap();

        assert!(!add_resp.liquidity_delta.is_zero());
        assert_eq!(add_resp.used_token_1, Uint128::zero());
        assert!(add_resp.used_token_2 > Uint128::zero());
    }

    /// One-sided position (true zero on the unused side) can be fully removed.
    #[apply(clp_matrix)]
    fn test_one_sided_position_full_remove(mode: FactorySetupMode, decimal_pair: (u32, u32)) {
        let (decimals_a, decimals_b) = decimal_pair;
        let (_interchain, factory, router, token_a, token_b) =
            setup_clp(mode, decimals_a, decimals_b);
        let pair = scaled_pair(&token_a, &token_b, 30, decimals_a, 30, decimals_b);
        let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

        let vlp_addr = Addr::unchecked(router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp);
        let vlp = get_concentrated_vlp(router.environment(), &vlp_addr);
        let slot0: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();

        let lower = ((slot0.tick + 100) / 10) * 10;
        let upper = ((slot0.tick + 500) / 10) * 10;

        add_concentrated_liquidity(
            &factory,
            &router,
            pair_with_amounts(&token_a, &token_b, raw_units(10, decimals_a), 0),
            pool_key.clone(),
            lower,
            upper,
            None,
            10_000,
        )
        .unwrap();

        let pos_id = last_position_id(&factory);
        let pos: PositionResponse = vlp
            .query(&ConcentratedQueryMsg::Position {
                position_id: pos_id,
            })
            .unwrap();
        assert!(!pos.liquidity.is_zero());

        remove_concentrated_liquidity(&factory, &router, pool_key, pos_id, pos.liquidity).unwrap();

        let pos_result: Result<PositionResponse, _> = vlp.query(&ConcentratedQueryMsg::Position {
            position_id: pos_id,
        });
        assert!(
            pos_result.is_err(),
            "one-sided position should be deleted after full removal",
        );
    }

    /// Adding more liquidity to an existing one-sided position with a true
    /// zero on the unused side should succeed.
    #[apply(clp_matrix)]
    fn test_one_sided_add_to_existing_position(mode: FactorySetupMode, decimal_pair: (u32, u32)) {
        let (decimals_a, decimals_b) = decimal_pair;
        let (_interchain, factory, router, token_a, token_b) =
            setup_clp(mode, decimals_a, decimals_b);
        let pair = scaled_pair(&token_a, &token_b, 30, decimals_a, 30, decimals_b);
        let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

        let vlp_addr = Addr::unchecked(router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp);
        let vlp = get_concentrated_vlp(router.environment(), &vlp_addr);
        let slot0: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();

        let lower = ((slot0.tick - 500) / 10) * 10;
        let upper = ((slot0.tick - 100) / 10) * 10;

        add_concentrated_liquidity(
            &factory,
            &router,
            pair_with_amounts(&token_a, &token_b, 0, raw_units(5, decimals_b)),
            pool_key.clone(),
            lower,
            upper,
            None,
            10_000,
        )
        .unwrap();

        let pos_id = last_position_id(&factory);
        let pos_before: PositionResponse = vlp
            .query(&ConcentratedQueryMsg::Position {
                position_id: pos_id,
            })
            .unwrap();

        let add_resp = add_concentrated_liquidity(
            &factory,
            &router,
            pair_with_amounts(&token_a, &token_b, 0, raw_units(5, decimals_b)),
            pool_key,
            lower,
            upper,
            Some(pos_id),
            10_000,
        )
        .unwrap();

        assert!(!add_resp.liquidity_delta.is_zero());
        assert_eq!(add_resp.used_token_1, Uint128::zero());

        let pos_after: PositionResponse = vlp
            .query(&ConcentratedQueryMsg::Position {
                position_id: pos_id,
            })
            .unwrap();
        assert_eq!(
            pos_after.liquidity,
            pos_before.liquidity + add_resp.liquidity_delta,
        );

        let ids = list_position_ids(&factory).unwrap();
        let oor_count = ids
            .iter()
            .filter(|id| id.as_str() == pos_id.to_string())
            .count();
        assert_eq!(oor_count, 1, "should still be same single position");
    }

    /// Providing mainly the wrong token for tick direction with tight slippage
    /// should fail. Below tick needs token_1; providing lots of token_0 with
    /// tight slippage fails because token_0 is 100% unused. Same for above.
    #[apply(clp_matrix)]
    fn test_one_sided_wrong_token_tight_slippage_fails(
        mode: FactorySetupMode,
        decimal_pair: (u32, u32),
    ) {
        let (decimals_a, decimals_b) = decimal_pair;
        let (_interchain, factory, router, token_a, token_b) =
            setup_clp(mode, decimals_a, decimals_b);
        let pair = scaled_pair(&token_a, &token_b, 30, decimals_a, 30, decimals_b);
        let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

        let vlp_addr = Addr::unchecked(router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp);
        let vlp = get_concentrated_vlp(router.environment(), &vlp_addr);
        let slot0: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();

        // Below tick needs token_1. Provide heavy token_0 + small token_1,
        // tight slippage, token_0 fully unused -> slippage error
        let lower = ((slot0.tick - 500) / 10) * 10;
        let upper = ((slot0.tick - 100) / 10) * 10;

        let err = add_concentrated_liquidity(
            &factory,
            &router,
            pair_with_amounts(
                &token_a,
                &token_b,
                raw_units(10, decimals_a),
                raw_units(1, decimals_b),
            ),
            pool_key.clone(),
            lower,
            upper,
            None,
            100,
        );
        assert!(
            err.is_err(),
            "below-tick: heavy wrong token + tight slippage should fail",
        );

        // Above tick needs token_0. Provide heavy token_1 + small token_0,
        // tight slippage, token_1 fully unused -> slippage error
        let lower_above = ((slot0.tick + 100) / 10) * 10;
        let upper_above = ((slot0.tick + 500) / 10) * 10;

        let err = add_concentrated_liquidity(
            &factory,
            &router,
            pair_with_amounts(
                &token_a,
                &token_b,
                raw_units(1, decimals_a),
                raw_units(10, decimals_b),
            ),
            pool_key,
            lower_above,
            upper_above,
            None,
            100,
        );
        assert!(
            err.is_err(),
            "above-tick: heavy wrong token + tight slippage should fail",
        );
    }
}
