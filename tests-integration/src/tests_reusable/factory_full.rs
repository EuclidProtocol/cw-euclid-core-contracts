#![cfg(not(target_arch = "wasm32"))]
use crate::helpers::chains::setup_router;
use crate::tests_reusable::factory_add_liquidity::deposit_token;
use crate::tests_reusable::factory_register::{setup_factory, setup_factory_evm, FactorySetupMode};
use crate::tests_reusable::factory_register_denom::register_denom;
use cosmwasm_std::Uint128;
use cw_orch::mock::MockBase;
use cw_orch_interchain::mock::MockInterchainEnv;
use cw_orch_interchain::prelude::InterchainEnv;
use euclid::recipient::Recipient;
use euclid::token::{Token, TokenType, TokenWithDenom};
use factory::FactoryContract;
use router::RouterContract;
use rstest::rstest;

pub(crate) fn setup_factory_full_flow(
    sender: &str,
    token: TokenWithDenom,
    amount: Uint128,
    recipients: Vec<Recipient>,
    mode: FactorySetupMode,
    factory_chain_id: &str,
    router_chain_id: &str,
) -> (FactoryContract<MockBase>, RouterContract<MockBase>) {
    let mut chains = vec![(router_chain_id, sender)];
    if router_chain_id != factory_chain_id {
        chains.push((factory_chain_id, sender));
    }
    let interchain = MockInterchainEnv::new(chains);
    let router_chain = interchain.get_chain(router_chain_id).unwrap();
    let router = setup_router(&router_chain).unwrap();
    let factory = match mode {
        FactorySetupMode::Native | FactorySetupMode::Ibc => {
            setup_factory(&interchain, factory_chain_id, router_chain_id, &router).unwrap()
        }
        FactorySetupMode::Evm => {
            setup_factory_evm(&interchain, factory_chain_id, router_chain_id, &router).unwrap()
        }
    };

    register_denom(&factory, &router, token.clone()).unwrap();

    deposit_token(&factory, &router, token.clone(), amount, recipients).unwrap();
    (factory, router)
}

#[cfg(test)]
mod tests {

    use cw_orch::prelude::Environment;
    use euclid::chain::ChainUid;
    use euclid::cross_chain_user::CrossChainUser;
    use euclid::limit::Limit;
    use euclid::msgs::escrow::QueryMsgFns as EscrowQueryMsgFns;
    use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
    use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
    use euclid::msgs::virtual_balance::QueryMsgFns as VirtualBalanceQueryMsgFns;
    use euclid::utils::pagination::Pagination;
    use euclid::voucher::BalanceKey;

    use crate::helpers::chains::{get_escrow, get_virtual_balance};

    use super::*;
    #[rstest]
    #[case(FactorySetupMode::Native, "nibiru", "nibiru", "empty")]
    #[case(FactorySetupMode::Native, "nibiru", "nibiru", "single_voucher")]
    #[case(FactorySetupMode::Native, "nibiru", "nibiru", "two_voucher")]
    #[case(FactorySetupMode::Ibc, "nibiru", "osmosis", "empty")]
    #[case(FactorySetupMode::Ibc, "nibiru", "osmosis", "single_voucher")]
    #[case(FactorySetupMode::Ibc, "nibiru", "osmosis", "two_voucher")]
    #[case(FactorySetupMode::Evm, "evm1", "osmosis", "empty")]
    #[case(FactorySetupMode::Evm, "evm1", "osmosis", "single_voucher")]
    #[case(FactorySetupMode::Evm, "evm1", "osmosis", "two_voucher")]

