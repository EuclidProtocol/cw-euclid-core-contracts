#![cfg(not(target_arch = "wasm32"))]
use crate::helpers::factory::faucet;
use crate::helpers::relayer::relay_factory_router_factory;
use cosmwasm_std::coin;
use cosmwasm_std::to_json_binary;
use cosmwasm_std::Addr;
use cosmwasm_std::Uint128;
use cosmwasm_std::Uint256;
use cw20::{Cw20Coin, MinterResponse};
use cw_orch::mock::MockBase;
use cw_orch::prelude::ContractInstance as _;
use cw_orch::prelude::CwOrchError;
use cw_orch::prelude::CwOrchExecute;
use cw_orch::prelude::CwOrchInstantiate;
use cw_orch::prelude::CwOrchUpload;
use cw_orch::prelude::Environment;
use cw_orch_interchain::prelude::InterchainEnv;
use euclid::fee::PartnerFee;
use euclid::msgs::cross_chain_config::CrossChainConfig;
use euclid::msgs::factory::cw20::FactoryCw20HookMsg;
use euclid::msgs::factory::ExecuteMsgFns;
use euclid::msgs::factory::ExecuteSwapRequest;
use euclid::msgs::factory::QueryMsgFns as FactoryQueryMsgFns;
use euclid::msgs::lp_token::msg::InstantiateMsg as LpTokenInstantiateMsg;
use euclid::msgs::lp_token::msg::QueryMsgFns;
use euclid::msgs::vlp::base::PoolConfig;
use euclid::recipient::Recipient;
use euclid::swap::NextSwapPair;
use euclid::token::{Pair, PairWithDenomAndAmount, Token, TokenType, TokenWithDenom};
use factory::FactoryContract;
use lp_token::LpTokenContract;
use router::RouterContract;
use rstest::rstest;

