#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{Uint128, Uint256};
use cw_orch::prelude::*;
use euclid::cross_chain_user::CrossChainUser;
use euclid::msgs::cross_chain_config::CrossChainConfig;
use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
use rstest::rstest;
use rstest_reuse::apply;

use super::utils::{first_position_id, raw_units, scaled_pair, setup_clp};
use crate::helpers::factory::{
    add_concentrated_liquidity, create_concentrated_pool, remove_concentrated_liquidity,
};
use crate::tests_reusable::concentrated_swap::execute_concentrated_swap;
use crate::tests_reusable::factory_register::FactorySetupMode;
use crate::tests_reusable::test_macros::clp_matrix;

#[cfg(test)]
mod tests {
    use super::*;

    #[apply(clp_matrix)]
    fn test_add_to_position_owned_by_another_user_rejected(
        mode: FactorySetupMode,
        decimal_pair: (u32, u32),
    ) {
        let (decimals_a, decimals_b) = decimal_pair;
        let (_interchain, mut factory, router, token_a, token_b) =
            setup_clp(mode, decimals_a, decimals_b);

        let pair = scaled_pair(&token_a, &token_b, 10, decimals_a, 10, decimals_b);
        let pool_key =
            create_concentrated_pool(&factory, &router, pair.clone(), 500, 10, 100).unwrap();

        add_concentrated_liquidity(
            &factory,
            &router,
            pair.clone(),
            pool_key.clone(),
            -120,
            120,
            None,
            100,
        )
        .unwrap();

        let position_id = first_position_id(&factory);

        let intruder = factory.environment().addr_make("intruder");
        factory.set_sender(&intruder);

        let intruder_pair = scaled_pair(&token_a, &token_b, 5, decimals_a, 5, decimals_b);
        let err = add_concentrated_liquidity(
            &factory,
            &router,
            intruder_pair,
            pool_key,
            -120,
            120,
            Some(position_id),
            100,
        );
        assert!(
            err.is_err(),
            "intruder should not add to another user's position"
        );
    }

    #[apply(clp_matrix)]
    fn test_remove_from_position_owned_by_another_user_rejected(
        mode: FactorySetupMode,
        decimal_pair: (u32, u32),
    ) {
        let (decimals_a, decimals_b) = decimal_pair;
        let (_interchain, mut factory, router, token_a, token_b) =
            setup_clp(mode, decimals_a, decimals_b);

        let pair = scaled_pair(&token_a, &token_b, 10, decimals_a, 10, decimals_b);
        let pool_key =
            create_concentrated_pool(&factory, &router, pair.clone(), 500, 10, 100).unwrap();

        add_concentrated_liquidity(
            &factory,
            &router,
            pair.clone(),
            pool_key.clone(),
            -120,
            120,
            None,
            100,
        )
        .unwrap();

        let position_id = first_position_id(&factory);

        let intruder = factory.environment().addr_make("intruder");
        factory.set_sender(&intruder);

        let err = remove_concentrated_liquidity(
            &factory,
            &router,
            pool_key,
            position_id,
            Uint128::new(1_000),
        );
        assert!(
            err.is_err(),
            "intruder should not remove from another user's position"
        );
    }

    #[apply(clp_matrix)]
    fn test_collect_fees_for_position_owned_by_another_user_rejected(
        mode: FactorySetupMode,
        decimal_pair: (u32, u32),
    ) {
        let (decimals_a, decimals_b) = decimal_pair;
        let (_interchain, mut factory, router, token_a, token_b) =
            setup_clp(mode, decimals_a, decimals_b);

        let pair = scaled_pair(&token_a, &token_b, 30, decimals_a, 30, decimals_b);
        let pool_key =
            create_concentrated_pool(&factory, &router, pair.clone(), 500, 10, 100).unwrap();

        add_concentrated_liquidity(
            &factory,
            &router,
            pair.clone(),
            pool_key.clone(),
            -120,
            120,
            None,
            100,
        )
        .unwrap();

        // Execute a swap to accrue fees
        execute_concentrated_swap(
            &factory,
            &router,
            pool_key.clone(),
            token_a.clone(),
            token_b.token.clone(),
            Uint256::from(raw_units(5, decimals_a)),
        );

        let position_id = first_position_id(&factory);

        let intruder = factory.environment().addr_make("intruder");
        factory.set_sender(&intruder);

        let chain_uid = factory.get_state().unwrap().chain_uid;
        let err = factory
            .execute(
                &euclid::msgs::factory::ExecuteMsg::CollectConcentratedFees {
                    pool_key: pool_key.clone(),
                    position_id,
                    recipient: CrossChainUser::new(chain_uid, intruder.to_string()),
                    cross_chain_config: CrossChainConfig::default(),
                },
                &[],
            )
            .unwrap_err();
        assert!(
            !err.to_string().is_empty(),
            "intruder should not collect fees for another user's position"
        );
    }

