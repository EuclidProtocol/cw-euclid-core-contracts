#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{Addr, Uint128, Uint256};
use cw_orch::prelude::*;
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
use euclid::msgs::vlp::concentrated::msg::{
    PositionResponse, QueryMsg as ConcentratedQueryMsg, Slot0Response,
};
use euclid::normalize::normalize_token_to_voucher;
use rstest::rstest;

use super::utils::{first_position_id, voucher_balance};
use crate::helpers::chains::get_concentrated_vlp;
use crate::helpers::factory::{
    add_concentrated_liquidity, create_concentrated_pool, remove_concentrated_liquidity,
};
use crate::tests_reusable::concentrated_create_pool::{
    pair_with_amounts, setup_concentrated_env_with_decimals,
};
use crate::tests_reusable::constants::{
    FACTORY_CHAIN_ID_EVM, FACTORY_CHAIN_ID_IBC, FACTORY_CHAIN_ID_LOCAL,
};
use crate::tests_reusable::factory_register::FactorySetupMode;

#[cfg(test)]
mod tests {
    use super::*;

    #[rstest]
    #[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
    #[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
    #[case(FactorySetupMode::Evm, FACTORY_CHAIN_ID_EVM)]
    fn test_concentrated_pool_6dec_and_18dec(
        #[case] mode: FactorySetupMode,
        #[case] factory_chain_id: &str,
    ) {
        let (_interchain, factory, router, token_a, token_b) =
            setup_concentrated_env_with_decimals(mode, factory_chain_id, 6, 18);

        let raw_a: u128 = 1_000_000;
        let raw_b: u128 = 1_000_000_000_000_000_000;
        let pair = pair_with_amounts(&token_a, &token_b, raw_a, raw_b);
        let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 10_000).unwrap();

        let vb_a = voucher_balance(&factory, &router, &token_a.token.to_string());
        let vb_b = voucher_balance(&factory, &router, &token_b.token.to_string());

        let expected_a = normalize_token_to_voucher(Uint256::from(raw_a), 6).unwrap();
        let expected_b = normalize_token_to_voucher(Uint256::from(raw_b), 18).unwrap();

        // Both should normalize to 10^24 (1 unit in voucher space)
        assert_eq!(expected_a, expected_b);

        // After pool creation, user's voucher balance should be near zero
        // (all deposited into VLP). Allow for rounding from slippage.
        assert!(
            vb_a < expected_a,
            "user VB for token_a should have decreased: vb={vb_a}, deposited={expected_a}"
        );
        assert!(
            vb_b < expected_b,
            "user VB for token_b should have decreased: vb={vb_b}, deposited={expected_b}"
        );

        // Verify VLP has non-zero liquidity at the correct tick
        let vlp_address = router.get_vlp_by_pool_key(pool_key).unwrap().vlp;
        let vlp = get_concentrated_vlp(router.environment(), &Addr::unchecked(vlp_address));
        let slot0: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();
        assert!(
            slot0.liquidity > Uint128::zero(),
            "VLP should have non-zero liquidity"
        );
    }

    #[rstest]
    #[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
    #[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
    #[case(FactorySetupMode::Evm, FACTORY_CHAIN_ID_EVM)]
    fn test_concentrated_create_remove_voucher_balance_roundtrip(
        #[case] mode: FactorySetupMode,
        #[case] factory_chain_id: &str,
    ) {
        let (_interchain, factory, router, token_a, token_b) =
            setup_concentrated_env_with_decimals(mode, factory_chain_id, 6, 18);

        let vb_a_before = voucher_balance(&factory, &router, &token_a.token.to_string());
        let vb_b_before = voucher_balance(&factory, &router, &token_b.token.to_string());
        assert!(vb_a_before.is_zero());
        assert!(vb_b_before.is_zero());

        let raw_a: u128 = 500_000;
        let raw_b: u128 = 500_000_000_000_000_000;
        let pair = pair_with_amounts(&token_a, &token_b, raw_a, raw_b);
        let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 10_000).unwrap();

        // After pool creation, VLP should hold liquidity. User gets leftover.
        let vb_a_after_create = voucher_balance(&factory, &router, &token_a.token.to_string());
        let vb_b_after_create = voucher_balance(&factory, &router, &token_b.token.to_string());

        // Remove all liquidity from the position created during pool creation
        let position_id = first_position_id(&factory);
        let vlp_address = router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp;
        let vlp = get_concentrated_vlp(router.environment(), &Addr::unchecked(vlp_address));
        let pos: PositionResponse = vlp
            .query(&ConcentratedQueryMsg::Position { position_id })
            .unwrap();
        assert!(
            pos.liquidity > Uint128::zero(),
            "position should have non-zero liquidity"
        );

        remove_concentrated_liquidity(&factory, &router, pool_key, position_id, pos.liquidity)
            .unwrap();

        let vb_a_after_remove = voucher_balance(&factory, &router, &token_a.token.to_string());
        let vb_b_after_remove = voucher_balance(&factory, &router, &token_b.token.to_string());

        // After removing all liquidity, user should get back approximately what was deposited
        assert!(
            vb_a_after_remove > vb_a_after_create,
            "VB for token_a should increase after remove: before={vb_a_after_create}, after={vb_a_after_remove}"
        );
        assert!(
            vb_b_after_remove > vb_b_after_create,
            "VB for token_b should increase after remove: before={vb_b_after_create}, after={vb_b_after_remove}"
        );

        // The total returned (leftover + removed) should be close to the deposited normalized amount
        let expected_a = normalize_token_to_voucher(Uint256::from(raw_a), 6).unwrap();
        let expected_b = normalize_token_to_voucher(Uint256::from(raw_b), 18).unwrap();
        let diff_a = expected_a.abs_diff(vb_a_after_remove);
        let diff_b = expected_b.abs_diff(vb_b_after_remove);
        // Allow up to 1% deviation for rounding
        assert!(
            diff_a <= expected_a / Uint256::from(100u128),
            "VB roundtrip for token_a off: expected~{expected_a}, got={vb_a_after_remove}, diff={diff_a}"
        );
        assert!(
            diff_b <= expected_b / Uint256::from(100u128),
            "VB roundtrip for token_b off: expected~{expected_b}, got={vb_b_after_remove}, diff={diff_b}"
        );
    }
}
