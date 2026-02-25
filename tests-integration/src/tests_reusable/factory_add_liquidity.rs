#![cfg(not(target_arch = "wasm32"))]
use crate::helpers::chains::get_escrow;
use crate::helpers::factory::faucet;
use cosmwasm_std::Uint128;
use cw_orch::mock::MockBase;
use cw_orch::prelude::*;
use cw_orch_interchain::prelude::InterchainEnv;
use euclid::msgs::cross_chain_config::CrossChainConfig;
use euclid::msgs::escrow::QueryMsgFns as EscrowQueryMsgFns;
use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
use euclid::msgs::factory::ExecuteMsgFns;
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
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
    let tx_response = factory
        .deposit_token(
            amount,
            token.clone(),
            CrossChainConfig::default(),
            recipients,
            &funds.to_vec(),
        )
        .unwrap();
    relay_factory_router_factory(tx_response.events, factory, router, factory_chain_uid)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helpers::chains::setup_router;
    use crate::tests_reusable::constants::{FACTORY_CHAIN_ID_IBC, FACTORY_CHAIN_ID_LOCAL};
    use crate::tests_reusable::factory_register::setup_factory;
    use crate::tests_reusable::factory_register_denom::register_denom;
    use crate::tests_reusable::state_sync::sync_state;
    use euclid::cross_chain_user::CrossChainUser;
    use euclid::limit::Limit;
    use euclid::token::{Token, TokenType};
    use rstest::rstest;

    #[rstest]
    #[case("empty", FACTORY_CHAIN_ID_LOCAL)]
    #[case("single_voucher", FACTORY_CHAIN_ID_LOCAL)]
    #[case("two_voucher", FACTORY_CHAIN_ID_LOCAL)]
    #[case("empty", FACTORY_CHAIN_ID_IBC)]
    #[case("single_voucher", FACTORY_CHAIN_ID_IBC)]
    #[case("two_voucher", FACTORY_CHAIN_ID_IBC)]
    fn deposit_token_updates_router_and_escrow_balances(
        #[case] recipient_case: &str,
        #[case] factory_chain_id: &str,
    ) {
        use crate::helpers::chains::setup_interchain;
        use crate::tests_reusable::constants::ROUTER_CHAIN_ID;

        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, factory_chain_id);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![factory_chain_id]).unwrap();
        let factory = setup_factory(&interchain, factory_chain_id, &router).unwrap();

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
        let recipients_for_sync = recipients.clone();
        deposit_token(&factory, &router, token.clone(), amount, recipients).unwrap();

        let synced_state = sync_state(
            &factory,
            &router,
            recipients_for_sync,
            vec![token.token.clone()],
            vec![],
            vec![token.token.clone()],
            factory_chain_uid.clone(),
            vec![],
        );

        let escrow_state = synced_state
            .escrow_balance(&factory_chain_uid, &token.token)
            .expect("Escrow state should exist for deposited token");
        assert_eq!(
            escrow_state.router_escrow_balance,
            old_balance + amount,
            "Router escrow balance not updated properly"
        );
        assert_eq!(
            escrow_state.factory_escrow_balance,
            old_escrow_balance.total_amount + amount,
            "Escrow balance not updated properly"
        );

        for (recipient, expected_amount) in expected_balances {
            let balance = synced_state
                .voucher_balance(&recipient, &token.token)
                .expect("Expected voucher balance for recipient is missing");
            assert_eq!(balance, expected_amount, "Recipient balance mismatch");
        }
    }
}
