#![cfg(not(target_arch = "wasm32"))]

use crate::helpers::chains::get_virtual_balance;
use crate::helpers::factory::faucet;
use crate::helpers::relayer::relay_factory_router_factory;
use cosmwasm_std::{Uint128, Uint256};
use cw_orch::mock::MockBase;
use cw_orch::prelude::*;
use euclid::cross_chain_user::CrossChainUser;
use euclid::limit::Limit;
use euclid::msgs::cross_chain_config::CrossChainConfig;
use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
use euclid::msgs::virtual_balance::msg::QueryMsgFns as VirtualBalanceQueryMsgFns;
use euclid::normalize::{normalize_token_to_voucher, normalize_voucher_to_token};
use euclid::recipient::Recipient;
use euclid::token::{Token, TokenType, TokenWithDenom};
use euclid::voucher::BalanceKey;
use factory::FactoryContract;
use router::RouterContract;

/// Deposit native token via factory, relay IBC round-trip, credit voucher balance on hub.
pub fn deposit_native_token(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    token: TokenWithDenom,
    amount: Uint256,
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
    let tx_response = factory.execute(
        &euclid::msgs::factory::msg::ExecuteMsg::DepositToken {
            asset_in: token,
            amount_in: amount,
            recipients: vec![],
            cross_chain_config: CrossChainConfig::default(),
        },
        &funds,
    )?;
    relay_factory_router_factory(tx_response.events, factory, router, factory_chain_uid)?;
    Ok(())
}

/// Withdraw voucher balance: burn vouchers, release escrow on target chain via IBC.
pub fn withdraw_voucher(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    token: Token,
    amount: Uint256,
    recipient: Recipient,
) -> Result<(), CwOrchError> {
    let factory_chain_uid = &factory.get_state().unwrap().chain_uid;
    let tx_response = factory.execute(
        &euclid::msgs::factory::ExecuteMsg::TransferVoucher {
            token_id: token,
            amount,
            from: None,
            recipients: vec![recipient],
            cross_chain_config: CrossChainConfig::default(),
        },
        &[],
    )?;
    relay_factory_router_factory(tx_response.events, factory, router, factory_chain_uid)?;
    Ok(())
}

