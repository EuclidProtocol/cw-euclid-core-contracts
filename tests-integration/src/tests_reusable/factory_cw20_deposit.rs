#![cfg(not(target_arch = "wasm32"))]
//! Deposit of a registered cw20 (smart) denom via the `FactoryCw20HookMsg::Deposit`
//! cw20 `Send` hook.
//!
//! Regression coverage for the escrow `TokenAllowed` check: the hook builds the
//! deposit's `TokenType::Smart` with `decimals: None`, but smart denoms are
//! always registered with explicit decimals. The query used to compare the full
//! token type (including decimals), so every cw20-hook deposit of a registered
//! smart denom failed with `UnsupportedDenomination`. The escrow now matches
//! denoms decimals-agnostically (`TokenType::shallow_eq`), and the hub derives
//! the canonical decimals from the registered token metadata.

use crate::helpers::factory::faucet;
use crate::helpers::relayer::relay_factory_router_factory;
use cosmwasm_std::{to_json_binary, Addr, Uint128, Uint256};
use cw_orch::mock::MockBase;
use cw_orch::prelude::*;
use euclid::msgs::cross_chain_config::CrossChainConfig;
use euclid::msgs::factory::cw20::FactoryCw20HookMsg;
use euclid::msgs::factory::QueryMsgFns as FactoryQueryMsgFns;
use euclid::recipient::Recipient;
use euclid::token::{TokenType, TokenWithDenom};
use factory::FactoryContract;
use lp_token::LpTokenContract;
use router::RouterContract;