    fn factory_full_flow_register_denom_and_deposit(
        #[case] mode: FactorySetupMode,
        #[case] factory_chain_id: &str,
        #[case] router_chain_id: &str,
        #[case] recipient_case: &str,
    ) {
        let sender = "sender_for_all_chains";
        let token = TokenWithDenom {
            token: Token::create("eucl".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "eucl".to_string(),
            },
        };
        let amount = Uint128::from(10_000u128);
        let chain_uid = ChainUid::create(factory_chain_id.to_string()).unwrap();
        let recipients = match recipient_case {
            "empty" => vec![],
            "single_voucher" => vec![Recipient::default_voucher_recipient(
                CrossChainUser::new(chain_uid.clone(), "recipient_one".to_string()),
                Limit::Dynamic(Uint128::zero()),
            )],
            "two_voucher" => vec![
                Recipient::default_voucher_recipient(
                    CrossChainUser::new(chain_uid.clone(), "recipient_one".to_string()),
                    Limit::Dynamic(Uint128::zero()),
                ),
                Recipient::default_voucher_recipient(
                    CrossChainUser::new(chain_uid.clone(), "recipient_two".to_string()),
                    Limit::Dynamic(Uint128::zero()),
                ),
            ],
            _ => unreachable!("unexpected recipient case"),
        };
        let (factory, router) = setup_factory_full_flow(
            sender,
            token.clone(),
            amount,
            recipients,
            mode,
            factory_chain_id,
            router_chain_id,
        );

        // Assert factory chain is registered on the router
        let all_chains = router.get_all_chains().unwrap();
        assert!(
            all_chains.chains.iter().any(|c| c.chain_uid == chain_uid),
            "Factory chain should be registered on router"
        );

        // Assert escrow exists and has the denom registered
        let escrow_response = factory.get_escrow(token.token.to_string()).unwrap();
        assert!(
            escrow_response.escrow_address.is_some(),
            "Escrow address should exist after registering denom"
        );
        assert!(
            escrow_response
                .denoms
                .iter()
                .any(|d| d == &token.token_type),
            "Token denom should be registered in escrow"
        );

        // Assert escrow balance equals the deposited amount
        let escrow_contract = get_escrow(&factory, token.token.as_str());
        let escrow_state = escrow_contract.state().unwrap();
        assert_eq!(
            escrow_state.total_amount, amount,
            "Escrow total amount should equal deposited amount"
        );

        // Assert router tracks correct escrow balance for this chain
        let token_escrows = router
            .query_token_escrows(
                Pagination::new(Some(chain_uid.clone()), None, None, Some(1)),
                token.token.clone(),
            )
            .unwrap();
        let chain_escrow = token_escrows
            .chains
            .iter()
            .find(|c| c.chain_uid == chain_uid)
            .expect("Factory chain should have escrow balance on router");
        assert_eq!(
            chain_escrow.balance, amount,
            "Router escrow balance should match deposited amount"
        );

        // Assert token is registered on the router
        let all_tokens = router
            .query_all_tokens(Pagination::new(None, None, None, None))
            .unwrap();
        assert!(
            all_tokens.tokens.iter().any(|t| t == &token.token),
            "Token should be registered on router"
        );

        // Assert token denom is registered on router for this chain
        let token_denoms = router.query_token_denoms(token.token.clone()).unwrap();
        assert!(
            token_denoms
                .denoms
                .iter()
                .any(|d| d.chain_uid == chain_uid && d.token_type == token.token_type),
            "Token denom should be registered on router for factory chain"
        );

        // Assert virtual balances for recipients
        let virtual_balance_contract = get_virtual_balance(
            router.environment(),
            &router.get_state().unwrap().virtual_balance_address,
        );
        match recipient_case {
            "empty" => {
                // When no recipients specified, sender receives the virtual balance
                let sender_user = CrossChainUser::new(
                    chain_uid.clone(),
                    factory.environment().sender.to_string(),
                );
                let sender_balance = virtual_balance_contract
                    .get_balance(BalanceKey {
                        cross_chain_user: sender_user,
                        token_id: token.token.to_string(),
                    })
                    .unwrap();
                assert_eq!(
                    sender_balance.amount, amount,
                    "Sender should receive full virtual balance when no recipients specified"
                );
            }
            "single_voucher" => {
                let recipient_one =
                    CrossChainUser::new(chain_uid.clone(), "recipient_one".to_string());
                let balance = virtual_balance_contract
                    .get_balance(BalanceKey {
                        cross_chain_user: recipient_one,
                        token_id: token.token.to_string(),
                    })
                    .unwrap();
                assert_eq!(
                    balance.amount, amount,
                    "Single recipient should receive entire virtual balance"
                );
            }
            "two_voucher" => {
                let recipient_one =
                    CrossChainUser::new(chain_uid.clone(), "recipient_one".to_string());
                let recipient_two =
                    CrossChainUser::new(chain_uid.clone(), "recipient_two".to_string());
                let balance_one = virtual_balance_contract
                    .get_balance(BalanceKey {
                        cross_chain_user: recipient_one,
                        token_id: token.token.to_string(),
                    })
                    .unwrap();
                let balance_two = virtual_balance_contract
                    .get_balance(BalanceKey {
                        cross_chain_user: recipient_two,
                        token_id: token.token.to_string(),
                    })
                    .unwrap();
                // Total distributed across recipients should equal the deposited amount
                assert_eq!(
                    balance_one.amount + balance_two.amount,
                    amount,
                    "Total virtual balance across recipients should equal deposited amount"
                );
            }
            _ => unreachable!("unexpected recipient case"),
        }
    }
}