fn get_escrow_for_chain(
    vb: &virtual_balance::VirtualBalanceContract<MockBase>,
    token_id: &str,
    chain_uid: &euclid::chain::ChainUid,
) -> Uint256 {
    let escrows = vb.get_token_escrows(token_id.to_string(), None).unwrap();
    escrows
        .escrows
        .iter()
        .find(|c| &c.chain_uid == chain_uid)
        .map(|c| c.balance)
        .unwrap_or(Uint256::zero())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helpers::chains::setup_interchain;
    use crate::tests_reusable::constants::{FACTORY_CHAIN_ID_EVM, ROUTER_CHAIN_ID};
    use crate::tests_reusable::factory_register::FactorySetupMode;
    use crate::tests_reusable::factory_register::{setup_factory, setup_factory_evm};
    use crate::tests_reusable::factory_register_denom::register_denom;
    use crate::tests_reusable::test_macros::factory_modes;
    use cw_orch_interchain::prelude::InterchainEnv;
    use euclid::chain::ChainUid;
    use rstest::rstest;
    use rstest_reuse::apply;

    fn setup_env(
        factory_chain_id: &str,
    ) -> (
        cw_orch_interchain::mock::MockInterchainEnv,
        RouterContract<MockBase>,
        FactoryContract<MockBase>,
    ) {
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, factory_chain_id);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router =
            crate::helpers::chains::setup_router(&router_chain, vec![factory_chain_id]).unwrap();
        let factory = if factory_chain_id == FACTORY_CHAIN_ID_EVM {
            setup_factory_evm(&interchain, factory_chain_id, &router).unwrap()
        } else {
            setup_factory(&interchain, factory_chain_id, &router).unwrap()
        };
        (interchain, router, factory)
    }

    /// End-to-end: deposit tokens with various decimals, verify voucher balance is
    /// normalized to 24-dec, then withdraw and verify escrow decreases by raw amount.
    #[apply(factory_modes)]
    #[case::six_decimals("uusdc", 6)]
    #[case::eighteen_decimals("uweth", 18)]
    fn test_deposit_and_withdraw_normalization(
        mode: FactorySetupMode,
        #[case] denom: &str,
        #[case] decimals: u32,
    ) {
        let factory_chain_id = mode.chain_id();
        let (_interchain, router, factory) = setup_env(factory_chain_id);
        let chain_uid = ChainUid::create(factory_chain_id.to_string()).unwrap();

        let token_with_denom = TokenWithDenom {
            token: Token::create(denom.to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: denom.to_string(),
                decimals: Some(decimals),
            },
        };

        register_denom(&factory, &router, token_with_denom.clone()).unwrap();

        let deposit_amount = Uint256::from(1_000u128);
        deposit_native_token(&factory, &router, token_with_denom.clone(), deposit_amount).unwrap();

        // Check voucher balance is normalized (24-decimal)
        let virtual_balance_address = router.get_state().unwrap().virtual_balance_address;
        let vb = get_virtual_balance(router.environment(), &virtual_balance_address);
        let sender_user =
            CrossChainUser::new(chain_uid.clone(), factory.environment().sender.to_string());
        let voucher_balance = vb
            .get_balance(BalanceKey {
                cross_chain_user: sender_user.clone(),
                token_id: denom.to_string(),
            })
            .unwrap()
            .amount;

        let expected_voucher = normalize_token_to_voucher(deposit_amount, decimals).unwrap();
        assert_eq!(
            voucher_balance, expected_voucher,
            "Voucher balance should be deposit_amount normalized to 24 decimals"
        );

        // Check escrow balance (raw token units)
        let escrow_balance = get_escrow_for_chain(&vb, denom, &chain_uid);
        assert_eq!(
            escrow_balance, deposit_amount,
            "Escrow stores raw token amounts"
        );

        // Withdraw half via release
        let withdraw_voucher_amount = expected_voucher / Uint256::from(2u128);
        let withdraw_raw = normalize_voucher_to_token(withdraw_voucher_amount, decimals).unwrap();

        let recipient = Recipient {
            recipient: CrossChainUser::new(
                chain_uid.clone(),
                factory.environment().sender.to_string(),
            ),
            amount: Limit::LessThanOrEqual(withdraw_raw),
            denom: token_with_denom.token_type.clone(),
            forwarding_message: None,
            unsafe_refund_as_voucher: None,
        };
        withdraw_voucher(
            &factory,
            &router,
            token_with_denom.token.clone(),
            withdraw_voucher_amount,
            recipient,
        )
        .unwrap();

        // Verify voucher balance decreased by withdrawal amount
        let voucher_after = vb
            .get_balance(BalanceKey {
                cross_chain_user: sender_user,
                token_id: denom.to_string(),
            })
            .unwrap()
            .amount;
        assert_eq!(
            voucher_after,
            voucher_balance - withdraw_voucher_amount,
            "Voucher balance should decrease by withdrawn voucher amount"
        );

        // Verify escrow decreased by raw token amount
        let escrow_after = get_escrow_for_chain(&vb, denom, &chain_uid);
        assert_eq!(
            escrow_after,
            escrow_balance - withdraw_raw,
            "Escrow should decrease by raw token amount (not voucher units)"
        );
    }

    /// Dust amount (too small to normalize to any raw tokens) should not release.
    /// Native: factory.execute fails synchronously.
    /// IBC/EVM: router writes error ack, factory handles gracefully (no state change).
    #[apply(factory_modes)]
    fn test_withdraw_dust_amount_fails(mode: FactorySetupMode) {
        let factory_chain_id = mode.chain_id();
        let (_interchain, router, factory) = setup_env(factory_chain_id);
        let chain_uid = ChainUid::create(factory_chain_id.to_string()).unwrap();

        let token_with_denom = TokenWithDenom {
            token: Token::create("uusdc".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "uusdc".to_string(),
                decimals: Some(6),
            },
        };
        register_denom(&factory, &router, token_with_denom.clone()).unwrap();

        let deposit_amount = Uint256::from(1_000u128);
        deposit_native_token(&factory, &router, token_with_denom.clone(), deposit_amount).unwrap();

        let virtual_balance_address = router.get_state().unwrap().virtual_balance_address;
        let vb = get_virtual_balance(router.environment(), &virtual_balance_address);
        let sender_user =
            CrossChainUser::new(chain_uid.clone(), factory.environment().sender.to_string());
        let balance_before = vb
            .get_balance(BalanceKey {
                cross_chain_user: sender_user.clone(),
                token_id: "uusdc".to_string(),
            })
            .unwrap()
            .amount;
        let escrow_before = get_escrow_for_chain(&vb, "uusdc", &chain_uid);

        // Try to withdraw 1 voucher unit (24-dec). For 6-dec token: 1 / 10^18 = 0 raw.
        let dust_voucher_amount = Uint256::from(1u128);
        let recipient = Recipient {
            recipient: CrossChainUser::new(
                chain_uid.clone(),
                factory.environment().sender.to_string(),
            ),
            amount: Limit::LessThanOrEqual(Uint256::from(1u128)),
            denom: token_with_denom.token_type.clone(),
            forwarding_message: None,
            unsafe_refund_as_voucher: None,
        };

        // Native: returns Err. IBC/EVM: returns Ok but error ack means no state change.
        let _ = withdraw_voucher(
            &factory,
            &router,
            token_with_denom.token.clone(),
            dust_voucher_amount,
            recipient,
        );

        // Regardless of chain type, balances must be unchanged
        let balance_after = vb
            .get_balance(BalanceKey {
                cross_chain_user: sender_user,
                token_id: "uusdc".to_string(),
            })
            .unwrap()
            .amount;
        let escrow_after = get_escrow_for_chain(&vb, "uusdc", &chain_uid);

        assert_eq!(
            balance_before, balance_after,
            "Voucher balance must not change for dust withdrawal"
        );
        assert_eq!(
            escrow_before, escrow_after,
            "Escrow must not change for dust withdrawal"
        );
    }

    /// Verify end-to-end that escrow accounting uses raw token units,
    /// proving no unit mismatch (disproves review Bug #1/#2).
    #[apply(factory_modes)]
    fn test_escrow_accounting_raw_units_end_to_end(mode: FactorySetupMode) {
        let factory_chain_id = mode.chain_id();
        let (_interchain, router, factory) = setup_env(factory_chain_id);
        let chain_uid = ChainUid::create(factory_chain_id.to_string()).unwrap();

        let token_with_denom = TokenWithDenom {
            token: Token::create("uusdc".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "uusdc".to_string(),
                decimals: Some(6),
            },
        };
        register_denom(&factory, &router, token_with_denom.clone()).unwrap();

        let deposit_amount = Uint256::from(1_000u128); // 1000 raw (6-dec)
        deposit_native_token(&factory, &router, token_with_denom.clone(), deposit_amount).unwrap();

        let virtual_balance_address = router.get_state().unwrap().virtual_balance_address;
        let vb = get_virtual_balance(router.environment(), &virtual_balance_address);

        // Escrow after deposit = 1000 raw
        let escrow_after_deposit = get_escrow_for_chain(&vb, "uusdc", &chain_uid);
        assert_eq!(escrow_after_deposit, Uint256::from(1_000u128));

        // Withdraw 500 raw tokens worth of vouchers
        let withdraw_raw = Uint256::from(500u128);
        let withdraw_voucher_amount = normalize_token_to_voucher(withdraw_raw, 6).unwrap();
        // 500 * 10^18 = 500_000_000_000_000_000_000 voucher units
        assert_eq!(
            withdraw_voucher_amount,
            Uint256::from(500_000_000_000_000_000_000u128)
        );

        let recipient = Recipient {
            recipient: CrossChainUser::new(
                chain_uid.clone(),
                factory.environment().sender.to_string(),
            ),
            amount: Limit::LessThanOrEqual(withdraw_raw),
            denom: token_with_denom.token_type.clone(),
            forwarding_message: None,
            unsafe_refund_as_voucher: None,
        };

        withdraw_voucher(
            &factory,
            &router,
            token_with_denom.token.clone(),
            withdraw_voucher_amount,
            recipient,
        )
        .unwrap();

        // Escrow after withdraw = 1000 - 500 = 500 raw
        let escrow_after_withdraw = get_escrow_for_chain(&vb, "uusdc", &chain_uid);
        assert_eq!(
            escrow_after_withdraw,
            Uint256::from(500u128),
            "Escrow tracks raw units: 1000 deposited - 500 withdrawn = 500 remaining"
        );

        // Voucher balance = (1000 - 500) * 10^18
        let sender_user = CrossChainUser::new(chain_uid, factory.environment().sender.to_string());
        let voucher_after = vb
            .get_balance(BalanceKey {
                cross_chain_user: sender_user,
                token_id: "uusdc".to_string(),
            })
            .unwrap()
            .amount;
        let expected_remaining_voucher = normalize_token_to_voucher(withdraw_raw, 6).unwrap();
        assert_eq!(
            voucher_after, expected_remaining_voucher,
            "Voucher balance = remaining raw * 10^18"
        );
    }
}