/// Mint `amount` of the smart token to the sender, deposit it through the cw20
/// `Send` hook (`FactoryCw20HookMsg::Deposit`) and relay the round-trip.
pub fn deposit_cw20_via_hook(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    token: TokenWithDenom,
    amount: Uint256,
    recipients: Vec<Recipient>,
) -> Result<(), CwOrchError> {
    let factory_chain_uid = &factory.get_state().unwrap().chain_uid;
    let contract_address = match &token.token_type {
        TokenType::Smart {
            contract_address, ..
        } => contract_address.clone(),
        _ => unreachable!("deposit_cw20_via_hook requires a smart token"),
    };
    faucet(
        factory.environment(),
        factory.environment().sender.as_str(),
        Uint128::try_from(amount).unwrap().u128(),
        token.token_type.clone(),
        &mut vec![],
    );
    let cw20 = LpTokenContract::new(factory.environment().clone());
    cw20.set_address(&Addr::unchecked(contract_address));
    let tx_response = cw20.execute(
        &euclid::msgs::lp_token::msg::ExecuteMsg::Send {
            contract: factory.address()?.to_string(),
            amount,
            msg: to_json_binary(&FactoryCw20HookMsg::Deposit {
                token: token.token.clone(),
                recipients,
                cross_chain_config: CrossChainConfig::default(),
            })?,
        },
        &[],
    )?;
    relay_factory_router_factory(tx_response.events, factory, router, factory_chain_uid)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helpers::chains::{
        get_escrow, get_lp_token, get_virtual_balance, setup_interchain, setup_router,
    };
    use crate::tests_reusable::constants::ROUTER_CHAIN_ID;
    use crate::tests_reusable::factory_register::{
        setup_factory, setup_factory_evm, FactorySetupMode,
    };
    use crate::tests_reusable::factory_register_denom::{register_denom, setup_smart_denom_token};
    use crate::tests_reusable::test_macros::factory_modes;
    use cw_orch_interchain::prelude::InterchainEnv;
    use euclid::chain::ChainUid;
    use euclid::cross_chain_user::CrossChainUser;
    use euclid::msgs::escrow::QueryMsgFns as EscrowQueryMsgFns;
    use euclid::msgs::lp_token::msg::QueryMsgFns as LpTokenQueryMsgFns;
    use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
    use euclid::msgs::virtual_balance::msg::QueryMsgFns as VirtualBalanceQueryMsgFns;
    use euclid::normalize::normalize_token_to_voucher;
    use euclid::token::Token;
    use euclid::voucher::BalanceKey;
    use rstest::rstest;
    use rstest_reuse::apply;

    /// Regression: a cw20-hook deposit of a registered smart denom must pass
    /// the escrow `TokenAllowed` check. The hook sends `decimals: None`; before
    /// the fix the escrow compared the full token type, so this never matched
    /// the registered denom (registered with explicit decimals) and the deposit
    /// failed synchronously with `UnsupportedDenomination`. The escrow now
    /// matches decimals-agnostically.
    // Cross-VM coverage: testing/euclid-tests/tests/protocol/deposit.rs::deposit_succeeds_and_accumulates
    #[apply(factory_modes)]
    #[case::six_decimals(6)]
    #[case::eighteen_decimals(18)]
    fn test_cw20_hook_deposit_of_registered_smart_denom(
        mode: FactorySetupMode,
        #[case] decimals: u32,
    ) {
        let factory_chain_id = mode.chain_id();
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, factory_chain_id);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![factory_chain_id]).unwrap();
        let factory = match mode {
            FactorySetupMode::Native | FactorySetupMode::Ibc => {
                setup_factory(&interchain, factory_chain_id, &router).unwrap()
            }
            FactorySetupMode::Evm => {
                setup_factory_evm(&interchain, factory_chain_id, &router).unwrap()
            }
        };
        let chain_uid = ChainUid::create(factory_chain_id.to_string()).unwrap();

        // Smart denoms are always registered with explicit decimals.
        let token = setup_smart_denom_token(
            &factory.environment(),
            Token::create("eucl".to_string()).unwrap(),
            decimals,
        );
        register_denom(&factory, &router, token.clone()).unwrap();

        // --- Pre-deposit state ---
        let escrow_contract = get_escrow(&factory, token.token.as_str());
        let escrow_before = escrow_contract.state().unwrap().total_amount;

        let virtual_balance_address = router.get_state().unwrap().virtual_balance_address;
        let vb = get_virtual_balance(router.environment(), &virtual_balance_address);
        let router_escrow_before = vb
            .get_token_escrows(token.token.to_string(), None)
            .unwrap()
            .escrows
            .iter()
            .find(|c| c.chain_uid == chain_uid)
            .map(|c| c.balance)
            .unwrap_or(Uint256::zero());

        let sender_addr = factory.environment().sender.to_string();
        let sender_user = CrossChainUser::new(chain_uid, sender_addr);
        let voucher_before = vb
            .get_balance(BalanceKey {
                cross_chain_user: sender_user.clone(),
                token_id: token.token.to_string(),
            })
            .unwrap()
            .amount;

        let cw20_address = match &token.token_type {
            TokenType::Smart {
                contract_address, ..
            } => Addr::unchecked(contract_address.clone()),
            _ => unreachable!("setup_smart_denom_token returns a smart token"),
        };
        let cw20 = get_lp_token(factory.environment(), &cw20_address);
        let escrow_cw20_before = cw20
            .balance(escrow_contract.address().unwrap().to_string())
            .unwrap()
            .balance;
        let factory_cw20_before = cw20
            .balance(factory.address().unwrap().to_string())
            .unwrap()
            .balance;

        let amount = Uint256::from(1_000u128)
            .checked_mul(Uint256::from(10u128).pow(decimals))
            .unwrap();

        // Before the fix this failed on the factory chain with
        // `UnsupportedDenomination` (escrow TokenAllowed mismatch).
        deposit_cw20_via_hook(&factory, &router, token.clone(), amount, vec![]).unwrap();

        // --- Post-deposit assertions ---

        // Factory-side escrow accounting increased by the raw amount.
        let escrow_after = escrow_contract.state().unwrap().total_amount;
        assert_eq!(
            escrow_after,
            escrow_before + amount,
            "Escrow accounting should increase by the deposited amount"
        );

        // The escrow physically holds the deposited cw20s; the factory retains none.
        let escrow_cw20_after = cw20
            .balance(escrow_contract.address().unwrap().to_string())
            .unwrap()
            .balance;
        assert_eq!(
            escrow_cw20_after,
            escrow_cw20_before + Uint128::try_from(amount).unwrap(),
            "Escrow contract should hold the deposited cw20 tokens"
        );
        let factory_cw20_after = cw20
            .balance(factory.address().unwrap().to_string())
            .unwrap()
            .balance;
        assert_eq!(
            factory_cw20_after, factory_cw20_before,
            "Factory should not retain deposited cw20 tokens"
        );

        // Router-side escrow tracking increased by the raw amount.
        let router_escrow_after = vb
            .get_token_escrows(token.token.to_string(), None)
            .unwrap()
            .escrows
            .iter()
            .find(|c| c.chain_uid == sender_user.chain_uid)
            .map(|c| c.balance)
            .unwrap_or(Uint256::zero());
        assert_eq!(
            router_escrow_after,
            router_escrow_before + amount,
            "Router escrow balance should increase by the deposited amount"
        );

        // The sender's voucher balance is the deposit normalized with the
        // registered denom's decimals — the hub derives them from the token
        // metadata, so the deposit's `decimals: None` does not affect it.
        let voucher_after = vb
            .get_balance(BalanceKey {
                cross_chain_user: sender_user,
                token_id: token.token.to_string(),
            })
            .unwrap()
            .amount;
        assert_eq!(
            voucher_after,
            voucher_before + normalize_token_to_voucher(amount, decimals).unwrap(),
            "Sender voucher balance should be the deposit normalized to 24 decimals"
        );
    }
}
