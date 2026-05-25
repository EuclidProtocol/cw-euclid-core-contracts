#![cfg(not(target_arch = "wasm32"))]
use crate::helpers::chains::get_escrow;
use crate::helpers::factory::faucet;
use cosmwasm_std::{Event, Uint128, Uint256};
use cw_orch::mock::MockBase;
use cw_orch::prelude::*;
use cw_orch_interchain::prelude::InterchainEnv;
use euclid::msgs::cross_chain_config::CrossChainConfig;
use euclid::msgs::escrow::QueryMsgFns as EscrowQueryMsgFns;
use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
use euclid::msgs::factory::ExecuteMsgFns;
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
use euclid::recipient::Recipient;
use euclid::token::{PairWithDenomAndAmount, TokenWithDenom};
use factory::FactoryContract;
use router::RouterContract;

use crate::helpers::relayer::relay_factory_router_factory;

pub fn deposit_token(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    token: TokenWithDenom,
    amount: Uint256,
    recipients: Vec<Recipient>,
) -> Result<(), CwOrchError> {
    let factory_chain_uid = &factory.get_state().unwrap().chain_uid;
    let mut funds = vec![];
    faucet(
        factory.environment(),
        factory.environment().sender.as_str(),
        Uint128::try_from(amount).unwrap().u128(),
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

pub fn add_liquidity(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    pair_with_denom: PairWithDenomAndAmount,
    slippage_tolerance_bps: u64,
) -> Result<Vec<Event>, CwOrchError> {
    let chain = factory.environment();
    let mut funds = vec![];
    for token in pair_with_denom.get_vec_token_info() {
        faucet(
            &chain,
            chain.sender.as_str(),
            Uint128::try_from(token.amount).unwrap().u128(),
            token.token_type.clone(),
            &mut funds,
        );
    }
    println!("Execute Add Liquidity {:?}", pair_with_denom);
    let tx_response = factory.execute(
        &euclid::msgs::factory::ExecuteMsg::AddLiquidity {
            pair_with_denom_and_amount: pair_with_denom,
            slippage_tolerance_bps,
            cross_chain_config: CrossChainConfig::default(),
        },
        &funds,
    )?;
    let factory_chain_uid = &factory.get_state().unwrap().chain_uid;
    println!("Relay Add Liquidity {:?}", tx_response.events);
    let events =
        relay_factory_router_factory(tx_response.events, factory, router, factory_chain_uid)?;
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helpers::chains::{get_virtual_balance, setup_interchain, setup_router};
    use crate::helpers::relayer::extract_ack_packet_events;
    use crate::tests_reusable::constants::ROUTER_CHAIN_ID;
    use crate::tests_reusable::factory_create_pool::create_pool;
    use crate::tests_reusable::factory_register::{setup_factory, FactorySetupMode};
    use crate::tests_reusable::factory_register_denom::register_denom;
    use crate::tests_reusable::state_sync::sync_state;
    use crate::tests_reusable::test_macros::{decimal_pair, single_decimal};
    use euclid::cross_chain_user::CrossChainUser;
    use euclid::limit::Limit;
    use euclid::msgs::virtual_balance::msg::QueryMsgFns as VirtualBalanceQueryMsgFns;
    use euclid::msgs::vlp::base::PoolConfig;
    use euclid::token::{Token, TokenType, TokenWithDenomAndAmount};
    use rstest::rstest;
    use rstest_reuse::apply;

    #[cfg_attr(not(feature = "full_decimals"), apply(single_decimal))]
    #[cfg_attr(feature = "full_decimals", apply(single_decimal_full))]
    #[case("empty")]
    #[case("single_voucher")]
    #[case("two_voucher")]
    fn deposit_token_updates_router_and_escrow_balances(
        #[case] recipient_case: &str,
        #[values(FactorySetupMode::Native, FactorySetupMode::Ibc, FactorySetupMode::Evm)]
        mode: FactorySetupMode,
        decimals: u32,
    ) {
        use crate::helpers::chains::setup_interchain;
        use crate::tests_reusable::constants::ROUTER_CHAIN_ID;

        let factory_chain_id = mode.chain_id();
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, factory_chain_id);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![factory_chain_id]).unwrap();
        let factory = setup_factory(&interchain, factory_chain_id, &router).unwrap();

        let token = TokenWithDenom {
            token: Token::create("eucl".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "eucl".to_string(),
                decimals: Some(decimals),
            },
        };
        register_denom(&factory, &router, token.clone()).unwrap();

        let factory_chain_uid = factory.get_state().unwrap().chain_uid;
        let escrow_contract = get_escrow(&factory, token.token.as_str());
        let old_escrow_balance = escrow_contract.state().unwrap();
        let virtual_balance_address = router.get_state().unwrap().virtual_balance_address;
        let virtual_balance_contract =
            get_virtual_balance(router.environment(), &virtual_balance_address);
        let old_router_escrow_balance = virtual_balance_contract
            .get_token_escrows(token.token.to_string(), None)
            .unwrap();
        let old_balance = old_router_escrow_balance
            .escrows
            .iter()
            .find(|c| c.chain_uid == factory_chain_uid)
            .map(|c| c.balance)
            .unwrap_or(Uint256::zero());

        let decimal_multiplier = Uint256::from(10u128).pow(decimals);
        let amount = Uint256::from(10_000u128)
            .checked_mul(decimal_multiplier)
            .unwrap();
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
                    Limit::Dynamic(Uint256::zero()),
                )],
                vec![(recipient_one.clone(), amount)],
            ),
            "two_voucher" => (
                vec![
                    Recipient::default_voucher_recipient(
                        recipient_one.clone(),
                        Limit::Dynamic(Uint256::zero()),
                    ),
                    Recipient::default_voucher_recipient(
                        recipient_two.clone(),
                        Limit::Dynamic(Uint256::zero()),
                    ),
                ],
                vec![
                    (recipient_one.clone(), amount),
                    (recipient_two.clone(), Uint256::zero()),
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
            old_balance + Uint256::from(amount),
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
            assert_eq!(
                balance,
                Uint256::from(expected_amount),
                "Recipient balance mismatch"
            );
        }
    }

    #[cfg_attr(not(feature = "full_decimals"), apply(decimal_pair))]
    #[cfg_attr(feature = "full_decimals", apply(decimal_pair_full))]
    fn add_liquidity_fails_when_slippage_exceeded(
        #[values(FactorySetupMode::Native, FactorySetupMode::Ibc, FactorySetupMode::Evm)]
        mode: FactorySetupMode,
        decimals_a: u32,
        decimals_b: u32,
    ) {
        let factory_chain_id = mode.chain_id();
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, factory_chain_id);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![factory_chain_id]).unwrap();
        let factory = setup_factory(&interchain, factory_chain_id, &router).unwrap();

        let token_a = TokenWithDenom {
            token: Token::create("tokena".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "tokena".to_string(),
                decimals: Some(decimals_a),
            },
        };
        let token_b = TokenWithDenom {
            token: Token::create("tokenb".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "tokenb".to_string(),
                decimals: Some(decimals_b),
            },
        };

        register_denom(&factory, &router, token_a.clone()).unwrap();
        register_denom(&factory, &router, token_b.clone()).unwrap();

        let decimal_a_multiplier = Uint256::from(10u128).pow(decimals_a);
        let decimal_b_multiplier = Uint256::from(10u128).pow(decimals_b);

        let pool_pair = PairWithDenomAndAmount {
            token_1: TokenWithDenomAndAmount {
                token: token_a.token.clone(),
                token_type: token_a.token_type.clone(),
                amount: Uint256::from(10_000u128)
                    .checked_mul(decimal_a_multiplier)
                    .unwrap(),
            },
            token_2: TokenWithDenomAndAmount {
                token: token_b.token.clone(),
                token_type: token_b.token_type.clone(),
                amount: Uint256::from(10_000u128)
                    .checked_mul(decimal_b_multiplier)
                    .unwrap(),
            },
        };
        create_pool(
            &factory,
            &router,
            pool_pair,
            500,
            PoolConfig::ConstantProduct {},
        )
        .unwrap();

        let skewed_pair = PairWithDenomAndAmount {
            token_1: TokenWithDenomAndAmount {
                token: token_a.token.clone(),
                token_type: token_a.token_type.clone(),
                amount: Uint256::from(1_000u128)
                    .checked_mul(decimal_a_multiplier)
                    .unwrap(),
            },
            token_2: TokenWithDenomAndAmount {
                token: token_b.token.clone(),
                token_type: token_b.token_type.clone(),
                amount: Uint256::from(5_000u128)
                    .checked_mul(decimal_b_multiplier)
                    .unwrap(),
            },
        };

        let result = add_liquidity(&factory, &router, skewed_pair, 100);
        if let Err(err) = result {
            let root_err = err.root().to_string();
            assert!(
                root_err.contains("Slippage has been exceeded when providing liquidity"),
                "Error should mention slippage exceeded, got: {root_err}"
            );
        } else {
            let events = result.unwrap();
            let ack_events = extract_ack_packet_events(&events);
            let ack_events = ack_events.first().unwrap();
            let ack_string = String::from_utf8(ack_events.ack.to_vec()).unwrap();

            println!("\n\nAck JSON: {:?}\n\n", ack_string);

            assert!(
                ack_string.contains("Slippage has been exceeded when providing liquidity"),
                "Ack should contain error"
            );
        }
    }
}