    #[apply(clp_matrix)]
    fn test_position_from_pool_a_cannot_be_used_in_pool_b(
        mode: FactorySetupMode,
        decimal_pair: (u32, u32),
    ) {
        let (decimals_a, decimals_b) = decimal_pair;
        let (_interchain, factory, router, token_a, token_b) =
            setup_clp(mode, decimals_a, decimals_b);

        // Create pool A (fee 500, spacing 10)
        let pair = scaled_pair(&token_a, &token_b, 20, decimals_a, 20, decimals_b);
        let pool_key_a =
            create_concentrated_pool(&factory, &router, pair.clone(), 500, 10, 100).unwrap();

        add_concentrated_liquidity(
            &factory,
            &router,
            pair.clone(),
            pool_key_a.clone(),
            -120,
            120,
            None,
            100,
        )
        .unwrap();

        let position_id = first_position_id(&factory);

        // Create pool B (fee 3000, spacing 60)
        let pair_b = scaled_pair(&token_a, &token_b, 20, decimals_a, 20, decimals_b);
        let pool_key_b =
            create_concentrated_pool(&factory, &router, pair_b.clone(), 3_000, 60, 100).unwrap();

        // Try to add liquidity to pool B using pool A's position
        let add_pair = scaled_pair(&token_a, &token_b, 5, decimals_a, 5, decimals_b);
        let err = add_concentrated_liquidity(
            &factory,
            &router,
            add_pair,
            pool_key_b.clone(),
            -120,
            120,
            Some(position_id),
            100,
        );
        assert!(
            err.is_err(),
            "position from pool A should not work in pool B"
        );

        // Try to remove from pool B using pool A's position
        let err = remove_concentrated_liquidity(
            &factory,
            &router,
            pool_key_b,
            position_id,
            Uint128::new(1_000),
        );
        assert!(
            err.is_err(),
            "remove with pool A position in pool B should fail"
        );
    }

    #[apply(clp_matrix)]
    fn test_nonexistent_position_id_rejected(mode: FactorySetupMode, decimal_pair: (u32, u32)) {
        let (decimals_a, decimals_b) = decimal_pair;
        let (_interchain, factory, router, token_a, token_b) =
            setup_clp(mode, decimals_a, decimals_b);

        let pair = scaled_pair(&token_a, &token_b, 10, decimals_a, 10, decimals_b);
        let pool_key =
            create_concentrated_pool(&factory, &router, pair.clone(), 500, 10, 100).unwrap();

        add_concentrated_liquidity(
            &factory,
            &router,
            pair.clone(),
            pool_key.clone(),
            -120,
            120,
            None,
            100,
        )
        .unwrap();

        let bogus_id = Uint128::new(999_999_999);

        // Add with nonexistent position
        let add_pair = scaled_pair(&token_a, &token_b, 5, decimals_a, 5, decimals_b);
        let err = add_concentrated_liquidity(
            &factory,
            &router,
            add_pair,
            pool_key.clone(),
            -120,
            120,
            Some(bogus_id),
            100,
        );
        assert!(err.is_err(), "add with nonexistent position should fail");

        // Remove with nonexistent position
        let err = remove_concentrated_liquidity(
            &factory,
            &router,
            pool_key.clone(),
            bogus_id,
            Uint128::new(1_000),
        );
        assert!(err.is_err(), "remove with nonexistent position should fail");

        // Collect fees with nonexistent position
        let chain_uid = factory.get_state().unwrap().chain_uid;
        let err = factory
            .execute(
                &euclid::msgs::factory::ExecuteMsg::CollectConcentratedFees {
                    pool_key,
                    position_id: bogus_id,
                    recipient: CrossChainUser::new(
                        chain_uid,
                        factory.environment().sender.to_string(),
                    ),
                    cross_chain_config: CrossChainConfig::default(),
                },
                &[],
            )
            .unwrap_err();
        assert!(
            !err.to_string().is_empty(),
            "collect fees with nonexistent position should fail"
        );
    }
}
