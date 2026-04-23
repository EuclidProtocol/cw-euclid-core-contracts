#![cfg(not(target_arch = "wasm32"))]
use crate::helpers::chains::setup_router;
use crate::helpers::multi_chain::MultiChainEnv;
use crate::tests_reusable::factory_add_liquidity::deposit_token;
use crate::tests_reusable::factory_create_pool::create_pool;
use crate::tests_reusable::factory_register::{setup_factory_with_mode, FactorySetupMode};
use crate::tests_reusable::factory_register_denom::register_denom;
use crate::tests_reusable::factory_swap::swap_request;
use cosmwasm_std::{Addr, Uint128};
use euclid::fee::PartnerFee;
use euclid::msgs::vlp::base::PoolConfig;
use euclid::recipient::Recipient;
use euclid::swap::NextSwapPair;
use euclid::token::{
    Pair, PairWithDenomAndAmount, Token, TokenType, TokenWithDenom, TokenWithDenomAndAmount,
};
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
) -> (MultiChainEnv, Addr, Addr) {
    use crate::helpers::chains::setup_interchain;

    let mut env = setup_interchain(sender, factory_chain_id);
    let router_addr =
        setup_router(env.chain_mut(router_chain_id), vec![factory_chain_id]).unwrap();
    let factory_addr =
        setup_factory_with_mode(&mut env, factory_chain_id, router_chain_id, &router_addr, mode)
            .unwrap();

    register_denom(
        &factory_addr,
        factory_chain_id,
        &router_addr,
        router_chain_id,
        &mut env,
        token_1.clone(),
    )
    .unwrap();
    register_denom(
        &factory_addr,
        factory_chain_id,
        &router_addr,
        router_chain_id,
        &mut env,
        token_2.clone(),
    )
    .unwrap();

    deposit_token(
        &factory_addr,
        factory_chain_id,
        &router_addr,
        router_chain_id,
        &mut env,
        token_1.clone(),
        amount_1,
        recipients.clone(),
    )
    .unwrap();
    deposit_token(
        &factory_addr,
        factory_chain_id,
        &router_addr,
        router_chain_id,
        &mut env,
        token_2.clone(),
        amount_2,
        recipients,
    )
    .unwrap();

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
        &factory_addr,
        factory_chain_id,
        &router_addr,
        router_chain_id,
        &mut env,
        pair_with_denom_and_amount,
        slippage_tolerance_bps,
        pool_type,
    )
    .unwrap();

    (env, factory_addr, router_addr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helpers::chains::{get_escrow_addr, get_virtual_balance_addr, lp_token_code, setup_interchain};
    use crate::tests_reusable::constants::{
        FACTORY_CHAIN_ID_EVM, FACTORY_CHAIN_ID_IBC, FACTORY_CHAIN_ID_LOCAL, ROUTER_CHAIN_ID,
    };
    use crate::tests_reusable::state_sync::{sync_state, UserFundsQuery};
    use cosmwasm_std::Uint64;
    use euclid::chain::ChainUid;
    use euclid::cross_chain_user::CrossChainUser;
    use euclid::cw20_types::{BalanceResponse, Cw20Coin, MinterResponse};
    use euclid::limit::Limit;
    use euclid::msgs::lp_token::msg::{InstantiateMsg as LpTokenInstantiateMsg, QueryMsg as LpTokenQueryMsg};
    use euclid::utils::pagination::Pagination;
    use euclid::voucher::BalanceKey;

    fn setup_smart_denom_token(
        env: &mut MultiChainEnv,
        factory_chain_id: &str,
        token: Token,
    ) -> TokenWithDenom {
        let app = env.chain_mut(factory_chain_id);
        let sender = app.sender();
        let code_id = lp_token_code(app);
        let aux_token = Token::create(format!("{}.aux", token)).unwrap();
        let token_pair = Pair::new(token.clone(), aux_token).unwrap();
        let lp_addr = app.instantiate(
            code_id,
            &sender,
            &LpTokenInstantiateMsg {
                name: format!("{}_cw20", token),
                symbol: "SWAPIN".to_string(),
                decimals: 6,
                initial_balances: vec![Cw20Coin {
                    address: sender.to_string(),
                    amount: Uint128::new(1_000_000_000),
                }],
                mint: Some(MinterResponse {
                    minter: sender.to_string(),
                    cap: None,
                }),
                marketing: None,
                vlp: app.addr_make("dummy_vlp").to_string(),
                factory: app.addr_make("dummy_factory"),
                token_pair,
            },
            &[],
            "lp_smart_token",
        );

        TokenWithDenom {
            token,
            token_type: TokenType::Smart {
                contract_address: lp_addr.to_string(),
            },
        }
    }

    fn lp_balance(env: &MultiChainEnv, chain_id: &str, lp_addr: &Addr, address: String) -> Uint128 {
        let resp: BalanceResponse = env
            .chain(chain_id)
            .query(lp_addr, &LpTokenQueryMsg::Balance { address });
        resp.balance
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
        let (mut env, factory_addr, router_addr) = setup_factory_full_flow(
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
        let sender_addr = env.chain(factory_chain_id).sender().to_string();
        let tracked_recipients = if recipients_for_checks.is_empty() {
            vec![Recipient::default_voucher_recipient(
                CrossChainUser::new(chain_uid.clone(), sender_addr.clone()),
                Limit::Dynamic(Uint128::zero()),
            )]
        } else {
            recipients_for_checks.clone()
        };

        let initial_state = sync_state(
            factory_chain_id,
            &factory_addr,
            ROUTER_CHAIN_ID,
            &router_addr,
            &env,
            tracked_recipients,
            vec![token_1.token.clone(), token_2.token.clone()],
            vec![],
            vec![token_1.token.clone()],
            chain_uid.clone(),
            vec![pool_pair.clone()],
        );

        let all_chains: euclid::msgs::router::AllChainResponse = env
            .chain(ROUTER_CHAIN_ID)
            .query(&router_addr, &euclid::msgs::router::QueryMsg::GetAllChains {});
        assert!(
            all_chains.chains.iter().any(|c| c.chain_uid == chain_uid),
            "Factory chain should be registered on router"
        );

        let escrow_response: euclid::msgs::factory::GetEscrowResponse = env
            .chain(factory_chain_id)
            .query(
                &factory_addr,
                &euclid::msgs::factory::QueryMsg::GetEscrow {
                    token_id: token_1.token.to_string(),
                },
            );
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

        let escrow_state = initial_state
            .escrow_balance(&chain_uid, &token_1.token)
            .expect("Escrow state for token should exist");
        assert_eq!(
            escrow_state.factory_escrow_balance,
            amount_1 + amount_2,
            "Escrow total amount should equal deposited amount",
        );

        assert_eq!(
            escrow_state.router_escrow_balance,
            amount_1 + amount_2,
            "Router escrow balance should match deposited amount",
        );

        let all_tokens: euclid::msgs::router::AllTokensResponse = env
            .chain(ROUTER_CHAIN_ID)
            .query(
                &router_addr,
                &euclid::msgs::router::QueryMsg::QueryAllTokens {
                    pagination: Pagination::new(None, None, None, None),
                },
            );
        assert!(
            all_tokens.tokens.iter().any(|t| t == &token_1.token),
            "Token should be registered on router"
        );

        let token_denoms: euclid::msgs::router::QueryTokenDenomsResponse = env
            .chain(ROUTER_CHAIN_ID)
            .query(
                &router_addr,
                &euclid::msgs::router::QueryMsg::QueryTokenDenoms {
                    token: token_1.token.clone(),
                },
            );
        assert!(
            token_denoms
                .denoms
                .iter()
                .any(|d| d.chain_uid == chain_uid && d.token_type == token_1.token_type),
            "Token denom should be registered on router for factory chain"
        );

        let virtual_balance_addr = get_virtual_balance_addr(env.chain(ROUTER_CHAIN_ID), &router_addr);
        match recipient_case {
            "empty" => {
                let sender_user =
                    CrossChainUser::new(chain_uid.clone(), sender_addr.clone());
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
        let swap_asset_in = if use_smart_asset_in {
            let smart_asset_in =
                setup_smart_denom_token(&mut env, factory_chain_id, token_1.token.clone());
            register_denom(
                &factory_addr,
                factory_chain_id,
                &router_addr,
                ROUTER_CHAIN_ID,
                &mut env,
                smart_asset_in.clone(),
            )
            .unwrap();
            smart_asset_in
        } else {
            token_1.clone()
        };

        let partner_fee_amount = swap_amount
            .checked_mul_ceil(cosmwasm_std::Decimal::bps(partner_fee_bps))
            .unwrap();
        let net_swap_amount = swap_amount - partner_fee_amount;

        let cw20_lp_addr = match &swap_asset_in.token_type {
            TokenType::Smart { contract_address } => {
                Some(Addr::unchecked(contract_address.clone()))
            }
            _ => None,
        };
        let cw20_sender_balance_before = cw20_lp_addr
            .as_ref()
            .map(|addr| lp_balance(&env, factory_chain_id, addr, sender_addr.clone()));
        let cw20_factory_balance_before = cw20_lp_addr
            .as_ref()
            .map(|addr| lp_balance(&env, factory_chain_id, addr, factory_addr.to_string()));

        let escrow_addr =
            get_escrow_addr(env.chain(factory_chain_id), &factory_addr, token_1.token.as_str());
        let escrow_in_before: euclid::msgs::escrow::StateResponse = env
            .chain(factory_chain_id)
            .query(&escrow_addr, &euclid::msgs::escrow::QueryMsg::State {});
        let escrow_in_before = escrow_in_before.total_amount;

        let router_escrow_in_before: euclid::msgs::router::TokenEscrowsResponse =
            env.chain(ROUTER_CHAIN_ID).query(
                &router_addr,
                &euclid::msgs::router::QueryMsg::QueryTokenEscrows {
                    token: token_1.token.clone(),
                    pagination: Pagination::new(Some(chain_uid.clone()), None, None, Some(1)),
                },
            );
        let router_escrow_in_before = router_escrow_in_before
            .chains
            .first()
            .map(|c| c.balance)
            .unwrap_or(Uint128::zero());

        let sender_user = CrossChainUser::new(chain_uid.clone(), sender_addr.clone());
        let vb_out_before: euclid::msgs::virtual_balance::GetBalanceResponse =
            env.chain(ROUTER_CHAIN_ID).query(
                &virtual_balance_addr,
                &euclid::msgs::virtual_balance::QueryMsg::GetBalance {
                    balance_key: BalanceKey {
                        cross_chain_user: sender_user.clone(),
                        token_id: token_2.token.to_string(),
                    },
                },
            );
        let vb_out_before = vb_out_before.amount;

        let partner_native_balance_before = if swap_asset_in.token_type.is_native() {
            Some(env.chain(factory_chain_id).query_balance(
                &Addr::unchecked(sender_addr.clone()),
                token_1.token.to_string().as_str(),
            ))
        } else {
            None
        };

        let tx_funds = if swap_asset_in.token_type.is_native() {
            let native_denom = swap_asset_in.token_type.get_denom().unwrap();
            let factory_sender = env.chain(factory_chain_id).sender();
            crate::helpers::factory::faucet(
                env.chain_mut(factory_chain_id),
                &factory_sender,
                swap_amount.u128(),
                swap_asset_in.token_type.clone(),
                &mut vec![],
            );
            vec![cosmwasm_std::coin(swap_amount.u128(), native_denom)]
        } else {
            vec![]
        };

        swap_request(
            &factory_addr,
            factory_chain_id,
            &router_addr,
            ROUTER_CHAIN_ID,
            &mut env,
            swap_asset_in.clone(),
            swap_amount,
            token_2.token.clone(),
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
            tx_funds,
        )
        .unwrap();

        let user_funds_queries = if swap_asset_in.token_type.is_native() {
            vec![UserFundsQuery {
                chain_uid: chain_uid.clone(),
                chain_id: factory_chain_id.to_string(),
                user_addr: sender_addr.clone(),
                denom: token_1.token.to_string(),
            }]
        } else {
            vec![]
        };

        let post_swap_state = sync_state(
            factory_chain_id,
            &factory_addr,
            ROUTER_CHAIN_ID,
            &router_addr,
            &env,
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

        let vb_out_after = post_swap_state
            .voucher_balance(&sender_user, &token_2.token)
            .expect("Sender output token voucher balance should exist");
        let amount_received = vb_out_after - vb_out_before;
        assert!(
            amount_received > Uint128::zero(),
            "Sender should have received output tokens as virtual balance, got 0"
        );

        assert!(
            amount_received < net_swap_amount,
            "Amount received ({}) should be less than net input ({}) for equal-reserve pools",
            amount_received,
            net_swap_amount
        );

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
        if let (Some(lp_addr), Some(sender_before), Some(factory_before)) = (
            cw20_lp_addr.as_ref(),
            cw20_sender_balance_before,
            cw20_factory_balance_before,
        ) {
            let sender_after =
                lp_balance(&env, factory_chain_id, lp_addr, sender_addr.clone());
            let factory_after =
                lp_balance(&env, factory_chain_id, lp_addr, factory_addr.to_string());
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
