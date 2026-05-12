use cosmwasm_std::{Uint128, Uint256};
use cw_orch::prelude::*;
use euclid::cross_chain_user::CrossChainUser;
use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
use euclid::msgs::virtual_balance::msg::QueryMsgFns as VirtualBalanceQueryMsgFns;
use euclid::token::TokenWithDenom;
use euclid::voucher::BalanceKey;

use crate::helpers::chains::get_virtual_balance;
use crate::helpers::factory::list_position_ids;
use crate::tests_reusable::concentrated_create_pool::{
    pair_with_amounts, setup_concentrated_env_with_decimals,
};
use crate::tests_reusable::factory_register::FactorySetupMode;

pub fn price_to_tick(price: f64) -> i64 {
    (price.ln() / 1.0001_f64.ln()).floor() as i64
}

pub fn pair_to_tick(amount_a: Uint128, amount_b: Uint128) -> i64 {
    price_to_tick(amount_b.u128() as f64 / amount_a.u128() as f64)
}

pub fn first_position_id(factory: &factory::FactoryContract<cw_orch::mock::MockBase>) -> Uint128 {
    let ids = list_position_ids(factory).unwrap();
    assert!(!ids.is_empty(), "expected at least one position");
    Uint128::new(ids[0].parse::<u128>().unwrap())
}

pub fn last_position_id(factory: &factory::FactoryContract<cw_orch::mock::MockBase>) -> Uint128 {
    let ids = list_position_ids(factory).unwrap();
    let max_id = ids
        .iter()
        .filter_map(|s| s.parse::<u128>().ok())
        .max()
        .expect("expected at least one position");
    Uint128::new(max_id)
}

pub fn sender(factory: &factory::FactoryContract<cw_orch::mock::MockBase>) -> CrossChainUser {
    CrossChainUser::new(
        factory.get_state().unwrap().chain_uid,
        factory.environment().sender.to_string(),
    )
}

/// Scale a "logical unit count" to raw token units for a given decimal.
/// E.g., `raw_units(5, 6)` = 5_000_000 (5 tokens at 6 decimals).
pub fn raw_units(whole_units: u128, decimals: u32) -> u128 {
    whole_units * 10u128.pow(decimals)
}

/// Setup CLP env with decimal pair and return (interchain, factory, router, token_a, token_b).
pub fn setup_clp(
    mode: FactorySetupMode,
    decimals_a: u32,
    decimals_b: u32,
) -> (
    cw_orch_interchain::mock::MockInterchainEnv,
    factory::FactoryContract<cw_orch::mock::MockBase>,
    router::RouterContract<cw_orch::mock::MockBase>,
    TokenWithDenom,
    TokenWithDenom,
) {
    setup_concentrated_env_with_decimals(mode, mode.chain_id(), decimals_a, decimals_b)
}

/// Build a pair with amounts scaled to the given decimals.
/// `units_a` and `units_b` are whole-token counts (e.g. 30 = 30 tokens).
pub fn scaled_pair(
    token_a: &TokenWithDenom,
    token_b: &TokenWithDenom,
    units_a: u128,
    decimals_a: u32,
    units_b: u128,
    decimals_b: u32,
) -> euclid::token::PairWithDenomAndAmount {
    pair_with_amounts(
        token_a,
        token_b,
        raw_units(units_a, decimals_a),
        raw_units(units_b, decimals_b),
    )
}

pub fn voucher_balance(
    factory: &factory::FactoryContract<cw_orch::mock::MockBase>,
    router: &router::RouterContract<cw_orch::mock::MockBase>,
    token_id: &str,
) -> Uint256 {
    let virtual_balance = get_virtual_balance(
        router.environment(),
        &router.get_state().unwrap().virtual_balance_address,
    );
    virtual_balance
        .get_balance(BalanceKey {
            cross_chain_user: sender(factory),
            token_id: token_id.to_string(),
        })
        .unwrap()
        .amount
}