pub fn swap_request(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    asset_in: TokenWithDenom,
    asset_out: Token,
    amount_in: Uint256,
    min_amount_out: Uint256,
    swaps: Vec<NextSwapPair>,
    recipients: Vec<Recipient>,
    partner_fee: Option<PartnerFee>,
) -> Result<(), CwOrchError> {
    let tx_response = if asset_in.token_type.is_smart() {
        let smart_contract = match &asset_in.token_type {
            TokenType::Smart {
                contract_address, ..
            } => contract_address.clone(),
            _ => unreachable!("asset_in.token_type already checked as smart"),
        };
        let cw20 = LpTokenContract::new(factory.environment().clone());
        cw20.set_address(&Addr::unchecked(smart_contract));
        cw20.execute(
            &euclid::msgs::lp_token::msg::ExecuteMsg::Send {
                contract: factory.address()?.to_string(),
                amount: amount_in,
                msg: to_json_binary(&FactoryCw20HookMsg::Swap {
                    recipients,
                    asset_in,
                    asset_out,
                    min_amount_out,
                    swaps,
                    partner_fee,
                    cross_chain_config: CrossChainConfig::default(),
                })?,
            },
            &[],
        )?
    } else {
        faucet(
            factory.environment(),
            factory.environment().sender.as_str(),
            Uint128::try_from(amount_in).unwrap().u128(),
            asset_in.token_type.clone(),
            &mut vec![],
        );
        factory.execute_swap_request(
            ExecuteSwapRequest {
                amount_in,
                recipients,
                asset_in: asset_in.clone(),
                asset_out,
                min_amount_out,
                swaps,
                partner_fee,
                cross_chain_config: CrossChainConfig::default(),
            },
            &vec![coin(
                Uint128::try_from(amount_in).unwrap().u128(),
                asset_in.token_type.get_denom().unwrap(),
            )],
        )?
    };

    let factory_chain_uid = &factory.get_state().unwrap().chain_uid;
    relay_factory_router_factory(tx_response.events, factory, router, factory_chain_uid)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helpers::chains::{
        get_escrow, get_lp_token, get_virtual_balance, get_vlp, setup_interchain, setup_router,
    };

    use crate::tests_reusable::factory_add_liquidity::deposit_token;
    use crate::tests_reusable::factory_create_pool::create_pool;
    use crate::tests_reusable::factory_register::{setup_factory, FactorySetupMode};
    use crate::tests_reusable::factory_register_denom::register_denom;
    use crate::tests_reusable::state_sync::sync_state;
    use euclid::chain::ChainUid;
    use euclid::cross_chain_user::CrossChainUser;
    use euclid::limit::Limit;
    use euclid::msgs::escrow::QueryMsgFns as EscrowQueryMsgFns;
    use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
    use euclid::msgs::virtual_balance::msg::QueryMsgFns as VirtualBalanceQueryMsgFns;

    use crate::tests_reusable::test_macros::decimal_pair;
    use euclid::voucher::BalanceKey;
    fn native_token(name: &str, decimals: u32) -> TokenWithDenom {
        TokenWithDenom {
            token: Token::create(name.to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: name.to_string(),
                decimals: Some(decimals),
            },
        }
    }

    fn setup_smart_denom_token(
        factory: &FactoryContract<MockBase>,
        token: Token,
        decimals: u32,
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
                decimals: decimals.try_into().unwrap(),
                initial_balances: vec![Cw20Coin {
                    address: sender.clone(),
                    amount: Uint128::from(10u128.pow(decimals) * 1_000_000_000u128),
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
                decimals: Some(decimals),
            },
        }
    }

    use rstest_reuse::apply;

    #[cfg_attr(not(feature = "full_decimals"), apply(decimal_pair))]
    #[cfg_attr(feature = "full_decimals", apply(decimal_pair_full))]
    fn test_swap_with_n_hops(
        #[values(FactorySetupMode::Native, FactorySetupMode::Ibc, FactorySetupMode::Evm)]
        mode: FactorySetupMode,
        #[values(false, true)] use_smart_asset_in: bool,
        #[values(1, 2, 3)] num_swaps: usize,
        decimals_a: u32,
        decimals_b: u32,
    ) {
        use crate::tests_reusable::constants::ROUTER_CHAIN_ID;

        let factory_chain_id = mode.chain_id();
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, factory_chain_id);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![factory_chain_id]).unwrap();
        let factory = setup_factory(&interchain, factory_chain_id, &router).unwrap();

        let chain_uid = ChainUid::create(factory_chain_id.to_string()).unwrap();

        let decimal_a_multiplier = Uint256::from(10u128).pow(decimals_a);
        let decimal_b_multiplier = Uint256::from(10u128).pow(decimals_b);
        let multipliers = [decimal_a_multiplier, decimal_b_multiplier];

        let decimals_pair = [decimals_a, decimals_b];
        let token_names: Vec<String> = (0..=num_swaps)
            .map(|i| format!("token{}", (b'a' + i as u8) as char))
            .collect();
        let tokens: Vec<TokenWithDenom> = token_names
            .iter()
            .enumerate()
            .map(|(i, n)| native_token(n, decimals_pair[i % 2]))
            .collect();

        // Register all tokens
        for token in &tokens {
            register_denom(&factory, &router, token.clone()).unwrap();
        }

        let base_deposit = Uint256::from(100_000u128);
        // Faucet sender with enough funds for deposits and the swap itself
        let mut funds = vec![];
        for (i, token) in tokens.iter().enumerate() {
            let scaled = base_deposit.checked_mul(multipliers[i % 2]).unwrap();
            faucet(
                factory.environment(),
                factory.environment().sender.as_str(),
                Uint128::try_from(scaled).unwrap().u128(),
                token.token_type.clone(),
                &mut funds,
            );
        }

        // Deposit tokens and create pools for each consecutive pair
        for (idx, window) in tokens.windows(2).enumerate() {
            let token_a = &window[0];
            let token_b = &window[1];
            let deposit_a = base_deposit.checked_mul(multipliers[idx % 2]).unwrap();
            let deposit_b = base_deposit
                .checked_mul(multipliers[(idx + 1) % 2])
                .unwrap();

            deposit_token(&factory, &router, token_a.clone(), deposit_a, vec![]).unwrap();
            deposit_token(&factory, &router, token_b.clone(), deposit_b, vec![]).unwrap();

            let pair = PairWithDenomAndAmount {
                token_1: token_a.with_amount(deposit_a),
                token_2: token_b.with_amount(deposit_b),
            };
            create_pool(&factory, &router, pair, 500, PoolConfig::ConstantProduct {}).unwrap();
        }

        // Build the swap route
        let swaps: Vec<NextSwapPair> = tokens
            .windows(2)
            .map(|w| NextSwapPair {
                token_in: w[0].token.clone(),
                token_out: w[1].token.clone(),
                pool_key: None,
                test_fail: None,
            })
            .collect();

        let asset_in_native = tokens.first().unwrap().clone();
        let asset_in = if use_smart_asset_in {
            let input_decimals = asset_in_native.token_type.get_decimals().unwrap();
            if input_decimals > 18 {
                return; // CW20 tokens only support up to 18 decimals
            }
            let smart_asset_in = setup_smart_denom_token(
                &factory,
                asset_in_native.token.clone(),
                asset_in_native.token_type.get_decimals().unwrap(),
            );
            register_denom(&factory, &router, smart_asset_in.clone()).unwrap();
            smart_asset_in
        } else {
            asset_in_native
        };
        let asset_out = tokens.last().unwrap().clone();
        let swap_amount = Uint256::from(1_000u128)
            .checked_mul(multipliers[0])
            .unwrap();
        let sender_addr = factory.environment().sender.to_string();
        let smart_cw20_contract = match &asset_in.token_type {
            TokenType::Smart {
                contract_address, ..
            } => Some(get_lp_token(
                factory.environment(),
                &Addr::unchecked(contract_address.clone()),
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

        // Record escrow balance for input token before swap
        let escrow_in = get_escrow(&factory, asset_in.token.as_str());
        let escrow_in_before = escrow_in.state().unwrap().total_amount;

        let virtual_balance_address = router.get_state().unwrap().virtual_balance_address;
        let virtual_balance_contract =
            get_virtual_balance(router.environment(), &virtual_balance_address);
        // Record router escrow balance for input token before swap
        let router_escrow_before = virtual_balance_contract
            .get_token_escrows(asset_in.token.to_string(), None)
            .unwrap();
        let router_escrow_in_before: Uint256 = router_escrow_before
            .escrows
            .iter()
            .filter(|c| c.chain_uid == chain_uid)
            .map(|c| c.balance)
            .fold(Uint256::zero(), |acc, b| acc + b);

        // Record sender's virtual balance for the output token before swap
        let virtual_balance_contract = get_virtual_balance(
            router.environment(),
            &router.get_state().unwrap().virtual_balance_address,
        );
        let sender_user =
            CrossChainUser::new(chain_uid.clone(), factory.environment().sender.to_string());
        let vb_out_before_raw = virtual_balance_contract
            .get_balance(BalanceKey {
                cross_chain_user: sender_user.clone(),
                token_id: asset_out.token.to_string(),
            })
            .unwrap()
            .amount;
        // De-normalize to match sync_state's de-normalized voucher balances
        let asset_out_metadata = virtual_balance_contract
            .get_token_metadata(asset_out.token.to_string(), None)
            .unwrap()
            .metadata;
        let vb_out_before = if let Some(metadata) = asset_out_metadata.first() {
            euclid::normalize::normalize_voucher_to_token(
                vb_out_before_raw,
                metadata.token_type.get_decimals().unwrap(),
            )
            .unwrap()
        } else {
            vb_out_before_raw
        };

        swap_request(
            &factory,
            &router,
            asset_in.clone(),
            asset_out.token.clone(),
            swap_amount,
            Uint256::from(1u128),
            swaps.clone(),
            vec![],
            None,
        )
        .unwrap();

        // --- Query post-swap state ---
        let recipients_for_sync = vec![Recipient::default_voucher_recipient(
            sender_user.clone(),
            Limit::Dynamic(Uint256::zero()),
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
            escrow_in_before + swap_amount,
            "Escrow balance for input token should increase by swap amount"
        );

        // 2. Router escrow balance for input token should have increased by swap_amount
        assert_eq!(
            escrow_state.router_escrow_balance,
            router_escrow_in_before + swap_amount,
            "Router escrow balance for input token should increase by swap amount"
        );

        // 3. Sender should have received output tokens as virtual balance (> 0 increase)
        let vb_out_after = state_sync
            .voucher_balance(&sender_user, &asset_out.token)
            .expect("Voucher balance for sender and output token should exist");
        let amount_received = vb_out_after.checked_sub(vb_out_before).unwrap();
        assert!(
            amount_received > Uint256::zero(),
            "Sender should have received output tokens, got 0"
        );

        // 4. Amount received should be less than swap amount (in normalized 24-dec units)
        let output_decimals = decimals_pair[num_swaps % 2];
        let normalized_received =
            euclid::normalize::normalize_token_to_voucher(amount_received, output_decimals)
                .unwrap();
        let normalized_input =
            euclid::normalize::normalize_token_to_voucher(swap_amount, decimals_a).unwrap();
        assert!(
            normalized_received < normalized_input,
            "Normalized output ({}) should be less than normalized input ({}) for equal-reserve pools",
            normalized_received,
            normalized_input
        );

        // 5. For smart swaps, assert CW20 movement from sender and no residual factory balance.
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
                "Sender CW20 balance should decrease by swap amount for smart-token swaps"
            );
            assert_eq!(
                factory_after, factory_before,
                "Factory should not retain smart input tokens after swap execution"
            );
        }
    }

    /// SC-23 Issue 4: run a 2-hop CP route (tokena -> tokenb -> tokenc) and return
    /// the Euclid fee collected by each hop's VLP, indexed by hop. When
    /// `override_bps` is `Some`, a per-wallet Euclid-fee override is set for the
    /// swapping wallet before the swap, so the override must apply on *every* hop
    /// (not just the first) thanks to the forwarding in `execute_swap`.
    fn two_hop_euclid_fees_per_vlp(override_bps: Option<u64>) -> Vec<Uint256> {
        use crate::tests_reusable::constants::ROUTER_CHAIN_ID;
        use cw_orch::prelude::CwOrchQuery;
        use euclid::msgs::router::execute::{
            ExecuteMsgFns as RouterExecuteMsgFns, ManageRouterState,
        };
        use euclid::msgs::vlp::cp::{QueryMsg as CpVlpQueryMsg, TotalFeesPerDenomResponse};

        let factory_chain_id = FactorySetupMode::Native.chain_id();
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, factory_chain_id);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![factory_chain_id]).unwrap();
        let factory = setup_factory(&interchain, factory_chain_id, &router).unwrap();
        let chain_uid = ChainUid::create(factory_chain_id.to_string()).unwrap();

        let decimals = 6u32;
        let multiplier = Uint256::from(10u128).pow(decimals);
        let tokens: Vec<TokenWithDenom> = ["tokena", "tokenb", "tokenc"]
            .iter()
            .map(|n| native_token(n, decimals))
            .collect();

        for token in &tokens {
            register_denom(&factory, &router, token.clone()).unwrap();
        }

        // Create a CP pool with equal reserves for each consecutive pair.
        let deposit = Uint256::from(100_000u128).checked_mul(multiplier).unwrap();
        for window in tokens.windows(2) {
            let (token_a, token_b) = (&window[0], &window[1]);
            deposit_token(&factory, &router, token_a.clone(), deposit, vec![]).unwrap();
            deposit_token(&factory, &router, token_b.clone(), deposit, vec![]).unwrap();
            let pair = PairWithDenomAndAmount {
                token_1: token_a.with_amount(deposit),
                token_2: token_b.with_amount(deposit),
            };
            create_pool(&factory, &router, pair, 500, PoolConfig::ConstantProduct {}).unwrap();
        }

        // Optionally exempt (or reduce the fee for) the swapping wallet. The
        // router's instantiator is the fee admin, which is the test sender.
        let swap_user =
            CrossChainUser::new(chain_uid.clone(), factory.environment().sender.to_string());
        if let Some(bps) = override_bps {
            router
                .manage_router_state(ManageRouterState::SetEuclidFeeOverride {
                    user: swap_user,
                    euclid_fee_bps: Some(bps),
                })
                .unwrap();
        }

        // Build and execute the 2-hop route.
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
        let swap_amount = Uint256::from(1_000u128).checked_mul(multiplier).unwrap();

        swap_request(
            &factory,
            &router,
            asset_in,
            asset_out.token.clone(),
            swap_amount,
            Uint256::from(1u128),
            swaps.clone(),
            vec![],
            None,
        )
        .unwrap();

        // Each hop's Euclid fee is recorded on that hop's VLP, keyed by the
        // hop's input denom. Collect it for every hop.
        swaps
            .iter()
            .map(|swap| {
                let pair = Pair::new(swap.token_in.clone(), swap.token_out.clone()).unwrap();
                let vlp_address = router.get_vlp(pair).unwrap().vlp;
                let vlp = get_vlp(&router_chain, &Addr::unchecked(vlp_address));
                let fees: TotalFeesPerDenomResponse = vlp
                    .query(&CpVlpQueryMsg::TotalFeesPerDenom {
                        denom: swap.token_in.to_string(),
                    })
                    .unwrap();
                fees.euclid_fees
            })
            .collect()
    }

    /// SC-23 Issue 4 acceptance: the Euclid-fee override is forwarded across every
    /// hop of a multi-hop CP route, not just the first.
    #[test]
    fn euclid_fee_override_applies_on_every_multi_hop_cp_leg() {
        // Non-whitelisted wallet: full Euclid fee charged on every hop.
        let baseline = two_hop_euclid_fees_per_vlp(None);
        assert_eq!(baseline.len(), 2, "expected a 2-hop route");
        for (hop, fee) in baseline.iter().enumerate() {
            assert!(
                !fee.is_zero(),
                "non-whitelisted hop {hop} should charge a Euclid fee, got 0"
            );
        }

        // Whitelisted wallet (full exemption): zero Euclid fee on EVERY hop.
        let exempt = two_hop_euclid_fees_per_vlp(Some(0));
        assert_eq!(exempt.len(), 2);
        for (hop, fee) in exempt.iter().enumerate() {
            assert!(
                fee.is_zero(),
                "exempt hop {hop} should charge no Euclid fee, got {fee}"
            );
        }
    }

    /// Outcome of `simulate_and_execute_cp_route` for one scenario, all amounts
    /// in voucher (24-decimal) units so simulation and execution are directly
    /// comparable bit-for-bit.
    struct SimVsExec {
        /// Router `SimulateSwap` output quoted with no sender (current behavior).
        sim_no_sender: Uint256,
        /// Router `SimulateSwap` output quoted with the swapping wallet as sender.
        sim_with_sender: Uint256,
        /// Output the swap actually delivered to the wallet's voucher balance.
        executed: Uint256,
    }

    /// SC-23 Issue 6: simulate then execute an `num_hops`-hop route (every hop
    /// using `pool_config`) for one wallet and report the quoted vs. executed
    /// output (voucher units). When `override_bps` is `Some`, the override is set
    /// for the wallet first.
    ///
    /// The Router does not normalize the `SimulateSwap` input, while execution
    /// does — so the simulation is fed a voucher-normalized amount to match what
    /// execution feeds the VLP. The executed output is read straight from the
    /// wallet's voucher balance (already in voucher units), so a matching
    /// simulation must equal it exactly.
    fn simulate_and_execute_route(
        num_hops: usize,
        override_bps: Option<u64>,
        pool_config: PoolConfig,
    ) -> SimVsExec {
        use crate::tests_reusable::constants::ROUTER_CHAIN_ID;
        use cw_orch::prelude::CwOrchQuery;
        use euclid::msgs::router::execute::{
            ExecuteMsgFns as RouterExecuteMsgFns, ManageRouterState,
        };
        use euclid::msgs::router::query::{QueryMsg as RouterQueryMsg, QuerySimulateSwap};
        use euclid::msgs::router::SimulateSwapResponse;
        use euclid::normalize::normalize_token_to_voucher;

        let factory_chain_id = FactorySetupMode::Native.chain_id();
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, factory_chain_id);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![factory_chain_id]).unwrap();
        let factory = setup_factory(&interchain, factory_chain_id, &router).unwrap();
        let chain_uid = ChainUid::create(factory_chain_id.to_string()).unwrap();

        let decimals = 6u32;
        let multiplier = Uint256::from(10u128).pow(decimals);
        let tokens: Vec<TokenWithDenom> = (0..=num_hops)
            .map(|i| native_token(&format!("token{}", (b'a' + i as u8) as char), decimals))
            .collect();

        for token in &tokens {
            register_denom(&factory, &router, token.clone()).unwrap();
        }

        let deposit = Uint256::from(100_000u128).checked_mul(multiplier).unwrap();
        for window in tokens.windows(2) {
            let (token_a, token_b) = (&window[0], &window[1]);
            deposit_token(&factory, &router, token_a.clone(), deposit, vec![]).unwrap();
            deposit_token(&factory, &router, token_b.clone(), deposit, vec![]).unwrap();
            let pair = PairWithDenomAndAmount {
                token_1: token_a.with_amount(deposit),
                token_2: token_b.with_amount(deposit),
            };
            create_pool(&factory, &router, pair, 500, pool_config.clone()).unwrap();
        }

        let swap_user =
            CrossChainUser::new(chain_uid.clone(), factory.environment().sender.to_string());
        if let Some(bps) = override_bps {
            router
                .manage_router_state(ManageRouterState::SetEuclidFeeOverride {
                    user: swap_user.clone(),
                    euclid_fee_bps: Some(bps),
                })
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
        let asset_in = tokens.first().unwrap().clone();
        let asset_out = tokens.last().unwrap().clone();
        let swap_amount = Uint256::from(1_000u128).checked_mul(multiplier).unwrap();
        // Execution normalizes amount_in to voucher units; mirror that for the
        // (non-normalizing) router simulate so both feed the VLP the same value.
        let normalized_amount_in = normalize_token_to_voucher(swap_amount, decimals).unwrap();

        let simulate = |sender: Option<CrossChainUser>| -> Uint256 {
            let res: SimulateSwapResponse = router
                .query(&RouterQueryMsg::SimulateSwap(QuerySimulateSwap {
                    asset_in: asset_in.token.clone(),
                    amount_in: normalized_amount_in,
                    asset_out: asset_out.token.clone(),
                    min_amount_out: Uint256::one(),
                    swaps: swaps.clone(),
                    sender,
                }))
                .unwrap();
            res.amount_out
        };

        let sim_no_sender = simulate(None);
        let sim_with_sender = simulate(Some(swap_user.clone()));

        // Execute the route and read the delivered output straight from the
        // wallet's voucher balance (voucher units, no de-normalization).
        let virtual_balance = get_virtual_balance(
            router.environment(),
            &router.get_state().unwrap().virtual_balance_address,
        );
        let out_key = BalanceKey {
            cross_chain_user: swap_user.clone(),
            token_id: asset_out.token.to_string(),
        };
        let out_before = virtual_balance.get_balance(out_key.clone()).unwrap().amount;

        swap_request(
            &factory,
            &router,
            asset_in,
            asset_out.token.clone(),
            swap_amount,
            Uint256::one(),
            swaps.clone(),
            vec![],
            None,
        )
        .unwrap();

        let out_after = virtual_balance.get_balance(out_key).unwrap().amount;
        let executed = out_after.checked_sub(out_before).unwrap();

        SimVsExec {
            sim_no_sender,
            sim_with_sender,
            executed,
        }
    }

    /// SC-23 Issue 6 acceptance: a sender-aware quote equals what execution
    /// charges, for both single-hop and multi-hop CP routes, whitelisted and not.
    #[test]
    fn euclid_fee_override_simulation_matches_execution() {
        let configs = [
            ("cp", PoolConfig::ConstantProduct {}),
            ("stable", PoolConfig::Stable { amp_factor: None }),
        ];
        // Single-hop for both curves; multi-hop for CP (curve-independent
        // forwarding is already proven, and stable single-hop covers the curve).
        for (label, pool_config) in configs {
            let hop_counts: &[usize] = if label == "cp" { &[1, 2] } else { &[1] };
            for &num_hops in hop_counts {
                // Non-whitelisted: a sender-aware quote equals the senderless
                // quote (no override resolved) and equals execution exactly.
                let full = simulate_and_execute_route(num_hops, None, pool_config.clone());
                assert_eq!(
                    full.sim_with_sender, full.sim_no_sender,
                    "{label} {num_hops}-hop: a non-whitelisted sender must quote the same as no sender"
                );
                assert_eq!(
                    full.sim_with_sender, full.executed,
                    "{label} {num_hops}-hop: non-whitelisted quote must equal executed output"
                );

                // Whitelisted (full exemption): the sender-aware quote reflects
                // the exemption (strictly more output than the full-fee quote)
                // and still equals execution exactly.
                let exempt = simulate_and_execute_route(num_hops, Some(0), pool_config.clone());
                assert!(
                    exempt.sim_with_sender > exempt.sim_no_sender,
                    "{label} {num_hops}-hop: exempt quote ({}) should exceed full-fee quote ({})",
                    exempt.sim_with_sender,
                    exempt.sim_no_sender
                );
                assert_eq!(
                    exempt.sim_with_sender, exempt.executed,
                    "{label} {num_hops}-hop: exempt quote must equal executed output bit-for-bit"
                );
            }
        }
    }
}
