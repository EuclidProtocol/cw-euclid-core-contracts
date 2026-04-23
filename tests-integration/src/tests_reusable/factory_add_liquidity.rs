#![cfg(not(target_arch = "wasm32"))]
use cosmwasm_std::{Addr, Coin, Uint128};
use euclid::msgs::cross_chain_config::CrossChainConfig;
use euclid::recipient::Recipient;
use euclid::token::{PairWithDenomAndAmount, TokenWithDenom};

use crate::helpers::factory::faucet;
use crate::helpers::multi_chain::MultiChainEnv;
use crate::helpers::relayer::relay_factory_router_factory;

pub fn deposit_token(
    factory_addr: &Addr,
    factory_chain_id: &str,
    router_addr: &Addr,
    router_chain_id: &str,
    env: &mut MultiChainEnv,
    token: TokenWithDenom,
    amount: Uint128,
    recipients: Vec<Recipient>,
) -> Result<(), anyhow::Error> {
    crate::helpers::factory::deposit_token(
        factory_addr,
        factory_chain_id,
        router_addr,
        router_chain_id,
        env,
        token,
        amount,
        recipients,
    )
}

pub fn add_liquidity(
    factory_addr: &Addr,
    factory_chain_id: &str,
    router_addr: &Addr,
    router_chain_id: &str,
    env: &mut MultiChainEnv,
    pair_with_denom: PairWithDenomAndAmount,
    slippage_tolerance_bps: u64,
) -> Result<(), anyhow::Error> {
    let sender = env.chain(factory_chain_id).sender();
    let mut funds: Vec<Coin> = vec![];
    for token in pair_with_denom.get_vec_token_info() {
        faucet(
            env.chain_mut(factory_chain_id),
            &sender,
            token.amount.u128(),
            token.token_type.clone(),
            &mut funds,
        );
    }

    let factory_chain_uid = {
        let factory_state: euclid::msgs::factory::StateResponse = env.chain(factory_chain_id).query(
            factory_addr,
            &euclid::msgs::factory::QueryMsg::GetState {},
        );
        factory_state.chain_uid
    };

    println!("Execute Add Liquidity {:?}", pair_with_denom);
    let tx_response = env.chain_mut(factory_chain_id).execute(
        &sender,
        factory_addr,
        &euclid::msgs::factory::ExecuteMsg::AddLiquidity {
            pair_with_denom_and_amount: pair_with_denom,
            slippage_tolerance_bps,
            cross_chain_config: CrossChainConfig::default(),
        },
        &funds,
    );

    println!("Relay Add Liquidity {:?}", tx_response.events);
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
    use crate::helpers::chains::{setup_interchain, setup_router};
    use crate::helpers::factory::faucet;
    use crate::tests_reusable::constants::{
        FACTORY_CHAIN_ID_IBC, FACTORY_CHAIN_ID_LOCAL, ROUTER_CHAIN_ID,
    };
    use crate::tests_reusable::factory_create_pool::create_pool;
    use crate::tests_reusable::factory_register::{setup_factory_with_mode, FactorySetupMode};
    use crate::tests_reusable::factory_register_denom::register_denom;
    use crate::tests_reusable::state_sync::{sync_state, UserFundsQuery};
    use euclid::chain::ChainUid;
    use euclid::cross_chain_user::CrossChainUser;
    use euclid::limit::Limit;
    use euclid::msgs::vlp::base::PoolConfig;
    use euclid::token::{Token, TokenType, TokenWithDenomAndAmount};
    use euclid::voucher::BalanceKey;
    use rstest::rstest;

    fn mode_for(factory_chain_id: &str) -> FactorySetupMode {
        if factory_chain_id == ROUTER_CHAIN_ID {
            FactorySetupMode::Native
        } else {
            FactorySetupMode::Ibc
        }
    }

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

        let token = TokenWithDenom {
            token: Token::create("eucl".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "eucl".to_string(),
            },
        };
        register_denom(
            &factory_addr,
            factory_chain_id,
            &router_addr,
            ROUTER_CHAIN_ID,
            &mut env,
            token.clone(),
        )
        .unwrap();

        let factory_chain_uid = {
            let factory_state: euclid::msgs::factory::StateResponse =
                env.chain(factory_chain_id).query(
                    &factory_addr,
                    &euclid::msgs::factory::QueryMsg::GetState {},
                );
            factory_state.chain_uid
        };

        let old_router_escrow_balance: euclid::msgs::router::TokenEscrowsResponse =
            env.chain(ROUTER_CHAIN_ID).query(
                &router_addr,
                &euclid::msgs::router::QueryMsg::QueryTokenEscrows {
                    token: token.token.clone(),
                    pagination: euclid::utils::pagination::Pagination::new(
                        Some(factory_chain_uid.clone()),
                        None,
                        None,
                        Some(1),
                    ),
                },
            );
        let old_balance = match old_router_escrow_balance.chains.first() {
            Some(chain) => chain.balance,
            None => Uint128::zero(),
        };

        let amount = Uint128::from(10_000u128);
        let recipient_one = CrossChainUser::new(
            factory_chain_uid.clone(),
            env.chain(factory_chain_id)
                .addr_make("recipient_one")
                .to_string(),
        );
        let recipient_two = CrossChainUser::new(
            factory_chain_uid.clone(),
            env.chain(factory_chain_id)
                .addr_make("recipient_two")
                .to_string(),
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
        deposit_token(
            &factory_addr,
            factory_chain_id,
            &router_addr,
            ROUTER_CHAIN_ID,
            &mut env,
            token.clone(),
            amount,
            recipients,
        )
        .unwrap();

        let synced_state = sync_state(
            factory_chain_id,
            &factory_addr,
            ROUTER_CHAIN_ID,
            &router_addr,
            &env,
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

        for (recipient, expected_amount) in expected_balances {
            let balance = synced_state
                .voucher_balance(&recipient, &token.token)
                .expect("Expected voucher balance for recipient is missing");
            assert_eq!(balance, expected_amount, "Recipient balance mismatch");
        }
    }

    #[rstest]
    #[case(FACTORY_CHAIN_ID_IBC)]
    fn add_liquidity_fails_when_slippage_exceeded(#[case] factory_chain_id: &str) {
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

        let token_a = TokenWithDenom {
            token: Token::create("tokena".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "tokena".to_string(),
            },
        };
        let token_b = TokenWithDenom {
            token: Token::create("tokenb".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "tokenb".to_string(),
            },
        };

        register_denom(
            &factory_addr,
            factory_chain_id,
            &router_addr,
            ROUTER_CHAIN_ID,
            &mut env,
            token_a.clone(),
        )
        .unwrap();
        register_denom(
            &factory_addr,
            factory_chain_id,
            &router_addr,
            ROUTER_CHAIN_ID,
            &mut env,
            token_b.clone(),
        )
        .unwrap();

        let pool_pair = PairWithDenomAndAmount {
            token_1: TokenWithDenomAndAmount {
                token: token_a.token.clone(),
                token_type: token_a.token_type.clone(),
                amount: Uint128::from(10_000u128),
            },
            token_2: TokenWithDenomAndAmount {
                token: token_b.token.clone(),
                token_type: token_b.token_type.clone(),
                amount: Uint128::from(10_000u128),
            },
        };
        create_pool(
            &factory_addr,
            factory_chain_id,
            &router_addr,
            ROUTER_CHAIN_ID,
            &mut env,
            pool_pair,
            500,
            PoolConfig::ConstantProduct {},
        )
        .unwrap();

        let skewed_pair = PairWithDenomAndAmount {
            token_1: TokenWithDenomAndAmount {
                token: token_a.token.clone(),
                token_type: token_a.token_type.clone(),
                amount: Uint128::from(1_000u128),
            },
            token_2: TokenWithDenomAndAmount {
                token: token_b.token.clone(),
                token_type: token_b.token_type.clone(),
                amount: Uint128::from(5_000u128),
            },
        };

        let result = add_liquidity(
            &factory_addr,
            factory_chain_id,
            &router_addr,
            ROUTER_CHAIN_ID,
            &mut env,
            skewed_pair,
            100,
        );

        assert!(
            result.is_err()
                || result
                    .as_ref()
                    .err()
                    .map(|e| e.to_string().contains("Slippage has been exceeded"))
                    .unwrap_or(false),
            "Expected slippage error"
        );
    }
}
