#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::Uint128;
use cw_orch::prelude::*;
use euclid::cross_chain_user::CrossChainUser;
use euclid::msgs::cross_chain_config::CrossChainConfig;
use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
use rstest::rstest;

use crate::helpers::factory::{
    add_concentrated_liquidity, create_concentrated_pool, list_position_ids,
    remove_concentrated_liquidity,
};
use crate::tests_reusable::concentrated_create_pool::{pair_with_amounts, setup_concentrated_env};
use crate::tests_reusable::concentrated_swap::execute_concentrated_swap;
use crate::tests_reusable::constants::{FACTORY_CHAIN_ID_IBC, FACTORY_CHAIN_ID_LOCAL};
use crate::tests_reusable::factory_register::FactorySetupMode;

fn first_position_id(factory: &factory::FactoryContract<cw_orch::mock::MockBase>) -> Uint128 {
    let ids = list_position_ids(factory).unwrap();
    assert!(!ids.is_empty(), "expected at least one position");
    Uint128::new(ids[0].parse::<u128>().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[rstest]
    #[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
    #[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
    fn test_add_to_position_owned_by_another_user_rejected(
        #[case] mode: FactorySetupMode,
        #[case] factory_chain_id: &str,
    ) {
        let (_interchain, mut factory, router, token_a, token_b) =
            setup_concentrated_env(mode, factory_chain_id);

        let pair = pair_with_amounts(&token_a, &token_b, 10_000, 10_000);
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

        let intruder_pair = pair_with_amounts(&token_a, &token_b, 5_000, 5_000);
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

    #[rstest]
    #[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
    #[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
    fn test_remove_from_position_owned_by_another_user_rejected(
        #[case] mode: FactorySetupMode,
        #[case] factory_chain_id: &str,
    ) {
        let (_interchain, mut factory, router, token_a, token_b) =
            setup_concentrated_env(mode, factory_chain_id);

        let pair = pair_with_amounts(&token_a, &token_b, 10_000, 10_000);
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

    #[rstest]
    #[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
    #[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
    fn test_collect_fees_for_position_owned_by_another_user_rejected(
        #[case] mode: FactorySetupMode,
        #[case] factory_chain_id: &str,
    ) {
        let (_interchain, mut factory, router, token_a, token_b) =
            setup_concentrated_env(mode, factory_chain_id);

        let pair = pair_with_amounts(&token_a, &token_b, 30_000, 30_000);
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
            Uint128::new(5_000),
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

    #[test]
    fn test_position_from_pool_a_cannot_be_used_in_pool_b() {
        let (_interchain, factory, router, token_a, token_b) =
            setup_concentrated_env(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL);

        // Create pool A (fee 500, spacing 10)
        let pair = pair_with_amounts(&token_a, &token_b, 20_000, 20_000);
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
        let pair_b = pair_with_amounts(&token_a, &token_b, 20_000, 20_000);
        let pool_key_b =
            create_concentrated_pool(&factory, &router, pair_b.clone(), 3_000, 60, 100).unwrap();

        // Try to add liquidity to pool B using pool A's position
        let add_pair = pair_with_amounts(&token_a, &token_b, 5_000, 5_000);
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

    #[test]
    fn test_nonexistent_position_id_rejected() {
        let (_interchain, factory, router, token_a, token_b) =
            setup_concentrated_env(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL);

        let pair = pair_with_amounts(&token_a, &token_b, 10_000, 10_000);
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
        let add_pair = pair_with_amounts(&token_a, &token_b, 5_000, 5_000);
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
