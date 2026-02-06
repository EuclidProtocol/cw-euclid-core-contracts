#![cfg(not(target_arch = "wasm32"))]
use crate::helpers::chains::get_escrow;
use crate::helpers::factory::faucet;
use cosmwasm_std::Uint128;
use cw_orch::mock::MockBase;
use cw_orch::prelude::*;
use cw_orch_interchain::mock::MockInterchainEnv;
use cw_orch_interchain::prelude::InterchainEnv;
use euclid::msgs::cross_chain_config::CrossChainConfig;
use euclid::msgs::escrow::QueryMsgFns as EscrowQueryMsgFns;
use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
use euclid::msgs::virtual_balance::QueryMsgFns as VirtualBalanceQueryMsgFns;
use euclid::recipient::Recipient;
use euclid::token::TokenWithDenom;
use euclid::utils::pagination::Pagination;
use factory::FactoryContract;
use router::RouterContract;

use crate::helpers::relayer::relay_factory_router_factory;

pub fn deposit_token(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    token: TokenWithDenom,
    amount: Uint128,
    recipients: Vec<Recipient>,
) -> Result<(), CwOrchError> {
    let factory_chain_uid = &factory.get_state().unwrap().chain_uid;
    let mut funds = vec![];
    faucet(
        factory.environment(),
        factory.environment().sender.as_str(),
        amount.u128(),
        token.token_type.clone(),
        &mut funds,
    );
    let tx_response = factory.execute(
        &euclid::msgs::factory::msg::ExecuteMsg::DepositToken {
            asset_in: token.clone(),
            amount_in: amount,
            recipients,
            cross_chain_config: CrossChainConfig::default(),
        },
        &funds,
    )?;
    relay_factory_router_factory(tx_response.events, factory, router, factory_chain_uid)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helpers::chains::{get_virtual_balance, setup_router};
    use crate::tests_reusable::factory_register::setup_factory;
    use crate::tests_reusable::factory_register_denom::register_denom;
    use euclid::cross_chain_user::CrossChainUser;
    use euclid::limit::Limit;
    use euclid::token::{Token, TokenType};
    use euclid::voucher::BalanceKey;
    use rstest::rstest;

    #[rstest]
    #[case("empty")]
    #[case("single_voucher")]
    #[case("two_voucher")]
    fn deposit_token_updates_router_and_escrow_balances(#[case] recipient_case: &str) {
        let sender = "sender_for_all_chains";
        let factory_chain_id = "nibiru";
        let router_chain_id = "nibiru";
        let interchain = MockInterchainEnv::new(vec![(router_chain_id, sender)]);
        let router_chain = interchain.get_chain(router_chain_id).unwrap();
        let router = setup_router(&router_chain).unwrap();
        let factory =
            setup_factory(&interchain, factory_chain_id, router_chain_id, &router).unwrap();

        let token = TokenWithDenom {
            token: Token::create("eucl".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "eucl".to_string(),
            },
        };
        register_denom(&factory, &router, token.clone()).unwrap();

        let factory_chain_uid = factory.get_state().unwrap().chain_uid;
        let escrow_contract = get_escrow(&factory, token.token.as_str());
        let old_escrow_balance = escrow_contract.state().unwrap();
        let old_router_escrow_balance = router
            .query_token_escrows(
                Pagination::new(Some(factory_chain_uid.clone()), None, None, Some(1)),
                token.token.clone(),
            )
            .unwrap();
        let old_balance = match old_router_escrow_balance.chains.first() {
            Some(chain) => chain.balance,
            None => Uint128::zero(),
        };

        let amount = Uint128::from(10_000u128);
        let recipient_one = CrossChainUser::new(
            factory_chain_uid.clone(),
            factory.environment().addr_make("recipient_one").to_string(),
        );
        let recipient_two = CrossChainUser::new(
            factory_chain_uid.clone(),
            factory.environment().addr_make("recipient_two").to_string(),
        );
        let (recipients, expected_balances) = match recipient_case {
            "empty" => (vec![], vec![]),
            "single_voucher" => (
                vec![Recipient::default_voucher_recipient(
                    recipient_one.clone(),
                    Limit::Dynamic(Uint128::zero()),
                )],
                vec![(recipient_one.clone(), amount)],
            ),
            "two_voucher" => (
                vec![
                    Recipient::default_voucher_recipient(
                        recipient_one.clone(),
                        Limit::Dynamic(Uint128::zero()),
                    ),
                    Recipient::default_voucher_recipient(
                        recipient_two.clone(),
                        Limit::Dynamic(Uint128::zero()),
                    ),
                ],
                vec![
                    (recipient_one.clone(), amount),
                    (recipient_two.clone(), Uint128::zero()),
                ],
            ),
            _ => unreachable!("unexpected recipient case"),
        };
        deposit_token(&factory, &router, token.clone(), amount, recipients).unwrap();

        let new_router_escrow_balance = router
            .query_token_escrows(
                Pagination::new(Some(factory_chain_uid.clone()), None, None, Some(1)),
                token.token.clone(),
            )
            .unwrap();
        let new_balance = match new_router_escrow_balance.chains.first() {
            Some(chain) => chain.balance,
            None => Uint128::zero(),
        };
        assert_eq!(
            new_balance,
            old_balance + amount,
            "Router escrow balance not updated properly"
        );
        let new_escrow_balance = escrow_contract.state().unwrap();
        assert_eq!(
            new_escrow_balance.total_amount,
            old_escrow_balance.total_amount + amount,
            "Escrow balance not updated properly"
        );

        let virtual_balance_contract = get_virtual_balance(
            router.environment(),
            &router.get_state().unwrap().virtual_balance_address,
        );
        for (recipient, expected_amount) in expected_balances {
            let balance = virtual_balance_contract
                .get_balance(BalanceKey {
                    cross_chain_user: recipient.clone(),
                    token_id: token.token.to_string(),
                })
                .unwrap();
            assert_eq!(
                balance.amount, expected_amount,
                "Recipient balance mismatch"
            );
        }
    }
}
