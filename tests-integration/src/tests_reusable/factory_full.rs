#![cfg(not(target_arch = "wasm32"))]
use crate::helpers::chains::setup_router;
use crate::tests_reusable::factory_add_liquidity::deposit_token;
use crate::tests_reusable::factory_create_pool::create_pool;
use crate::tests_reusable::factory_register::{setup_factory, setup_factory_evm, FactorySetupMode};
use crate::tests_reusable::factory_register_denom::register_denom;
use crate::tests_reusable::factory_swap::swap_request;
use cosmwasm_std::{Uint128, Uint256};
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
    amount_1: Uint256,
    amount_2: Uint256,
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
    use crate::tests_reusable::factory_register_denom::setup_smart_denom_token;
    use crate::tests_reusable::state_sync::sync_state;
    use crate::tests_reusable::state_sync::UserFundsQuery;
    use crate::tests_reusable::test_macros::{decimal_pair, decimal_pair_full};
    use cosmwasm_std::Uint64;
    use cw_orch::prelude::ContractInstance as _;
    use cw_orch::prelude::Environment;
    use euclid::chain::ChainUid;
    use euclid::cross_chain_user::CrossChainUser;
    use euclid::limit::Limit;
    use euclid::msgs::escrow::QueryMsgFns as EscrowQueryMsgFns;
    use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
    use euclid::msgs::lp_token::msg::QueryMsgFns as LpTokenQueryMsgFns;
    use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
    use euclid::msgs::virtual_balance::QueryMsgFns as VirtualBalanceQueryMsgFns;
    use euclid::utils::pagination::Pagination;
    use euclid::voucher::BalanceKey;
    use rstest_reuse::apply;

    #[cfg_attr(not(feature = "full_decimals"), apply(decimal_pair))]
    #[cfg_attr(feature = "full_decimals", apply(decimal_pair_full))]
    fn factory_full_flow_register_denom_and_deposit(
        #[values(FactorySetupMode::Native, FactorySetupMode::Ibc, FactorySetupMode::Evm)]
        mode: FactorySetupMode,
        #[values(false, true)] use_smart_asset_in: bool,
        #[values("empty", "single_voucher", "two_voucher")] recipient_case: &str,
        #[values(PoolConfig::ConstantProduct {}, PoolConfig::Stable { amp_factor: Some(Uint64::new(100)) })]
        pool_type: PoolConfig,
        decimals_a: u32,
        decimals_b: u32,
    ) {
        let factory_chain_id = match mode {
            FactorySetupMode::Native => FACTORY_CHAIN_ID_LOCAL,
            FactorySetupMode::Ibc => FACTORY_CHAIN_ID_IBC,
            FactorySetupMode::Evm => FACTORY_CHAIN_ID_EVM,
        };
        let sender = "sender_for_all_chains";
        let token_1 = TokenWithDenom {
            token: Token::create("eucl".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "eucl".to_string(),
                decimals: Some(decimals_a),
            },
        };
        let decimal_a_multiplier = Uint256::from(10u128).pow(decimals_a);
        let decimal_b_multiplier = Uint256::from(10u128).pow(decimals_b);
        let amount_1 = Uint256::from(10_000u128)
            .checked_mul(decimal_a_multiplier)
            .unwrap();
        let token_2 = TokenWithDenom {
            token: Token::create("andr".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "andr".to_string(),
                decimals: Some(decimals_b),
            },
        };
        let amount_2 = Uint256::from(10_000u128)
            .checked_mul(decimal_b_multiplier)
            .unwrap();
        let chain_uid = ChainUid::create(factory_chain_id.to_string()).unwrap();
        let recipients = match recipient_case {
            "empty" => vec![],
            "single_voucher" => vec![Recipient::default_voucher_recipient(
                CrossChainUser::new(chain_uid.clone(), "recipient_one".to_string()),
                Limit::Dynamic(Uint256::zero()),
            )],
            "two_voucher" => vec![
                Recipient::default_voucher_recipient(
                    CrossChainUser::new(chain_uid.clone(), "recipient_one".to_string()),
                    Limit::Dynamic(Uint256::zero()),
                ),
                Recipient::default_voucher_recipient(
                    CrossChainUser::new(chain_uid.clone(), "recipient_two".to_string()),
                    Limit::Dynamic(Uint256::zero()),
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
                Limit::Dynamic(Uint256::zero()),
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
        let expected_token_1_escrow = Uint256::from(2u128) * amount_1;
        assert_eq!(
            escrow_state.factory_escrow_balance, expected_token_1_escrow,
            "Escrow total = deposit + pool creation, both contribute amount_1",
        );

        // Assert router tracks correct escrow balance for this chain
        assert_eq!(
            escrow_state.router_escrow_balance, expected_token_1_escrow,
            "Router escrow balance should match factory escrow",
        );

        let virtual_balance_address = router.get_state().unwrap().virtual_balance_address;
        let virtual_balance_contract =
            get_virtual_balance(router.environment(), &virtual_balance_address);
        // Assert token is registered on the router
        let all_tokens = virtual_balance_contract
            .get_all_token_metadata(Some(Pagination::new(None, None, None, None)))
            .unwrap();
        assert!(
            all_tokens.metadata.iter().any(|t| t.token == token_1.token),
            "Token should be registered on router"
        );

        // Assert token denom is registered on router for this chain
        let token_denoms = virtual_balance_contract
            .get_token_metadata(token_1.token.to_string(), None)
            .unwrap();
        assert!(
            token_denoms
                .metadata
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
                    sender_amount,
                    Uint256::from(amount_1),
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
                    recipient_balance,
                    Uint256::from(amount_1),
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
                    Uint256::from(amount_1 + amount_2),
                    "Total virtual balance across recipients should equal deposited amount"
                );
            }
            _ => unreachable!("unexpected recipient case"),
        }

        // --- Swap with partner fee ---

        let swap_amount = Uint256::from(1_000u128)
            .checked_mul(decimal_a_multiplier)
            .unwrap();
        let partner_fee_bps: u64 = 30;
        let sender_addr = factory.environment().sender.to_string();
        let partner_fee_recipient = factory
            .environment()
            .addr_make("partner_fee_recipient")
            .into_string();
        let swap_asset_in = if use_smart_asset_in {
            let token_1_decimals = token_1.token_type.get_decimals().unwrap();
            if token_1_decimals > 18 {
                return; // CW20 tokens only support up to 18 decimals
            }
            let smart_asset_in = setup_smart_denom_token(
                &factory.environment(),
                token_1.token.clone(),
                token_1_decimals,
            );
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
            TokenType::Smart {
                contract_address, ..
            } => Some(get_lp_token(
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

        let router_escrow_in_before: Uint256 = virtual_balance_contract
            .get_token_escrows(token_1.token.to_string(), None)
            .unwrap()
            .escrows
            .iter()
            .filter(|c| c.chain_uid == chain_uid)
            .map(|c| c.balance)
            .fold(Uint256::zero(), |acc, b| acc + b);

        let sender_user = CrossChainUser::new(chain_uid.clone(), sender_addr.clone());
        let partner_fee_recipient_user =
            CrossChainUser::new(chain_uid.clone(), partner_fee_recipient.clone());
        let vb_out_before_raw = virtual_balance_contract
            .get_balance(BalanceKey {
                cross_chain_user: sender_user.clone(),
                token_id: token_2.token.to_string(),
            })
            .unwrap()
            .amount;
        // De-normalize to match sync_state's de-normalized voucher balances
        let token_2_metadata = virtual_balance_contract
            .get_token_metadata(token_2.token.to_string(), None)
            .unwrap()
            .metadata;
        let vb_out_before = if let Some(metadata) = token_2_metadata.first() {
            euclid::normalize::normalize_voucher_to_token(
                vb_out_before_raw,
                metadata.token_type.get_decimals().unwrap(),
            )
            .unwrap()
        } else {
            vb_out_before_raw
        };

        // Execute the swap
        swap_request(
            &factory,
            &router,
            swap_asset_in.clone(),
            token_2.clone().token,
            swap_amount,
            Uint256::from(1u128),
            vec![NextSwapPair {
                token_in: token_1.token.clone(),
                token_out: token_2.token.clone(),
                pool_key: None,
                test_fail: None,
            }],
            vec![],
            Some(PartnerFee {
                partner_fee_bps,
                recipient: partner_fee_recipient.clone(),
            }),
        )
        .unwrap();

        let user_funds_queries = if swap_asset_in.token_type.is_native() {
            vec![
                UserFundsQuery {
                    chain_uid: chain_uid.clone(),
                    chain: factory.environment().clone(),
                    user_addr: sender_addr.clone(),
                    denom: token_1.token.to_string(),
                },
                UserFundsQuery {
                    chain_uid: chain_uid.clone(),
                    chain: factory.environment().clone(),
                    user_addr: partner_fee_recipient.clone(),
                    denom: token_1.token.to_string(),
                },
            ]
        } else {
            vec![]
        };

        // --- Post-swap assertions ---

        let post_swap_state = sync_state(
            &factory,
            &router,
            vec![
                Recipient::default_voucher_recipient(
                    sender_user.clone(),
                    Limit::Dynamic(Uint256::zero()),
                ),
                Recipient::default_voucher_recipient(
                    partner_fee_recipient_user.clone(),
                    Limit::Dynamic(Uint256::zero()),
                ),
            ],
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
            router_escrow_in_before + Uint256::from(net_swap_amount),
            "Router escrow balance for input token should increase by net swap amount"
        );

        // 3. Sender received output tokens as virtual balance
        let vb_out_after = post_swap_state
            .voucher_balance(&sender_user, &token_2.token)
            .expect("Sender output token voucher balance should exist");
        let amount_received = vb_out_after.checked_sub(vb_out_before).unwrap();
        assert!(
            amount_received > Uint256::zero(),
            "Sender should have received output tokens as virtual balance, got 0"
        );

        // 4. Output amount should be less than net input (in normalized 24-dec units)
        let normalized_received =
            euclid::normalize::normalize_token_to_voucher(amount_received, decimals_b).unwrap();
        let normalized_input =
            euclid::normalize::normalize_token_to_voucher(net_swap_amount, decimals_a).unwrap();
        assert!(
            normalized_received < normalized_input,
            "Normalized output ({}) should be less than normalized input ({}) for equal-reserve pools",
            normalized_received,
            normalized_input
        );

        // 5. Partner fee recipient receives fee in the input token type.
        match &swap_asset_in.token_type {
            TokenType::Native { denom, .. } => {
                let partner_native_balance_after = post_swap_state
                    .user_funds(&chain_uid, &partner_fee_recipient, denom)
                    .expect("Partner fee recipient native balance should exist");
                assert_eq!(
                    partner_native_balance_after, partner_fee_amount,
                    "Partner fee recipient should have received {} native input tokens as fee",
                    partner_fee_amount
                );
            }
            TokenType::Smart {
                contract_address, ..
            } => {
                let cw20 = get_lp_token(
                    factory.environment(),
                    &cosmwasm_std::Addr::unchecked(contract_address.clone()),
                );
                let partner_fee_recipient_balance_after =
                    cw20.balance(partner_fee_recipient).unwrap().balance;
                assert_eq!(
                    Uint256::from(partner_fee_recipient_balance_after),
                    partner_fee_amount,
                    "Partner fee recipient should have received {} smart input tokens as fee",
                    partner_fee_amount
                );
            }
            TokenType::Voucher { .. } => {
                let partner_fee_recipient_user =
                    CrossChainUser::new(chain_uid.clone(), partner_fee_recipient.clone());
                let partner_voucher_balance_after = post_swap_state
                    .voucher_balance(&partner_fee_recipient_user, &swap_asset_in.token)
                    .expect("Partner fee recipient voucher balance should exist");
                assert_eq!(partner_voucher_balance_after, partner_fee_amount,);
            }
        };
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
                sender_before - Uint128::try_from(swap_amount).unwrap(),
                "Sender CW20 balance should decrease by full swap amount (CW20 hook sends entire amount to factory)"
            );
            assert_eq!(
                factory_after, factory_before,
                "Factory should not retain smart input tokens after swap execution"
            );
        }
    }
}
