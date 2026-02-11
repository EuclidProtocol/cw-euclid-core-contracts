#![cfg(not(target_arch = "wasm32"))]
use crate::helpers::factory::faucet;
use crate::helpers::relayer::relay_factory_router_factory;
use cosmwasm_std::Coin;
use cosmwasm_std::Uint128;
use cw_orch::mock::MockBase;
use cw_orch::prelude::CwOrchError;
use cw_orch::prelude::CwOrchExecute;
use cw_orch::prelude::Environment;
use cw_orch_interchain::mock::MockInterchainEnv;
use cw_orch_interchain::prelude::InterchainEnv;
use euclid::fee::PartnerFee;
use euclid::msgs::cross_chain_config::CrossChainConfig;
use euclid::msgs::factory::ExecuteSwapRequest;
use euclid::msgs::factory::QueryMsgFns as FactoryQueryMsgFns;
use euclid::msgs::vlp::base::PoolConfig;
use euclid::recipient::Recipient;
use euclid::swap::NextSwapPair;
use euclid::token::{Pair, PairWithDenomAndAmount, Token, TokenType, TokenWithDenom};

use factory::FactoryContract;
use router::RouterContract;
use rstest::rstest;

pub fn swap_request(
    _interchain: &MockInterchainEnv,
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    asset_in: TokenWithDenom,
    asset_out: Token,
    min_amount_out: Uint128,
    swaps: Vec<NextSwapPair>,
    recipients: Vec<Recipient>,
    partner_fee: Option<PartnerFee>,
    funds: Vec<Coin>,
) -> Result<(), CwOrchError> {
    let tx_response = factory.execute(
        &euclid::msgs::factory::ExecuteMsg::ExecuteSwapRequest(ExecuteSwapRequest {
            recipients,
            asset_in,
            asset_out,
            min_amount_out,
            swaps,
            partner_fee,
            cross_chain_config: CrossChainConfig::default(),
        }),
        &funds,
    )?;

    let factory_chain_uid = &factory.get_state().unwrap().chain_uid;
    relay_factory_router_factory(tx_response.events, factory, router, factory_chain_uid)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helpers::chains::{get_escrow, get_virtual_balance, setup_router};
    use crate::tests_reusable::state_sync::sync_state;
    use crate::tests_reusable::factory_add_liquidity::deposit_token;
    use crate::tests_reusable::factory_create_pool::create_pool;
    use crate::tests_reusable::factory_register::setup_factory;
    use crate::tests_reusable::factory_register_denom::register_denom;
    use euclid::chain::ChainUid;
    use euclid::cross_chain_user::CrossChainUser;
    use euclid::limit::Limit;
    use euclid::msgs::escrow::QueryMsgFns as EscrowQueryMsgFns;
    use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
    use euclid::msgs::virtual_balance::msg::QueryMsgFns as VirtualBalanceQueryMsgFns;
    use euclid::utils::pagination::Pagination;
    use euclid::voucher::BalanceKey;

    /// Helper to create a native TokenWithDenom from a name string.
    fn native_token(name: &str) -> TokenWithDenom {
        TokenWithDenom {
            token: Token::create(name.to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: name.to_string(),
            },
        }
    }

    #[rstest]
    #[case::single_swap(1)]
    #[case::two_hop_swap(2)]
    #[case::three_hop_swap(3)]
    fn test_swap_with_n_hops(#[case] num_swaps: usize) {
        let sender = "sender_for_all_chains";
        let factory_chain_id = "nibiru";
        let router_chain_id = "nibiru";
        let interchain = MockInterchainEnv::new(vec![(router_chain_id, sender)]);
        let router_chain = interchain.get_chain(router_chain_id).unwrap();
        let router = setup_router(&router_chain).unwrap();
        let factory =
            setup_factory(&interchain, factory_chain_id, router_chain_id, &router).unwrap();

        let chain_uid = ChainUid::create(factory_chain_id.to_string()).unwrap();

        // Create num_swaps + 1 tokens (e.g. 2 swaps needs tokens a, b, c)
        let token_names: Vec<String> = (0..=num_swaps)
            .map(|i| format!("token{}", (b'a' + i as u8) as char))
            .collect();
        let tokens: Vec<TokenWithDenom> = token_names.iter().map(|n| native_token(n)).collect();

        // Register all tokens
        for token in &tokens {
            register_denom(&factory, &router, token.clone()).unwrap();
        }

        // Faucet sender with enough funds for deposits and the swap itself
        let deposit_amount = Uint128::from(100_000u128);
        let mut funds = vec![];
        for token in &tokens {
            faucet(
                factory.environment(),
                factory.environment().sender.as_str(),
                deposit_amount.u128(),
                token.token_type.clone(),
                &mut funds,
            );
        }

        // Deposit tokens and create pools for each consecutive pair
        for window in tokens.windows(2) {
            let token_a = &window[0];
            let token_b = &window[1];

            deposit_token(&factory, &router, token_a.clone(), deposit_amount, vec![]).unwrap();
            deposit_token(&factory, &router, token_b.clone(), deposit_amount, vec![]).unwrap();

            let pair = PairWithDenomAndAmount {
                token_1: token_a.with_amount(deposit_amount),
                token_2: token_b.with_amount(deposit_amount),
            };
            create_pool(&factory, &router, pair, 500, PoolConfig::ConstantProduct {}).unwrap();
        }

        // Build the swap route
        let swaps: Vec<NextSwapPair> = tokens
            .windows(2)
            .map(|w| NextSwapPair {
                token_in: w[0].token.clone(),
                token_out: w[1].token.clone(),
                test_fail: None,
            })
            .collect();

        let asset_in = tokens.first().unwrap().clone();
        let asset_out = tokens.last().unwrap().clone();
        let swap_amount = 1_000u128;

        // Record escrow balance for input token before swap
        let escrow_in = get_escrow(&factory, asset_in.token.as_str());
        let escrow_in_before = escrow_in.state().unwrap().total_amount;

        // Record router escrow balance for input token before swap
        let router_escrow_before = router
            .query_token_escrows(
                Pagination::new(Some(chain_uid.clone()), None, None, Some(1)),
                asset_in.token.clone(),
            )
            .unwrap();
        let router_escrow_in_before = router_escrow_before
            .chains
            .first()
            .map(|c| c.balance)
            .unwrap_or(Uint128::zero());

        // Record sender's virtual balance for the output token before swap
        let virtual_balance_contract = get_virtual_balance(
            router.environment(),
            &router.get_state().unwrap().virtual_balance_address,
        );
        let sender_user =
            CrossChainUser::new(chain_uid.clone(), factory.environment().sender.to_string());
        let vb_out_before = virtual_balance_contract
            .get_balance(BalanceKey {
                cross_chain_user: sender_user.clone(),
                token_id: asset_out.token.to_string(),
            })
            .unwrap()
            .amount;

        // Faucet for the swap itself
        let mut swap_funds = vec![];
        faucet(
            factory.environment(),
            factory.environment().sender.as_str(),
            swap_amount,
            asset_in.token_type.clone(),
            &mut swap_funds,
        );

        swap_request(
            &interchain,
            &factory,
            &router,
            asset_in.clone(),
            asset_out.token.clone(),
            Uint128::new(1),
            swaps.clone(),
            vec![],
            None,
            swap_funds,
        )
        .unwrap();

        // --- Query post-swap state ---
        let recipients_for_sync = vec![Recipient::default_voucher_recipient(
            sender_user.clone(),
            Limit::Dynamic(Uint128::zero()),
        )];
        let vlp_pairs = swaps
            .clone()
            .iter()
            .map(|swap| Pair::new(swap.token_in.clone(), swap.token_out.clone()).unwrap())
            .collect();
        let state_sync = sync_state(
            &factory,
            &router,
            recipients_for_sync,
            vec![asset_out.token.clone()],
            vec![],
            vec![asset_in.token.clone()],
            chain_uid.clone(),
            vlp_pairs,
        );

        // 1. Escrow balance for input token should have increased by swap_amount
        let escrow_state = state_sync
            .escrow_balance(&chain_uid, &asset_in.token)
            .expect("Escrow state for input token should exist");
        assert_eq!(
            escrow_state.factory_escrow_balance,
            escrow_in_before + Uint128::new(swap_amount),
            "Escrow balance for input token should increase by swap amount"
        );

        // 2. Router escrow balance for input token should have increased by swap_amount
        assert_eq!(
            escrow_state.router_escrow_balance,
            router_escrow_in_before + Uint128::new(swap_amount),
            "Router escrow balance for input token should increase by swap amount"
        );

        // 3. Sender should have received output tokens as virtual balance (> 0 increase)
        let vb_out_after = state_sync
            .voucher_balance(&sender_user, &asset_out.token)
            .expect("Voucher balance for sender and output token should exist");
        let amount_received = vb_out_after - vb_out_before;
        assert!(
            amount_received > Uint128::zero(),
            "Sender should have received output tokens, got 0"
        );

        // 4. Amount received should be less than swap amount
        //    (constant product pricing on equal-reserve pools always yields less than input)
        assert!(
            amount_received < Uint128::new(swap_amount),
            "Amount received ({}) should be less than amount in ({}) for equal-reserve pools",
            amount_received,
            swap_amount
        );
    }
}
