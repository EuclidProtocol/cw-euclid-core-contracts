use cosmwasm_std::Uint128;
use cw_orch::prelude::*;
use euclid::cross_chain_user::CrossChainUser;
use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
use euclid::msgs::virtual_balance::msg::QueryMsgFns as VirtualBalanceQueryMsgFns;
use euclid::voucher::BalanceKey;

use crate::helpers::chains::get_virtual_balance;
use crate::helpers::factory::list_position_ids;

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

pub fn voucher_balance(
    factory: &factory::FactoryContract<cw_orch::mock::MockBase>,
    router: &router::RouterContract<cw_orch::mock::MockBase>,
    token_id: &str,
) -> Uint128 {
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
