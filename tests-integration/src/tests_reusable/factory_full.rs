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
    Pair, PairWithDenomAndAmount, Token, TokenType, TokenWithDenom, TokenWithDenomAndAmount,
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
    let router = setup_router(&router_chain, vec![factory_chain_id]).unwrap();
    let factory = match mode {
        FactorySetupMode::Native | FactorySetupMode::Ibc => {
            setup_factory(&interchain, factory_chain_id, &router).unwrap()
        }
        FactorySetupMode::Evm => setup_factory_evm(&interchain, factory_chain_id, &router).unwrap(),
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
    use crate::helpers::chains::{get_escrow, get_lp_token, get_virtual_balance};
    use crate::tests_reusable::constants::{
        FACTORY_CHAIN_ID_EVM, FACTORY_CHAIN_ID_IBC, FACTORY_CHAIN_ID_LOCAL, ROUTER_CHAIN_ID,
    };
    use crate::tests_reusable::state_sync::sync_state;
    use crate::tests_reusable::state_sync::UserFundsQuery;
    use cosmwasm_std::Uint64;
    use cw20::{Cw20Coin, MinterResponse};
    use cw_orch::prelude::Environment;
    use cw_orch::prelude::{ContractInstance as _, CwOrchInstantiate, CwOrchUpload};
    use euclid::chain::ChainUid;
    use euclid::cross_chain_user::CrossChainUser;
    use euclid::limit::Limit;
    use euclid::msgs::escrow::QueryMsgFns as EscrowQueryMsgFns;
    use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
    use euclid::msgs::lp_token::msg::InstantiateMsg as LpTokenInstantiateMsg;
    use euclid::msgs::lp_token::msg::QueryMsgFns as LpTokenQueryMsgFns;
    use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
    use euclid::msgs::virtual_balance::QueryMsgFns as VirtualBalanceQueryMsgFns;
    use euclid::utils::pagination::Pagination;
    use euclid::voucher::BalanceKey;
    use lp_token::LpTokenContract;

    fn setup_smart_denom_token(
        factory: &FactoryContract<MockBase>,
        token: Token,
    ) -> TokenWithDenom {
        let sender = factory.environment().sender.to_string();
        let chain = factory.environment();
        let cw20 = LpTokenContract::new(chain.clone());
        cw20.upload().unwrap();

        let aux_token = Token::create(format!("{}.aux", token)).unwrap();
        let token_pair = Pair::new(token.clone(), aux_token).unwrap();
        cw20.instantiate(
            &LpTokenInstantiateMsg {
                name: format!("{}_cw20", token),
                symbol: "SWAPIN".to_string(),
                decimals: 6,
                initial_balances: vec![Cw20Coin {
                    address: sender.clone(),
                    amount: Uint128::new(1_000_000_000),
                }],
                mint: Some(MinterResponse {
                    minter: sender,
                    cap: None,
                }),
                marketing: None,
                vlp: chain.addr_make("dummy_vlp").to_string(),
                factory: chain.addr_make("dummy_factory"),
                token_pair,
            },
            None,
            &[],
        )
        .unwrap();

        TokenWithDenom {
            token,
            token_type: TokenType::Smart {
                contract_address: cw20.address().unwrap().to_string(),
            },
        }
    }

    #[rstest]
    #[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL,  "empty", PoolConfig::ConstantProduct {}, false)]
    #[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL,  "single_voucher", PoolConfig::ConstantProduct {}, false)]
    #[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL,  "two_voucher", PoolConfig::ConstantProduct {}, false)]
    #[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC,  "empty", PoolConfig::ConstantProduct {}, false)]
    #[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC,  "single_voucher", PoolConfig::ConstantProduct {}, false)]
    #[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC,  "two_voucher", PoolConfig::ConstantProduct {}, false)]
    #[case(FactorySetupMode::Evm, FACTORY_CHAIN_ID_EVM,  "empty", PoolConfig::ConstantProduct {}, false)]
    #[case(FactorySetupMode::Evm, FACTORY_CHAIN_ID_EVM,  "single_voucher", PoolConfig::ConstantProduct {}, false)]
    #[case(FactorySetupMode::Evm, FACTORY_CHAIN_ID_EVM,  "two_voucher", PoolConfig::ConstantProduct {}, false)]
    #[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL,  "empty", PoolConfig::Stable { amp_factor: Some(Uint64::new(100)) }, false)]
    #[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC,  "single_voucher", PoolConfig::Stable { amp_factor: Some(Uint64::new(100)) }, false)]
    #[case(FactorySetupMode::Evm, FACTORY_CHAIN_ID_EVM, "two_voucher", PoolConfig::Stable { amp_factor: Some(Uint64::new(100)) }, false)]
    #[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL,  "empty", PoolConfig::ConstantProduct {}, true)]
    fn factory_full_flow_register_denom_and_deposit(
        #[case] mode: FactorySetupMode,
        #[case] factory_chain_id: &str,
        #[case] recipient_case: &str,
        #[case] pool_type: PoolConfig,
        #[case] use_smart_asset_in: bool,
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
        let recipients_for_checks = recipients.clone();
        let (_interchain, factory, router) = setup_factory_full_flow(
            sender,
            token_1.clone(),
            token_2.clone(),
            amount_1,
            amount_2,
            recipients.clone(),
            mode,
            pool_type,
            factory_chain_id,
            ROUTER_CHAIN_ID,
        );
        let pool_pair = Pair::new(token_1.token.clone(), token_2.token.clone()).unwrap();
        let tracked_recipients = if recipients_for_checks.is_empty() {
            vec![Recipient::default_voucher_recipient(
                CrossChainUser::new(chain_uid.clone(), factory.environment().sender.to_string()),
                Limit::Dynamic(Uint128::zero()),
            )]
        } else {
            recipients_for_checks.clone()
        };
        let initial_state = sync_state(
            &factory,
            &router,
            tracked_recipients,
            vec![token_1.token.clone(), token_2.token.clone()],
            vec![],
            vec![token_1.token.clone()],
            chain_uid.clone(),
            vec![pool_pair.clone()],
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
        let escrow_state = initial_state
            .escrow_balance(&chain_uid, &token_1.token)
            .expect("Escrow state for token should exist");
        assert_eq!(
            escrow_state.factory_escrow_balance,
            amount_1 + amount_2,
            "Escrow total amount should equal deposited amount",
        );

        // Assert router tracks correct escrow balance for this chain
        assert_eq!(
            escrow_state.router_escrow_balance,
            amount_1 + amount_2,
            "Router escrow balance should match deposited amount",
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
                let sender_amount = initial_state
                    .voucher_balance(&sender_user, &token_1.token)
                    .expect("Sender voucher balance should exist in synced state");
                assert_eq!(
                    sender_amount, amount_1,
                    "Sender should receive full virtual balance when no recipients specified"
                );
            }
            "single_voucher" => {
                let recipient_one =
                    CrossChainUser::new(chain_uid.clone(), "recipient_one".to_string());
                let recipient_balance = initial_state
                    .voucher_balance(&recipient_one, &token_1.token)
                    .expect("Recipient voucher balance should exist in synced state");
                assert_eq!(
                    recipient_balance, amount_1,
                    "Single recipient should receive entire virtual balance"
                );
            }
            "two_voucher" => {
                let recipient_one =
                    CrossChainUser::new(chain_uid.clone(), "recipient_one".to_string());
                let recipient_two =
                    CrossChainUser::new(chain_uid.clone(), "recipient_two".to_string());
                let balance_one = initial_state
                    .voucher_balance(&recipient_one, &token_1.token)
                    .expect("Recipient one token_1 balance should exist");
                let balance_two = initial_state
                    .voucher_balance(&recipient_two, &token_1.token)
                    .expect("Recipient two token_1 balance should exist");
                let balance_three = initial_state
                    .voucher_balance(&recipient_one, &token_2.token)
                    .expect("Recipient one token_2 balance should exist");
                let balance_four = initial_state
                    .voucher_balance(&recipient_two, &token_2.token)
                    .expect("Recipient two token_2 balance should exist");
                // Total distributed across recipients should equal the deposited amount
                assert_eq!(
                    balance_one + balance_two + balance_three + balance_four,
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
        let swap_asset_in = if use_smart_asset_in {
            let smart_asset_in = setup_smart_denom_token(&factory, token_1.token.clone());
            register_denom(&factory, &router, smart_asset_in.clone()).unwrap();
            smart_asset_in
        } else {
            token_1.clone()
        };

        // Partner fee: checked_mul_ceil(1000, 0.003) = ceil(3.0) = 3
        let partner_fee_amount = swap_amount
            .checked_mul_ceil(cosmwasm_std::Decimal::bps(partner_fee_bps))
            .unwrap();
        let net_swap_amount = swap_amount - partner_fee_amount;
        let smart_cw20_contract = match &swap_asset_in.token_type {
            TokenType::Smart { contract_address } => Some(get_lp_token(
                factory.environment(),
                &cosmwasm_std::Addr::unchecked(contract_address.clone()),
            )),
            _ => None,
        };
        let cw20_sender_balance_before = smart_cw20_contract
            .as_ref()
            .map(|cw20| cw20.balance(sender_addr.clone()).unwrap().balance);
        let cw20_factory_balance_before = smart_cw20_contract.as_ref().map(|cw20| {
            cw20.balance(factory.address().unwrap().to_string())
                .unwrap()
                .balance
        });

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

        let partner_native_balance_before = if swap_asset_in.token_type.is_native() {
            Some(
                factory
                    .environment()
                    .query_balance(
                        &cosmwasm_std::Addr::unchecked(sender_addr.clone()),
                        token_1.token.to_string().as_str(),
                    )
                    .unwrap(),
            )
        } else {
            None
        };

        // Execute the swap
        swap_request(
            &factory,
            &router,
            swap_asset_in.clone(),
            token_2.clone().token,
            swap_amount,
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
        )
        .unwrap();

        let user_funds_queries = if swap_asset_in.token_type.is_native() {
            vec![UserFundsQuery {
                chain_uid: chain_uid.clone(),
                chain: factory.environment().clone(),
                user_addr: sender_addr.clone(),
                denom: token_1.token.to_string(),
            }]
        } else {
            vec![]
        };

        // --- Post-swap assertions ---

        let post_swap_state = sync_state(
            &factory,
            &router,
            vec![Recipient::default_voucher_recipient(
                sender_user.clone(),
                Limit::Dynamic(Uint128::zero()),
            )],
            vec![token_2.token.clone()],
            user_funds_queries,
            vec![token_1.token.clone()],
            chain_uid.clone(),
            vec![pool_pair],
        );

        // 1-2. Escrow balances for input token increased by net swap amount
        let post_swap_escrow = post_swap_state
            .escrow_balance(&chain_uid, &token_1.token)
            .expect("Post-swap escrow state for token should exist");
        assert_eq!(
            post_swap_escrow.factory_escrow_balance,
            escrow_in_before + net_swap_amount,
            "Escrow for input token should increase by net swap amount (swap_amount - partner_fee)"
        );
        assert_eq!(
            post_swap_escrow.router_escrow_balance,
            router_escrow_in_before + net_swap_amount,
            "Router escrow balance for input token should increase by net swap amount"
        );

        // 3. Sender received output tokens as virtual balance
        let vb_out_after = post_swap_state
            .voucher_balance(&sender_user, &token_2.token)
            .expect("Sender output token voucher balance should exist");
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

        // 5. Partner fee recipient receives fee in the input token type.
        if let Some(native_before) = partner_native_balance_before {
            let partner_native_balance_after = post_swap_state
                .user_funds(&chain_uid, &sender_addr, token_1.token.to_string().as_str())
                .expect("Partner fee recipient native balance should exist");
            assert_eq!(
                partner_native_balance_after,
                native_before + partner_fee_amount,
                "Partner fee recipient should have received {} native input tokens as fee",
                partner_fee_amount
            );
        }
        if let (Some(cw20), Some(sender_before), Some(factory_before)) = (
            smart_cw20_contract.as_ref(),
            cw20_sender_balance_before,
            cw20_factory_balance_before,
        ) {
            let sender_after = cw20.balance(sender_addr).unwrap().balance;
            let factory_after = cw20
                .balance(factory.address().unwrap().to_string())
                .unwrap()
                .balance;
            assert_eq!(
                sender_after,
                sender_before - net_swap_amount,
                "Sender CW20 balance should decrease by net swap amount when partner fee recipient is sender"
            );
            assert_eq!(
                factory_after, factory_before,
                "Factory should not retain smart input tokens after swap execution"
            );
        }
    }
}
