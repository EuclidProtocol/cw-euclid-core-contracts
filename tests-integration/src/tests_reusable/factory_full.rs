#![cfg(not(target_arch = "wasm32"))]
use crate::helpers::chains::setup_router;
use crate::tests_reusable::factory_add_liquidity::deposit_token;
use crate::tests_reusable::factory_create_pool::create_pool;
use crate::tests_reusable::factory_register::{setup_factory, setup_factory_evm, FactorySetupMode};
use crate::tests_reusable::factory_register_denom::register_denom;
use crate::tests_reusable::factory_swap::swap_request;
use cosmwasm_std::Uint128;
use cw_orch::mock::MockBase;
use cw_orch_interchain::mock::MockInterchainEnv;
use cw_orch_interchain::prelude::InterchainEnv;
use euclid::fee::PartnerFee;
use euclid::msgs::vlp::base::PoolConfig;
use euclid::recipient::Recipient;
use euclid::swap::NextSwapPair;
use euclid::token::{
    PairWithDenomAndAmount, Token, TokenType, TokenWithDenom, TokenWithDenomAndAmount,
};
use factory::FactoryContract;
use router::RouterContract;
use rstest::rstest;

pub(crate) fn setup_factory_full_flow(
    sender: &str,
    token_1: TokenWithDenom,
    token_2: TokenWithDenom,
    amount_1: Uint128,
    amount_2: Uint128,
    recipients: Vec<Recipient>,
    mode: FactorySetupMode,
    pool_type: PoolConfig,
    factory_chain_id: &str,
    router_chain_id: &str,
) -> (
    MockInterchainEnv,
    FactoryContract<MockBase>,
    RouterContract<MockBase>,
) {
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

    register_denom(&factory, &router, token_1.clone()).unwrap();
    register_denom(&factory, &router, token_2.clone()).unwrap();

    deposit_token(
        &factory,
        &router,
        token_1.clone(),
        amount_1,
        recipients.clone(),
    )
    .unwrap();
    deposit_token(&factory, &router, token_2.clone(), amount_2, recipients).unwrap();
    let pair_with_denom_and_amount = PairWithDenomAndAmount {
        token_1: TokenWithDenomAndAmount {
            token: token_1.token.clone(),
            amount: amount_1,
            token_type: token_1.token_type.clone(),
        },
        token_2: TokenWithDenomAndAmount {
            token: token_2.token.clone(),
            amount: amount_2,
            token_type: token_2.token_type.clone(),
        },
    };
    let slippage_tolerance_bps = 100;
    create_pool(
        &factory,
        &router,
        pair_with_denom_and_amount,
        slippage_tolerance_bps,
        pool_type,
    )
    .unwrap();

    (interchain, factory, router)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helpers::chains::{get_escrow, get_virtual_balance};
    use cosmwasm_std::Uint64;
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

    #[rstest]
    #[case(FactorySetupMode::Native, "nibiru", "nibiru", "empty", PoolConfig::ConstantProduct {})]
    #[case(FactorySetupMode::Native, "nibiru", "nibiru", "single_voucher", PoolConfig::ConstantProduct {})]
    #[case(FactorySetupMode::Native, "nibiru", "nibiru", "two_voucher", PoolConfig::ConstantProduct {})]
    #[case(FactorySetupMode::Ibc, "nibiru", "osmosis", "empty", PoolConfig::ConstantProduct {})]
    #[case(FactorySetupMode::Ibc, "nibiru", "osmosis", "single_voucher", PoolConfig::ConstantProduct {})]
    #[case(FactorySetupMode::Ibc, "nibiru", "osmosis", "two_voucher", PoolConfig::ConstantProduct {})]
    #[case(FactorySetupMode::Evm, "evm1", "osmosis", "empty", PoolConfig::ConstantProduct {})]
    #[case(FactorySetupMode::Evm, "evm1", "osmosis", "single_voucher", PoolConfig::ConstantProduct {})]
    #[case(FactorySetupMode::Evm, "evm1", "osmosis", "two_voucher", PoolConfig::ConstantProduct {})]
    #[case(FactorySetupMode::Native, "nibiru", "nibiru", "empty", PoolConfig::Stable { amp_factor: Some(Uint64::new(100)) })]
    #[case(FactorySetupMode::Ibc, "nibiru", "osmosis", "single_voucher", PoolConfig::Stable { amp_factor: Some(Uint64::new(100)) })]
    #[case(FactorySetupMode::Evm, "evm1", "osmosis", "two_voucher", PoolConfig::Stable { amp_factor: Some(Uint64::new(100)) })]
    fn factory_full_flow_register_denom_and_deposit(
        #[case] mode: FactorySetupMode,
        #[case] factory_chain_id: &str,
        #[case] router_chain_id: &str,
        #[case] recipient_case: &str,
        #[case] pool_type: PoolConfig,
    ) {
        let sender = "sender_for_all_chains";
        let token_1 = TokenWithDenom {
            token: Token::create("eucl".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "eucl".to_string(),
            },
        };
        let amount_1 = Uint128::from(10_000u128);
        let token_2 = TokenWithDenom {
            token: Token::create("andr".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "andr".to_string(),
            },
        };
        let amount_2 = Uint128::from(10_000u128);
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
        let (interchain, factory, router) = setup_factory_full_flow(
            sender,
            token_1.clone(),
            token_2.clone(),
            amount_1,
            amount_2,
            recipients,
            mode,
            pool_type,
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
        let escrow_response = factory.get_escrow(token_1.token.to_string()).unwrap();
        assert!(
            escrow_response.escrow_address.is_some(),
            "Escrow address should exist after registering denom"
        );
        assert!(
            escrow_response
                .denoms
                .iter()
                .any(|d| d == &token_1.token_type),
            "Token denom should be registered in escrow"
        );

        // Assert escrow balance equals the deposited amount
        let escrow_contract = get_escrow(&factory, token_1.token.as_str());
        let escrow_state = escrow_contract.state().unwrap();
        assert_eq!(
            escrow_state.total_amount,
            amount_1 + amount_2,
            "Escrow total amount should equal deposited amount"
        );

        // Assert router tracks correct escrow balance for this chain
        let token_escrows = router
            .query_token_escrows(
                Pagination::new(Some(chain_uid.clone()), None, None, Some(1)),
                token_1.token.clone(),
            )
            .unwrap();
        let chain_escrow = token_escrows
            .chains
            .iter()
            .find(|c| c.chain_uid == chain_uid)
            .expect("Factory chain should have escrow balance on router");
        assert_eq!(
            chain_escrow.balance,
            amount_1 + amount_2,
            "Router escrow balance should match deposited amount"
        );

        // Assert token is registered on the router
        let all_tokens = router
            .query_all_tokens(Pagination::new(None, None, None, None))
            .unwrap();
        assert!(
            all_tokens.tokens.iter().any(|t| t == &token_1.token),
            "Token should be registered on router"
        );

        // Assert token denom is registered on router for this chain
        let token_denoms = router.query_token_denoms(token_1.token.clone()).unwrap();
        assert!(
            token_denoms
                .denoms
                .iter()
                .any(|d| d.chain_uid == chain_uid && d.token_type == token_1.token_type),
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
                        token_id: token_1.token.to_string(),
                    })
                    .unwrap();
                assert_eq!(
                    sender_balance.amount, amount_1,
                    "Sender should receive full virtual balance when no recipients specified"
                );
            }
            "single_voucher" => {
                let recipient_one =
                    CrossChainUser::new(chain_uid.clone(), "recipient_one".to_string());
                let balance = virtual_balance_contract
                    .get_balance(BalanceKey {
                        cross_chain_user: recipient_one,
                        token_id: token_1.token.to_string(),
                    })
                    .unwrap();
                assert_eq!(
                    balance.amount, amount_1,
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
                        cross_chain_user: recipient_one.clone(),
                        token_id: token_1.token.to_string(),
                    })
                    .unwrap();
                let balance_two = virtual_balance_contract
                    .get_balance(BalanceKey {
                        cross_chain_user: recipient_two.clone(),
                        token_id: token_1.token.to_string(),
                    })
                    .unwrap();

                let balance_three = virtual_balance_contract
                    .get_balance(BalanceKey {
                        cross_chain_user: recipient_one,
                        token_id: token_2.token.to_string(),
                    })
                    .unwrap();
                let balance_four = virtual_balance_contract
                    .get_balance(BalanceKey {
                        cross_chain_user: recipient_two,
                        token_id: token_2.token.to_string(),
                    })
                    .unwrap();
                // Total distributed across recipients should equal the deposited amount
                assert_eq!(
                    balance_one.amount
                        + balance_two.amount
                        + balance_three.amount
                        + balance_four.amount,
                    amount_1 + amount_2,
                    "Total virtual balance across recipients should equal deposited amount"
                );
            }
            _ => unreachable!("unexpected recipient case"),
        }

        // --- Swap with partner fee ---

        let swap_amount = Uint128::new(1_000);
        let partner_fee_bps: u64 = 30;
        let sender_addr = factory.environment().sender.to_string();

        // Partner fee: checked_mul_ceil(1000, 0.003) = ceil(3.0) = 3
        let partner_fee_amount = swap_amount
            .checked_mul_ceil(cosmwasm_std::Decimal::bps(partner_fee_bps))
            .unwrap();
        let net_swap_amount = swap_amount - partner_fee_amount;

        // Record pre-swap state
        let escrow_in_before = get_escrow(&factory, token_1.token.as_str())
            .state()
            .unwrap()
            .total_amount;

        let router_escrow_in_before = router
            .query_token_escrows(
                Pagination::new(Some(chain_uid.clone()), None, None, Some(1)),
                token_1.token.clone(),
            )
            .unwrap()
            .chains
            .first()
            .map(|c| c.balance)
            .unwrap_or(Uint128::zero());

        let sender_user = CrossChainUser::new(chain_uid.clone(), sender_addr.clone());
        let vb_out_before = virtual_balance_contract
            .get_balance(BalanceKey {
                cross_chain_user: sender_user.clone(),
                token_id: token_2.token.to_string(),
            })
            .unwrap()
            .amount;

        let partner_native_balance_before = factory
            .environment()
            .query_balance(
                &cosmwasm_std::Addr::unchecked(sender_addr.clone()),
                token_1.token.to_string().as_str(),
            )
            .unwrap();

        // Execute the swap
        let mut swap_funds = vec![];
        crate::helpers::factory::faucet(
            factory.environment(),
            factory.environment().sender.as_str(),
            swap_amount.u128(),
            token_1.token_type.clone(),
            &mut swap_funds,
        );
        swap_request(
            &interchain,
            &factory,
            &router,
            token_1.clone(),
            token_2.clone().token,
            Uint128::new(1),
            vec![NextSwapPair {
                token_in: token_1.token.clone(),
                token_out: token_2.token.clone(),
                test_fail: None,
            }],
            vec![],
            Some(PartnerFee {
                partner_fee_bps,
                recipient: sender_addr.clone(),
            }),
            swap_funds,
        )
        .unwrap();

        // --- Post-swap assertions ---

        // 1. Input token escrow increased by net swap amount (after partner fee deduction)
        let escrow_in_after = get_escrow(&factory, token_1.token.as_str())
            .state()
            .unwrap()
            .total_amount;
        assert_eq!(
            escrow_in_after,
            escrow_in_before + net_swap_amount,
            "Escrow for input token should increase by net swap amount (swap_amount - partner_fee)"
        );

        // 2. Router escrow balance for input token increased by net swap amount
        let router_escrow_in_after = router
            .query_token_escrows(
                Pagination::new(Some(chain_uid.clone()), None, None, Some(1)),
                token_1.token.clone(),
            )
            .unwrap()
            .chains
            .first()
            .map(|c| c.balance)
            .unwrap_or(Uint128::zero());
        assert_eq!(
            router_escrow_in_after,
            router_escrow_in_before + net_swap_amount,
            "Router escrow balance for input token should increase by net swap amount"
        );

        // 3. Sender received output tokens as virtual balance
        let vb_out_after = virtual_balance_contract
            .get_balance(BalanceKey {
                cross_chain_user: sender_user.clone(),
                token_id: token_2.token.to_string(),
            })
            .unwrap()
            .amount;
        let amount_received = vb_out_after - vb_out_before;
        assert!(
            amount_received > Uint128::zero(),
            "Sender should have received output tokens as virtual balance, got 0"
        );

        // 4. Output amount should be less than net input (AMM pricing with equal reserves)
        assert!(
            amount_received < net_swap_amount,
            "Amount received ({}) should be less than net input ({}) for equal-reserve pools",
            amount_received,
            net_swap_amount
        );

        // 5. Partner fee recipient received the fee as native tokens
        let partner_native_balance_after = factory
            .environment()
            .query_balance(
                &cosmwasm_std::Addr::unchecked(sender_addr.clone()),
                token_1.token.to_string().as_str(),
            )
            .unwrap();
        assert_eq!(
            partner_native_balance_after,
            partner_native_balance_before + partner_fee_amount,
            "Partner fee recipient should have received {} native input tokens as fee",
            partner_fee_amount
        );
    }
}
