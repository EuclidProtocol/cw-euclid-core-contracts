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
    use euclid::{chain::ChainUid, cross_chain_user::CrossChainUser};
    use euclid::limit::Limit;

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
        setup_factory_full_flow(
            sender,
            token,
            amount,
            recipients,
            mode,
            factory_chain_id,
            router_chain_id,
        );
    }
}
