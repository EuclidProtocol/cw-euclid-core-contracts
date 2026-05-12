#![cfg(not(target_arch = "wasm32"))]
use cosmwasm_std::{coin, to_json_binary, Addr, Uint128, Uint256};
use euclid::cw20_types::{Cw20Coin, MinterResponse};
use euclid::fee::PartnerFee;
use euclid::msgs::cross_chain_config::CrossChainConfig;
use euclid::msgs::factory::cw20::FactoryCw20HookMsg;
use euclid::msgs::factory::ExecuteSwapRequest;
use euclid::msgs::lp_token::msg::InstantiateMsg as LpTokenInstantiateMsg;
use euclid::recipient::Recipient;
use euclid::swap::NextSwapPair;
use euclid::token::{Pair, PairWithDenomAndAmount, Token, TokenType, TokenWithDenom};

use crate::helpers::app::EuclidApp;
use crate::helpers::chains::lp_token_code;
use crate::helpers::factory::faucet;
use crate::helpers::multi_chain::MultiChainEnv;
use crate::helpers::relayer::relay_factory_router_factory;

/// Execute a swap request. For native tokens the funds slice must contain the input coin.
/// For smart (CW20) tokens, funds should be empty and the CW20 Send is done internally.
pub fn swap_request(
    factory_addr: &Addr,
    factory_chain_id: &str,
    router_addr: &Addr,
    router_chain_id: &str,
    env: &mut MultiChainEnv,
    asset_in: TokenWithDenom,
    amount_in: Uint256,
    asset_out: Token,
    min_amount_out: Uint256,
    swaps: Vec<NextSwapPair>,
    recipients: Vec<Recipient>,
    partner_fee: Option<PartnerFee>,
    funds: Vec<cosmwasm_std::Coin>,
) -> Result<(), anyhow::Error> {
    let factory_chain_uid = {
        let state: euclid::msgs::factory::StateResponse = env
            .chain(factory_chain_id)
            .query(factory_addr, &euclid::msgs::factory::QueryMsg::GetState {});
        state.chain_uid
    };

    let sender = env.chain(factory_chain_id).sender();

    let tx_response = if asset_in.token_type.is_smart() {
        let smart_contract = match &asset_in.token_type {
            TokenType::Smart {
                contract_address, ..
            } => Addr::unchecked(contract_address.clone()),
            _ => unreachable!(),
        };
        env.chain_mut(factory_chain_id).execute(
            &sender,
            &smart_contract,
            &euclid::msgs::lp_token::msg::ExecuteMsg::Send {
                contract: factory_addr.to_string(),
                amount: amount_in,
                msg: to_json_binary(&FactoryCw20HookMsg::Swap {
                    recipients,
                    asset_in: asset_in.clone(),
                    asset_out,
                    min_amount_out,
                    swaps,
                    partner_fee,
                    cross_chain_config: CrossChainConfig::default(),
                })
                .unwrap(),
            },
            &[],
        )
    } else {
        env.chain_mut(factory_chain_id).execute(
            &sender,
            factory_addr,
            &euclid::msgs::factory::ExecuteMsg::ExecuteSwapRequest(ExecuteSwapRequest {
                amount_in,
                recipients,
                asset_in: asset_in.clone(),
                asset_out,
                min_amount_out,
                swaps,
                partner_fee,
                cross_chain_config: CrossChainConfig::default(),
            }),
            &funds,
        )
    };

    relay_factory_router_factory(
        tx_response.events,
        factory_chain_id,
        factory_addr,
        &factory_chain_uid,
        router_chain_id,
        router_addr,
        env,
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helpers::chains::{
        get_escrow_addr, get_virtual_balance_addr, setup_interchain, setup_router,
    };
    use crate::tests_reusable::constants::{
        FACTORY_CHAIN_ID_EVM, FACTORY_CHAIN_ID_IBC, FACTORY_CHAIN_ID_LOCAL, ROUTER_CHAIN_ID,
    };
    use crate::tests_reusable::factory_add_liquidity::deposit_token;
    use crate::tests_reusable::factory_create_pool::create_pool;
    use crate::tests_reusable::factory_register::{setup_factory_with_mode, FactorySetupMode};
    use crate::tests_reusable::factory_register_denom::register_denom;
    use crate::tests_reusable::state_sync::sync_state;
    use euclid::chain::ChainUid;
    use euclid::cross_chain_user::CrossChainUser;
    use euclid::cw20_types::BalanceResponse;
    use euclid::limit::Limit;
    use euclid::msgs::lp_token::msg::QueryMsg as LpTokenQueryMsg;
    use euclid::msgs::vlp::base::PoolConfig;

    use euclid::voucher::BalanceKey;
    use rstest::rstest;

    fn mode_for(factory_chain_id: &str) -> FactorySetupMode {
        if factory_chain_id == ROUTER_CHAIN_ID {
            FactorySetupMode::Native
        } else if factory_chain_id == FACTORY_CHAIN_ID_EVM {
            FactorySetupMode::Evm
        } else {
            FactorySetupMode::Ibc
        }
    }

    fn native_token(name: &str) -> TokenWithDenom {
        TokenWithDenom {
            token: Token::create(name.to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: name.to_string(),
                decimals: Some(6),
            },
        }
    }

    fn setup_smart_denom_token(app: &mut EuclidApp, token: Token) -> TokenWithDenom {
        let sender = app.sender();
        let code_id = lp_token_code(app);
        let aux_token = Token::create(format!("{token}.aux")).unwrap();
        let token_pair = Pair::new(token.clone(), aux_token).unwrap();
        let lp_addr = app.instantiate(
            code_id,
            &sender,
            &LpTokenInstantiateMsg {
                name: format!("{token}_cw20"),
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
                decimals: Some(6),
            },
        }
    }

    fn lp_balance(app: &EuclidApp, lp_addr: &Addr, address: String) -> Uint128 {
        let resp: BalanceResponse = app.query(lp_addr, &LpTokenQueryMsg::Balance { address });
        resp.balance
    }

    #[rstest]
    #[case::single_swap(1, FACTORY_CHAIN_ID_LOCAL, false)]
    #[case::two_hop_swap(2, FACTORY_CHAIN_ID_LOCAL, false)]
    #[case::three_hop_swap(3, FACTORY_CHAIN_ID_LOCAL, false)]
    #[case::single_swap(1, FACTORY_CHAIN_ID_IBC, false)]
    #[case::two_hop_swap(2, FACTORY_CHAIN_ID_IBC, false)]
    #[case::three_hop_swap(3, FACTORY_CHAIN_ID_IBC, false)]
    #[case::single_swap(1, FACTORY_CHAIN_ID_EVM, false)]
    #[case::two_hop_swap(2, FACTORY_CHAIN_ID_EVM, false)]
    #[case::three_hop_swap(3, FACTORY_CHAIN_ID_EVM, false)]
    #[case::single_swap_smart_in(1, FACTORY_CHAIN_ID_LOCAL, true)]
    fn test_swap_with_n_hops(
        #[case] num_swaps: usize,
        #[case] factory_chain_id: &str,
        #[case] use_smart_asset_in: bool,
    ) {
        let sender = "sender_for_all_chains";
        let mut env = setup_interchain(sender, factory_chain_id);
        let router_addr =
            setup_router(env.chain_mut(ROUTER_CHAIN_ID), vec![factory_chain_id]).unwrap();
        let factory_addr = setup_factory_with_mode(
            &mut env,
            factory_chain_id,
            ROUTER_CHAIN_ID,
            &router_addr,
            mode_for(factory_chain_id),
        )
        .unwrap();

        let chain_uid = ChainUid::create(factory_chain_id.to_string()).unwrap();

        let token_names: Vec<String> = (0..=num_swaps)
            .map(|i| format!("token{}", (b'a' + i as u8) as char))
            .collect();
        let tokens: Vec<TokenWithDenom> = token_names.iter().map(|n| native_token(n)).collect();

        for token in &tokens {
            register_denom(
                &factory_addr,
                factory_chain_id,
                &router_addr,
                ROUTER_CHAIN_ID,
                &mut env,
                token.clone(),
            )
            .unwrap();
        }

        let deposit_amount = Uint256::from(100_000u128);

        for window in tokens.windows(2) {
            let token_a = &window[0];
            let token_b = &window[1];

            deposit_token(
                &factory_addr,
                factory_chain_id,
                &router_addr,
                ROUTER_CHAIN_ID,
                &mut env,
                token_a.clone(),
                deposit_amount,
                vec![],
            )
            .unwrap();
            deposit_token(
                &factory_addr,
                factory_chain_id,
                &router_addr,
                ROUTER_CHAIN_ID,
                &mut env,
                token_b.clone(),
                deposit_amount,
                vec![],
            )
            .unwrap();

            let pair = PairWithDenomAndAmount {
                token_1: token_a.with_amount(deposit_amount),
                token_2: token_b.with_amount(deposit_amount),
            };
            create_pool(
                &factory_addr,
                factory_chain_id,
                &router_addr,
                ROUTER_CHAIN_ID,
                &mut env,
                pair,
                500,
                PoolConfig::ConstantProduct {},
            )
            .unwrap();
        }

        let swaps: Vec<NextSwapPair> = tokens
            .windows(2)
            .map(|w| NextSwapPair {
                token_in: w[0].token.clone(),
                token_out: w[1].token.clone(),
                test_fail: None,
            })
            .collect();

        let asset_in_native = tokens.first().unwrap().clone();
        let asset_in = if use_smart_asset_in {
            let smart_asset_in = setup_smart_denom_token(
                env.chain_mut(factory_chain_id),
                asset_in_native.token.clone(),
            );
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
            asset_in_native
        };
        let asset_out = tokens.last().unwrap().clone();
        let swap_amount = 1_000u128;
        let sender_addr = env.chain(factory_chain_id).sender().to_string();

        let cw20_lp_addr = match &asset_in.token_type {
            TokenType::Smart {
                contract_address, ..
            } => Some(Addr::unchecked(contract_address.clone())),
            _ => None,
        };
        let cw20_sender_balance_before = cw20_lp_addr
            .as_ref()
            .map(|addr| lp_balance(env.chain(factory_chain_id), addr, sender_addr.clone()));
        let cw20_factory_balance_before = cw20_lp_addr
            .as_ref()
            .map(|addr| lp_balance(env.chain(factory_chain_id), addr, factory_addr.to_string()));

        let escrow_addr = get_escrow_addr(
            env.chain(factory_chain_id),
            &factory_addr,
            asset_in.token.as_str(),
        );
        let escrow_state_before: euclid::msgs::escrow::StateResponse = env
            .chain(factory_chain_id)
            .query(&escrow_addr, &euclid::msgs::escrow::QueryMsg::State {});
        let escrow_in_before = escrow_state_before.total_amount;

        let virtual_balance_addr_init =
            get_virtual_balance_addr(env.chain(ROUTER_CHAIN_ID), &router_addr);
        let router_escrow_before: euclid::msgs::virtual_balance::GetTokenEscrowsResponse =
            env.chain(ROUTER_CHAIN_ID).query(
                &virtual_balance_addr_init,
                &euclid::msgs::virtual_balance::QueryMsg::GetTokenEscrows {
                    token_id: asset_in.token.to_string(),
                    pagination: None,
                },
            );
        let router_escrow_in_before = router_escrow_before
            .escrows
            .iter()
            .find(|e| e.chain_uid == chain_uid)
            .map_or(Uint256::zero(), |e| e.balance);

        let virtual_balance_addr =
            get_virtual_balance_addr(env.chain(ROUTER_CHAIN_ID), &router_addr);
        let sender_user = CrossChainUser::new(chain_uid.clone(), sender_addr.clone());
        let vb_out_before: euclid::msgs::virtual_balance::GetBalanceResponse =
            env.chain(ROUTER_CHAIN_ID).query(
                &virtual_balance_addr,
                &euclid::msgs::virtual_balance::QueryMsg::GetBalance {
                    balance_key: BalanceKey {
                        cross_chain_user: sender_user.clone(),
                        token_id: asset_out.token.to_string(),
                    },
                },
            );
        let vb_out_before = vb_out_before.amount;

        // For native tokens, faucet the sender and include funds
        let tx_funds = if asset_in.token_type.is_native() {
            let native_denom = asset_in.token_type.get_denom().unwrap();
            let factory_sender = env.chain(factory_chain_id).sender();
            faucet(
                env.chain_mut(factory_chain_id),
                &factory_sender,
                swap_amount,
                asset_in.token_type.clone(),
                &mut vec![],
            );
            vec![coin(swap_amount, native_denom)]
        } else {
            vec![]
        };

        swap_request(
            &factory_addr,
            factory_chain_id,
            &router_addr,
            ROUTER_CHAIN_ID,
            &mut env,
            asset_in.clone(),
            Uint256::new(swap_amount),
            asset_out.token.clone(),
            Uint256::new(1),
            swaps.clone(),
            vec![],
            None,
            tx_funds,
        )
        .unwrap();

        let recipients_for_sync = vec![Recipient::default_voucher_recipient(
            sender_user.clone(),
            Limit::Dynamic(Uint256::zero()),
        )];
        let vlp_pairs = swaps
            .iter()
            .map(|swap| Pair::new(swap.token_in.clone(), swap.token_out.clone()).unwrap())
            .collect();

        let state_sync = sync_state(
            factory_chain_id,
            &factory_addr,
            ROUTER_CHAIN_ID,
            &router_addr,
            &env,
            recipients_for_sync,
            vec![asset_out.token.clone()],
            vec![],
            vec![asset_in.token.clone()],
            chain_uid.clone(),
            vlp_pairs,
        );

        let escrow_state = state_sync
            .escrow_balance(&chain_uid, &asset_in.token)
            .expect("Escrow state for input token should exist");
        assert_eq!(
            escrow_state.factory_escrow_balance,
            escrow_in_before + Uint256::new(swap_amount),
            "Escrow balance for input token should increase by swap amount"
        );

        if !use_smart_asset_in {
            assert_eq!(
                escrow_state.router_escrow_balance,
                router_escrow_in_before + Uint256::new(swap_amount),
                "Router escrow balance for input token should increase by swap amount"
            );
        }

        let vb_out_after = state_sync
            .voucher_balance(&sender_user, &asset_out.token)
            .expect("Voucher balance for sender and output token should exist");
        let amount_received = vb_out_after - vb_out_before;
        assert!(
            amount_received > Uint256::zero(),
            "Sender should have received output tokens, got 0"
        );

        let swap_amount_normalized =
            euclid::normalize::normalize_token_to_voucher(Uint256::new(swap_amount), 6).unwrap();
        assert!(
            amount_received < swap_amount_normalized,
            "Amount received ({amount_received}) should be less than amount in ({swap_amount}) for equal-reserve pools"
        );

        if let (Some(lp_addr), Some(sender_before), Some(factory_before)) = (
            cw20_lp_addr.as_ref(),
            cw20_sender_balance_before,
            cw20_factory_balance_before,
        ) {
            let sender_after =
                lp_balance(env.chain(factory_chain_id), lp_addr, sender_addr.clone());
            let factory_after = lp_balance(
                env.chain(factory_chain_id),
                lp_addr,
                factory_addr.to_string(),
            );
            assert_eq!(
                sender_after,
                sender_before - Uint128::new(swap_amount),
                "Sender CW20 balance should decrease by swap amount for smart-token swaps"
            );
            assert_eq!(
                factory_after, factory_before,
                "Factory should not retain smart input tokens after swap execution"
            );
        }
    }
}
