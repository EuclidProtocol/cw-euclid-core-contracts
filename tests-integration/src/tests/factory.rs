#![cfg(not(target_arch = "wasm32"))]
use cosmwasm_std::{coin, Addr, Coin, IbcTimeout, Timestamp, Uint128, Uint64};
use cw_orch::{
    core::CwEnvError,
    mock::MockBase,
    prelude::{CallAs, ContractInstance, CwOrchExecute, CwOrchQuery, Environment},
};
use cw_orch_interchain::core::InterchainEnv;
use cw_orch_interchain::prelude::*;
use escrow::mock::mock_escrow;
use euclid::{
    chain::{Chain, ChainType, ChainUid, CrossChainUser, CrossChainUserWithLimit, IbcChain},
    error::ContractError,
    fee::{DenomFees, PartnerFee, BPS_100_PERCENT, BPS_1_PERCENT, MAX_PARTNER_FEE_BPS},
    liquidity::AddLiquidityRequest,
    msgs::{
        escrow::{
            ExecuteMsgFns as EscrowExecuteMsgFns, QueryMsgFns as EscrowQueryMsgFns,
            StateResponse as EscrowStateResponse,
        },
        factory::{
            AllPoolsResponse, ExecuteSwapRequest, GetPendingLiquidityResponse,
            GetPendingSwapsResponse, PartnerFeesCollectedResponse, PoolVlpResponse,
            QueryMsgFns as FactoryQueryMsgFns, StateResponse,
        },
        router::{
            AllChainResponse, AllEscrowsResponse, AllTokensResponse, AllVlpResponse, ChainResponse,
            EscrowResponse, QueryMsgFns as RouterQueryMsgFns, QuerySimulateSwap,
            RelayerAddressesResponse, SimulateEscrowReleaseResponse, SimulateSwapResponse,
            TokenDenom, TokenDenomsResponse, TokenEscrowChainResponse, TokenEscrowsResponse,
            VlpResponse,
        },
        virtual_balance::QueryMsgFns as VirtualBalanceQueryMsgFns,
        vlp::GetLiquidityResponse,
    },
    pool::PoolConfig,
    swap::{NextSwapPair, SwapRequest},
    token::{
        Pair, PairWithDenomAndAmount, Token, TokenType, TokenWithDenom, TokenWithDenomAndAmount,
    },
    utils::pagination::Pagination,
    virtual_balance::BalanceKey,
};
use factory::{
    mock::{mock_factory, MockFactory},
    FactoryContract,
};
use mock::{mock::mock_app, mock_builder::MockEuclidBuilder};
use router::RouterContract;
use std::collections::HashMap;

use crate::helpers::{
    chains::{get_escrow, get_virtual_balance, get_vlp, setup_factory, setup_router},
    factory::{add_liquidity, create_pool, deposit_token, faucet, register_token, swap_request},
    relayer::{
        relay_factory_router_factory, relay_factory_send_packet, relay_router_factory_router,
    },
};
use rstest::*;

#[test]
fn test_proper_instantiation() {
    let mut factory = mock_app(None);
    let andr = MockEuclidBuilder::new(&mut factory, "admin")
        .with_wallets(vec![
            ("owner", vec![coin(1000, "eucl")]),
            ("recipient1", vec![]),
            ("recipient2", vec![]),
        ])
        .with_contracts(vec![("escrow", mock_escrow()), ("factory", mock_factory())])
        .build(&mut factory);
    let owner = andr.get_wallet("owner");

    let escrow_code_id = 1;
    let factory_code_id = 2;
    let cw20_code_id = 3;
    let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
    let router_contract = "router_contract".to_string();

    let mock_factory = MockFactory::instantiate(
        &mut factory,
        factory_code_id,
        owner.clone(),
        router_contract.clone(),
        chain_uid.clone(),
        escrow_code_id,
        cw20_code_id,
        true,
    );

    let state_response = MockFactory::query_state(&mock_factory, &factory);
    let expected_state_id = StateResponse {
        chain_uid,
        router_contract,
        hub_channel: None,
        admin: owner.clone().into_string(),
        is_native: true,
        cw20_code_id,
        escrow_code_id,
        partner_fees_collected: DenomFees {
            totals: HashMap::new(),
        },
    };
    assert_eq!(state_response, expected_state_id);
}

#[rstest]
#[case("nibiru", "osmosis")]
#[case("nibiru", "nibiru")]
fn test_create_pool_with_funds(#[case] router_chain_id: &str, #[case] factory_chain_id: &str) {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let mut chains = vec![(router_chain_id, sender.as_str())];
    if router_chain_id != factory_chain_id {
        chains.push((factory_chain_id, sender.as_str()));
    }
    let interchain = MockInterchainEnv::new(chains);
    let router = interchain.get_chain(router_chain_id).unwrap();
    let factory = interchain.get_chain(factory_chain_id).unwrap();

    let token_a_id: String = format!("token.a.{}", router_chain_id);
    let token_b_id: String = format!("token.b.{}", router_chain_id);
    println!("token_a: {:?}", token_a_id);
    println!("token_b: {:?}", token_b_id);

    let sender = factory.addr_make("sender_for_all_chains");

    router
        .set_balance(
            &Addr::unchecked(sender.clone()),
            vec![
                Coin::new(100000000000000u128, token_a_id.clone()),
                Coin::new(100000000000000u128, token_b_id.clone()),
            ],
        )
        .unwrap();

    factory
        .set_balance(
            &Addr::unchecked(sender.clone()),
            vec![
                Coin::new(100000000000000u128, token_b_id.clone()),
                Coin::new(100000000000000u128, token_a_id.clone()),
            ],
        )
        .unwrap();

    let router_contract = setup_router(&router).unwrap();
    let router_state = router_contract.get_state().unwrap();

    let _virtual_balance_router =
        get_virtual_balance(&router, &router_state.virtual_balance_address.unwrap());

    let factory_chain_uid = ChainUid::create(factory_chain_id.to_string()).unwrap();
    let factory_contract = setup_factory(
        &interchain,
        factory_chain_id,
        router_chain_id,
        &router_contract,
    )
    .unwrap();

    let token_a = TokenWithDenom {
        token: Token::create(token_a_id.clone()).unwrap(),
        token_type: euclid::token::TokenType::Native {
            denom: token_a_id.clone(),
        },
    };
    let token_b = TokenWithDenom {
        token: Token::create(token_b_id.clone()).unwrap(),
        token_type: euclid::token::TokenType::Native { denom: token_b_id },
    };

    // Register escrow
    let register_escrow_request = factory_contract
        .execute(
            &euclid::msgs::factory::ExecuteMsg::RequestRegisterDenom {
                token: token_a.clone(),
                timeout: None,
            },
            &[],
        )
        .unwrap();

    relay_factory_router_factory(
        register_escrow_request.events,
        &factory_contract,
        &router_contract,
        &factory_chain_uid,
    )
    .unwrap();

    let token_denoms_response: TokenDenomsResponse = router_contract
        .query(&euclid::msgs::router::QueryMsg::QueryTokenDenoms {
            token: token_a.token.clone(),
        })
        .unwrap();

    assert_eq!(
        token_denoms_response,
        TokenDenomsResponse {
            denoms: vec![TokenDenom {
                chain_uid: factory_chain_uid.clone(),
                token_type: euclid::token::TokenType::Native {
                    denom: token_a.token.to_string(),
                },
            }],
        }
    );

    // Test Create pool without funds
    let create_pool_with_funds_request = factory_contract.execute(
        &euclid::msgs::factory::ExecuteMsg::RequestPoolCreation {
            pair: PairWithDenomAndAmount {
                token_1: token_a.with_amount(Uint128::from(0u128)),
                token_2: token_b.with_amount(Uint128::from(0u128)),
            },
            slippage_tolerance_bps: 100,
            timeout: None,
            lp_token_name: "lp".to_string(),
            lp_token_symbol: "lp".to_string(),
            lp_token_decimal: 6,
            lp_token_marketing: None,
            pool_config: PoolConfig::ConstantProduct {},
        },
        &[],
    );
    assert_eq!(
        ContractError::new("Amount cannot be zero"),
        create_pool_with_funds_request
            .unwrap_err()
            .downcast()
            .unwrap()
    );

    // Need to request register escrow first
    let create_pool_with_funds_request = factory_contract
        .execute(
            &euclid::msgs::factory::ExecuteMsg::RequestPoolCreation {
                pair: PairWithDenomAndAmount {
                    token_1: token_a.with_amount(Uint128::from(10_000u128)),
                    token_2: token_b.with_amount(Uint128::from(100_000u128)),
                },
                slippage_tolerance_bps: 100,
                timeout: None,
                lp_token_name: "lpname".to_string(),
                lp_token_symbol: "lpsymbol".to_string(),
                lp_token_decimal: 6,
                lp_token_marketing: None,
                pool_config: PoolConfig::ConstantProduct {},
            },
            &[
                coin(10_000u128, token_a.token.to_string()),
                coin(100_000u128, token_b.token.to_string()),
            ],
        )
        .unwrap();

    relay_factory_router_factory(
        create_pool_with_funds_request.events,
        &factory_contract,
        &router_contract,
        &factory_chain_uid,
    )
    .unwrap();

    let all_pools_query: AllPoolsResponse = factory_contract
        .query(&euclid::msgs::factory::QueryMsg::GetAllPools {})
        .unwrap();

    for pool in all_pools_query.pools {
        assert_eq!(
            pool.pair,
            Pair::new(token_a.token.clone(), token_b.token.clone()).unwrap()
        );
    }

    let vlp_query: VlpResponse = router_contract
        .query(&euclid::msgs::router::QueryMsg::GetVlp {
            pair: Pair::new(token_a.token.clone(), token_b.token.clone()).unwrap(),
        })
        .unwrap();
    assert_eq!(vlp_query.token_1, token_a.token.clone());
    assert_eq!(vlp_query.token_2, token_b.token.clone());

    let vlp_contract = get_vlp(&router, &Addr::unchecked(vlp_query.vlp));

    let liquidity_query: GetLiquidityResponse = vlp_contract
        .query(&euclid::msgs::vlp::QueryMsg::Liquidity {})
        .unwrap();
    assert_eq!(
        liquidity_query,
        GetLiquidityResponse {
            pair: Pair {
                token_1: token_a.token.clone(),
                token_2: token_b.token.clone(),
            },
            token_1_reserve: Uint128::new(10_000),
            token_2_reserve: Uint128::new(100_000),
            total_lp_tokens: Uint128::new(31622),
        }
    );

    // Osmo escrow contract
    let escrow_token_a = get_escrow(&factory_contract, token_a.token.to_string().as_str());
    let escrow_token_b = get_escrow(&factory_contract, token_b.token.to_string().as_str());

    let escrow_query: EscrowStateResponse = escrow_token_a
        .query(&euclid::msgs::escrow::QueryMsg::State {})
        .unwrap();

    assert_eq!(
        escrow_query,
        EscrowStateResponse {
            token: token_a.token.clone(),
            factory_address: factory_contract.address().unwrap(),
            total_amount: Uint128::from(10_000u128),
        }
    );

    // This is the escrow for the Euclid token
    let escrow_query: EscrowStateResponse = escrow_token_b
        .query(&euclid::msgs::escrow::QueryMsg::State {})
        .unwrap();
    assert_eq!(
        escrow_query,
        EscrowStateResponse {
            token: token_b.token.clone(),
            factory_address: factory_contract.address().unwrap(),
            total_amount: Uint128::from(100_000u128),
        }
    );

    // Add Liquidity
    // Need to request register escrow first
    let add_liquidity_request = factory_contract
        .execute(
            &euclid::msgs::factory::ExecuteMsg::AddLiquidityRequest {
                pair_info: PairWithDenomAndAmount {
                    token_1: token_a.with_amount(Uint128::from(10_000u128)),
                    token_2: token_b.with_amount(Uint128::from(100_000u128)),
                },
                slippage_tolerance_bps: 100, // 1% slippage tolerance
                timeout: None,               // 10 minutes in seconds
            },
            &[
                coin(10_000u128, token_a.token.to_string()),
                coin(100_000u128, token_b.token.to_string()),
            ],
        )
        .unwrap();
    if router_chain_id != factory_chain_id {
        // Query pending liquidity
        let pending_liquidity_query: GetPendingLiquidityResponse = factory_contract
            .query(&euclid::msgs::factory::QueryMsg::PendingLiquidity {
                user: sender.clone(),
                pagination: Pagination::new(None, None, None, None),
            })
            .unwrap();

        let expected_pending_liquidity = GetPendingLiquidityResponse {
            pending_add_liquidity: vec![AddLiquidityRequest {
                sender: sender.to_string(),
                tx_id: format!(
                    "{}:{}:{}:12345:0:3",
                    factory_chain_id, sender, factory_chain_id
                ),
                pair_info: PairWithDenomAndAmount {
                    token_1: token_a.with_amount(Uint128::from(10_000u128)),
                    token_2: token_b.with_amount(Uint128::from(100_000u128)),
                },
            }],
        };
        assert_eq!(pending_liquidity_query, expected_pending_liquidity);
    }

    relay_factory_router_factory(
        add_liquidity_request.events,
        &factory_contract,
        &router_contract,
        &factory_chain_uid,
    )
    .unwrap();

    let liquidity_query: GetLiquidityResponse = vlp_contract
        .query(&euclid::msgs::vlp::QueryMsg::Liquidity {})
        .unwrap();
    assert_eq!(
        liquidity_query,
        GetLiquidityResponse {
            pair: Pair {
                token_1: token_a.token.clone(),
                token_2: token_b.token.clone(),
            },
            token_1_reserve: Uint128::new(10_000u128 * 2),
            token_2_reserve: Uint128::new(100_000u128 * 2),
            total_lp_tokens: Uint128::new(31622u128 * 2),
        }
    );
    // Euclid escrow contract
    let escrow_query: EscrowStateResponse = escrow_token_a
        .query(&euclid::msgs::escrow::QueryMsg::State {})
        .unwrap();
    assert_eq!(
        escrow_query,
        EscrowStateResponse {
            token: token_a.token.clone(),
            factory_address: factory_contract.address().unwrap(),
            total_amount: Uint128::from(10_000u128 * 2),
        }
    );
    // Osmo escrow contract
    let escrow_query: EscrowStateResponse = escrow_token_b
        .query(&euclid::msgs::escrow::QueryMsg::State {})
        .unwrap();
    assert_eq!(
        escrow_query,
        EscrowStateResponse {
            token: token_b.token.clone(),
            factory_address: factory_contract.address().unwrap(),
            total_amount: Uint128::from(100_000u128 * 2),
        }
    );

    let resp: AllEscrowsResponse = router_contract
        .query(&euclid::msgs::router::QueryMsg::QueryAllEscrows {
            pagination: Pagination::new(None, None, None, None),
        })
        .unwrap();

    let expected_escrows = AllEscrowsResponse {
        escrows: vec![
            EscrowResponse {
                token: token_a.token.clone(),
                chain_uid: factory_chain_uid.clone(),
                balance: Uint128::from(10_000u128 * 2),
            },
            EscrowResponse {
                token: token_b.token.clone(),
                chain_uid: factory_chain_uid.clone(),
                balance: Uint128::from(100_000u128 * 2),
            },
        ],
    };
    assert_eq!(resp, expected_escrows);

    let resp: AllVlpResponse = router_contract
        .query(&euclid::msgs::router::QueryMsg::GetAllVlps {
            pagination: Pagination::new(None, None, None, None),
        })
        .unwrap();

    let expected_vlps = AllVlpResponse {
        vlps: vec![VlpResponse {
            vlp: vlp_contract.address().unwrap().to_string(),
            token_1: token_a.token.clone(),
            token_2: token_b.token.clone(),
        }],
    };
    assert_eq!(resp, expected_vlps);

    let chain_response: ChainResponse = router_contract
        .query(&euclid::msgs::router::QueryMsg::GetChain {
            chain_uid: factory_chain_uid.clone(),
        })
        .unwrap();
    let chain_type = if factory_chain_id == router_chain_id {
        ChainType::Native {}
    } else {
        ChainType::Ibc(IbcChain {
            from_hub_channel: "".to_string(),
            from_factory_channel: "".to_string(),
        })
    };
    assert_eq!(
        chain_response,
        ChainResponse {
            chain: Chain {
                factory_chain_id: factory_chain_id.to_string(),
                factory: factory_contract.address().unwrap().to_string(),
                chain_type,
            },
            chain_uid: factory_chain_uid.clone(),
        }
    );

    let all_chains_response: AllChainResponse = router_contract
        .query(&euclid::msgs::router::QueryMsg::GetAllChains {})
        .unwrap();
    assert_eq!(
        all_chains_response,
        AllChainResponse {
            chains: vec![chain_response]
        }
    );

    let simulate_swap_response: SimulateSwapResponse = router_contract
        .query(&euclid::msgs::router::QueryMsg::SimulateSwap(
            QuerySimulateSwap {
                asset_in: token_a.token.clone(),
                amount_in: Uint128::from(10_000u128),
                asset_out: token_b.token.clone(),
                min_amount_out: Uint128::from(10_000u128),
                swaps: vec![NextSwapPair {
                    token_in: token_a.token.clone(),
                    token_out: token_b.token.clone(),
                    test_fail: None,
                }],
            },
        ))
        .unwrap();
    let expected_simulate_swap_response = SimulateSwapResponse {
        amount_out: Uint128::from(66_578u128),
        asset_out: token_b.token.clone(),
    };
    assert_eq!(simulate_swap_response, expected_simulate_swap_response);

    let simulate_release_escrow_response: SimulateEscrowReleaseResponse = router_contract
        .query(&euclid::msgs::router::QueryMsg::SimulateReleaseEscrow {
            token: token_a.token.clone(),
            amount: Uint128::from(10_000u128),
            cross_chain_addresses: vec![CrossChainUserWithLimit {
                user: CrossChainUser::new(factory_chain_uid.clone(), sender.to_string()),
                limit: None,
                preferred_token_type: None,
                refund_address: None,
                unsafe_refund_voucher_to_recipient: None,
                forwarding_message: None,
                voucher_msg: None,
            }],
        })
        .unwrap();

    let expected_simulate_release_escrow_response = SimulateEscrowReleaseResponse {
        remaining_amount: Uint128::zero(),
        release_amounts: vec![(
            Uint128::from(10_000u128),
            CrossChainUserWithLimit {
                user: CrossChainUser::new(factory_chain_uid.clone(), sender.to_string()),
                limit: None,
                preferred_token_type: None,
                refund_address: None,
                unsafe_refund_voucher_to_recipient: None,
                forwarding_message: None,
                voucher_msg: None,
            },
        )],
    };
    assert_eq!(
        simulate_release_escrow_response,
        expected_simulate_release_escrow_response
    );

    let token_escrows_response: TokenEscrowsResponse = router_contract
        .query(&euclid::msgs::router::QueryMsg::QueryTokenEscrows {
            token: token_a.token.clone(),
            pagination: Pagination::new(None, None, None, None),
        })
        .unwrap();
    let expected_token_escrows_response = TokenEscrowsResponse {
        chains: vec![TokenEscrowChainResponse {
            chain_uid: factory_chain_uid.clone(),
            balance: Uint128::from(10_000u128 * 2),
        }],
    };
    assert_eq!(token_escrows_response, expected_token_escrows_response);

    let all_tokens_response: AllTokensResponse = router_contract
        .query(&euclid::msgs::router::QueryMsg::QueryAllTokens {
            pagination: Pagination::new(None, None, None, None),
        })
        .unwrap();
    let expected_all_tokens_response = AllTokensResponse {
        tokens: vec![token_a.token.clone(), token_b.token.clone()],
    };
    assert_eq!(all_tokens_response, expected_all_tokens_response);

    let relayer_addresses_response: RelayerAddressesResponse = router_contract
        .query(&euclid::msgs::router::QueryMsg::QueryRelayerAddresses {})
        .unwrap();
    let expected_relayer_addresses_response = RelayerAddressesResponse {
        relayer_addresses: vec![
            "cosmwasm1mzdhwvvh22wrt07w59wxyd58822qavwkx5lcej7aqfkpqqlhaqfsgn6fq2".to_string(),
        ],
    };
    assert_eq!(
        relayer_addresses_response,
        expected_relayer_addresses_response
    );
}

#[rstest]
#[case("osmosis", "nibiru")]
#[case("nibiru", "nibiru")]
fn test_add_liquidity(#[case] factory_chain_id: &str, #[case] router_chain_id: &str) {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let mut chains = vec![(factory_chain_id, sender.as_str())];
    if factory_chain_id != router_chain_id {
        chains.push((router_chain_id, sender.as_str()));
    }
    let interchain = MockInterchainEnv::new(chains);
    let factory_chain = interchain.get_chain(factory_chain_id).unwrap();
    let router_chain = interchain.get_chain(router_chain_id).unwrap();

    let token_a_id: String = "token.a".to_string();
    let token_a = TokenWithDenom {
        token: Token::create(token_a_id.clone()).unwrap(),
        token_type: euclid::token::TokenType::Native {
            denom: token_a_id.clone(),
        },
    };
    let token_b_id: String = "token.b".to_string();
    let token_b = TokenWithDenom {
        token: Token::create(token_b_id.clone()).unwrap(),
        token_type: euclid::token::TokenType::Native {
            denom: token_b_id.clone(),
        },
    };

    let sender = factory_chain.addr_make("sender_for_all_chains");
    println!("the sender is: {:?}", sender);
    factory_chain
        .set_balance(
            &sender.clone(),
            vec![
                Coin::new(100000000000000u128, token_a_id.clone()),
                Coin::new(100000000000000u128, token_b_id.clone()),
            ],
        )
        .unwrap();

    router_chain
        .set_balance(
            &sender.clone(),
            vec![
                Coin::new(100000000000000u128, token_a_id.clone()),
                Coin::new(100000000000000u128, token_b_id.clone()),
            ],
        )
        .unwrap();

    let router_contract = setup_router(&router_chain).unwrap();
    let _router_state = router_contract.get_state().unwrap();

    let factory_chain_uid = ChainUid::create(factory_chain_id.to_string()).unwrap();
    let router_chain_uid = ChainUid::create(router_chain_id.to_string()).unwrap();
    let factory_contract = setup_factory(
        &interchain,
        factory_chain_id,
        router_chain_id,
        &router_contract,
    )
    .unwrap();

    // // Register escrow
    let register_escrow_request = factory_contract
        .execute(
            &euclid::msgs::factory::ExecuteMsg::RequestRegisterDenom {
                token: token_a.clone(),
                timeout: None,
            },
            &[],
        )
        .unwrap();

    relay_factory_router_factory(
        register_escrow_request.events,
        &factory_contract,
        &router_contract,
        &factory_chain_uid,
    )
    .unwrap();

    let token_denoms_response: TokenDenomsResponse = router_contract
        .query(&euclid::msgs::router::QueryMsg::QueryTokenDenoms {
            token: token_a.token.clone(),
        })
        .unwrap();

    assert_eq!(
        token_denoms_response,
        TokenDenomsResponse {
            denoms: vec![TokenDenom {
                chain_uid: factory_chain_uid.clone(),
                token_type: token_a.token_type.clone(),
            }],
        }
    );

    // create pool with funds
    let create_pool_with_funds_request = factory_contract
        .execute(
            &euclid::msgs::factory::ExecuteMsg::RequestPoolCreation {
                pair: PairWithDenomAndAmount {
                    token_1: token_a.with_amount(Uint128::from(10_000u128)),
                    token_2: token_b.with_amount(Uint128::from(100_000u128)),
                },
                slippage_tolerance_bps: 100,
                timeout: None,
                lp_token_name: "lpname".to_string(),
                lp_token_symbol: "lpsymbol".to_string(),
                lp_token_decimal: 6,
                lp_token_marketing: None,
                pool_config: PoolConfig::ConstantProduct {},
            },
            &[
                coin(100_000u128, token_b.token.to_string()),
                coin(10_000u128, token_a.token.to_string()),
            ],
        )
        .unwrap();

    relay_factory_router_factory(
        create_pool_with_funds_request.events,
        &factory_contract,
        &router_contract,
        &factory_chain_uid,
    )
    .unwrap();

    let all_pools_query: AllPoolsResponse = factory_contract
        .query(&euclid::msgs::factory::QueryMsg::GetAllPools {})
        .unwrap();

    for pool in all_pools_query.pools {
        assert_eq!(
            pool.pair,
            Pair::new(token_a.token.clone(), token_b.token.clone()).unwrap()
        );
    }

    let vlp_query: VlpResponse = router_contract
        .query(&euclid::msgs::router::QueryMsg::GetVlp {
            pair: Pair::new(token_a.token.clone(), token_b.token.clone()).unwrap(),
        })
        .unwrap();
    assert_eq!(vlp_query.token_1, token_a.token.clone());
    assert_eq!(vlp_query.token_2, token_b.token.clone());

    let vlp_contract = get_vlp(&router_chain, &Addr::unchecked(vlp_query.vlp));

    let liquidity_query: GetLiquidityResponse = vlp_contract
        .query(&euclid::msgs::vlp::QueryMsg::Liquidity {})
        .unwrap();
    assert_eq!(
        liquidity_query,
        GetLiquidityResponse {
            pair: Pair {
                token_1: token_a.token.clone(),
                token_2: token_b.token.clone(),
            },
            token_1_reserve: Uint128::new(10_000),
            token_2_reserve: Uint128::new(100_000),
            total_lp_tokens: Uint128::new(31622),
        }
    );

    // Osmo escrow contract
    let escrow_token_a = get_escrow(&factory_contract, token_a.token.as_str());
    let escrow_token_b = get_escrow(&factory_contract, token_b.token.as_str());

    // This is the escrow for the Euclid token
    let escrow_query: EscrowStateResponse = escrow_token_a
        .query(&euclid::msgs::escrow::QueryMsg::State {})
        .unwrap();
    assert_eq!(
        escrow_query,
        EscrowStateResponse {
            token: token_a.token.clone(),
            factory_address: factory_contract.address().unwrap(),
            total_amount: Uint128::from(10_000u128),
        }
    );

    let escrow_query: EscrowStateResponse = escrow_token_b
        .query(&euclid::msgs::escrow::QueryMsg::State {})
        .unwrap();
    assert_eq!(
        escrow_query,
        EscrowStateResponse {
            token: token_b.token.clone(),
            factory_address: factory_contract.address().unwrap(),
            total_amount: Uint128::from(100_000u128),
        }
    );

    // Try deregistering a chain when two chains are involved.
    if router_chain_id != factory_chain_id {
        router_contract
            .execute(
                &euclid::msgs::router::ExecuteMsg::DeregisterChain {
                    chain: ChainUid::create("osmosis".to_string()).unwrap(),
                },
                &[],
            )
            .unwrap();

        let add_liquidity_request = factory_contract
            .execute(
                &euclid::msgs::factory::ExecuteMsg::AddLiquidityRequest {
                    pair_info: PairWithDenomAndAmount {
                        token_1: token_a.with_amount(Uint128::from(10_000u128)),
                        token_2: token_b.with_amount(Uint128::from(100_000u128)),
                    },
                    slippage_tolerance_bps: 100, // 1% slippage tolerance
                    timeout: None,               // 10 minutes in seconds
                },
                &[
                    coin(100_000u128, token_b.token.to_string()),
                    coin(10_000u128, token_a.token.to_string()),
                ],
            )
            .unwrap();
        let res = relay_factory_router_factory(
            add_liquidity_request.events,
            &factory_contract,
            &router_contract,
            &factory_chain_uid,
        )
        .unwrap();
        let wasm_event = res.iter().find(|event| {
            event.ty == "wasm"
                && event.attributes.iter().any(|attr| {
                    attr.key == "reply_on_cosmos_receive_processing" && attr.value == "error"
                })
        });
        assert!(wasm_event.is_some(), "Expected wasm event with error");

        router_contract
            .execute(
                &euclid::msgs::router::ExecuteMsg::ReregisterChain {
                    chain: ChainUid::create("osmosis".to_string()).unwrap(),
                },
                &[],
            )
            .unwrap();
    }

    // Add Liquidity
    // Need to request register escrow first
    let add_liquidity_request = factory_contract
        .execute(
            &euclid::msgs::factory::ExecuteMsg::AddLiquidityRequest {
                pair_info: PairWithDenomAndAmount {
                    token_1: token_a.with_amount(Uint128::from(10_000u128)),
                    token_2: token_b.with_amount(Uint128::from(100_000u128)),
                },
                slippage_tolerance_bps: 100, // 1% slippage tolerance
                timeout: None,               // 10 minutes in seconds
            },
            &[
                coin(100_000u128, token_b.token.to_string()),
                coin(10_000u128, token_a.token.to_string()),
            ],
        )
        .unwrap();

    relay_factory_router_factory(
        add_liquidity_request.events,
        &factory_contract,
        &router_contract,
        &factory_chain_uid,
    )
    .unwrap();

    let liquidity_query: GetLiquidityResponse = vlp_contract
        .query(&euclid::msgs::vlp::QueryMsg::Liquidity {})
        .unwrap();
    assert_eq!(
        liquidity_query,
        GetLiquidityResponse {
            pair: Pair {
                token_1: token_a.token.clone(),
                token_2: token_b.token.clone(),
            },
            token_1_reserve: Uint128::new(10_000u128 * 2),
            token_2_reserve: Uint128::new(100_000u128 * 2),
            total_lp_tokens: Uint128::new(31622u128 * 2),
        }
    );

    let virtual_balance_contract = router_contract
        .get_state()
        .unwrap()
        .virtual_balance_address
        .unwrap();

    let virtual_balance_contract = get_virtual_balance(&router_chain, &virtual_balance_contract);

    let virtual_balance_state_query: euclid::msgs::virtual_balance::GetUserBalancesResponse =
        virtual_balance_contract
            .query(&euclid::msgs::virtual_balance::QueryMsg::GetUserBalances {
                user: CrossChainUser::new(
                    ChainUid::vsl_chain_uid(),
                    vlp_contract.address().unwrap().into_string(),
                ),
            })
            .unwrap();

    let expected_virtual_balance_state_query =
        euclid::msgs::virtual_balance::GetUserBalancesResponse {
            balances: vec![
                euclid::msgs::virtual_balance::GetUserBalancesResponseItem {
                    amount: Uint128::from(20_000u128),
                    token_id: "token.a".to_string(),
                },
                euclid::msgs::virtual_balance::GetUserBalancesResponseItem {
                    amount: Uint128::from(20_0000u128),
                    token_id: "token.b".to_string(),
                },
            ],
        };

    assert_eq!(
        virtual_balance_state_query,
        expected_virtual_balance_state_query
    );
    // Euclid escrow contract
    let escrow_query: EscrowStateResponse = escrow_token_a
        .query(&euclid::msgs::escrow::QueryMsg::State {})
        .unwrap();
    assert_eq!(
        escrow_query,
        EscrowStateResponse {
            token: token_a.token.clone(),
            factory_address: factory_contract.address().unwrap(),
            total_amount: Uint128::from(10_000u128 * 2),
        }
    );
    // Osmo escrow contract
    let escrow_query: EscrowStateResponse = escrow_token_b
        .query(&euclid::msgs::escrow::QueryMsg::State {})
        .unwrap();
    assert_eq!(
        escrow_query,
        EscrowStateResponse {
            token: token_b.token.clone(),
            factory_address: factory_contract.address().unwrap(),
            total_amount: Uint128::from(100_000u128 * 2),
        }
    );

    // Deposit
    let factory_deposit_request = factory_contract
        .execute(
            &euclid::msgs::factory::ExecuteMsg::DepositToken {
                asset_in: token_a.clone(),
                amount_in: Uint128::from(1000u128),
                timeout: None,
                recipient: None,
                msg: None,
            },
            &[coin(1000u128, token_a.token.to_string())],
        )
        .unwrap();

    let res = relay_factory_router_factory(
        factory_deposit_request.events,
        &factory_contract,
        &router_contract,
        &factory_chain_uid,
    )
    .unwrap();
    let wasm_event = res.iter().find(|event| {
        event.ty == "wasm"
            && event.attributes.iter().any(|attr| {
                attr.key == "reply_on_cosmos_receive_processing" && attr.value == "error"
            })
    });
    assert!(wasm_event.is_none(), "Expected wasm event without error");

    let balance_key = BalanceKey {
        cross_chain_user: CrossChainUser::new(
            ChainUid::vsl_chain_uid(),
            vlp_contract.address().unwrap().into_string(),
        ),
        token_id: token_b_id,
    };
    println!("balance key in test: {:?}", balance_key);

    let _virtual_balance_allowance_err = virtual_balance_contract
        .query::<euclid::msgs::virtual_balance::GetAllowanceResponse>(
            &euclid::msgs::virtual_balance::QueryMsg::GetAllowance { balance_key },
        )
        .unwrap_err();

    // Withdraw
    // Two cross chain users on unique chains
    let withdraw_request = factory_contract
        .execute(
            &euclid::msgs::factory::ExecuteMsg::WithdrawVirtualBalance {
                token: token_a.token.clone(),
                amount: Uint128::from(100u128),
                cross_chain_addresses: vec![
                    CrossChainUserWithLimit {
                        user: CrossChainUser::new(
                            factory_chain_uid.clone(),
                            factory_contract.environment().sender.to_string(),
                        ),
                        limit: None,
                        preferred_token_type: None,
                        refund_address: None,
                        unsafe_refund_voucher_to_recipient: None,
                        forwarding_message: None,
                        voucher_msg: None,
                    },
                    CrossChainUserWithLimit {
                        user: CrossChainUser::new(
                            router_chain_uid.clone(),
                            router_contract.environment().sender.to_string(),
                        ),
                        limit: None,
                        preferred_token_type: None,
                        refund_address: None,
                        unsafe_refund_voucher_to_recipient: None,
                        forwarding_message: None,
                        voucher_msg: None,
                    },
                ],
                timeout: None,
            },
            &[],
        )
        .unwrap();

    let res = relay_factory_router_factory(
        withdraw_request.events,
        &factory_contract,
        &router_contract,
        &factory_chain_uid,
    )
    .unwrap();
    let wasm_event = res.iter().find(|event| {
        event.ty == "wasm"
            && event.attributes.iter().any(|attr| {
                attr.key == "reply_on_cosmos_receive_processing" && attr.value == "error"
            })
    });
    assert!(wasm_event.is_none(), "Expected wasm event without error");

    // Query get all pools from factory
    let all_pools_query: AllPoolsResponse = factory_contract
        .query(&euclid::msgs::factory::QueryMsg::GetAllPools {})
        .unwrap();
    let expected_all_pools_response = AllPoolsResponse {
        pools: vec![PoolVlpResponse {
            pair: Pair {
                token_1: Token::create("token.a".to_string()).unwrap(),
                token_2: Token::create("token.b".to_string()).unwrap(),
            },
            vlp: vlp_contract.address().unwrap().into_string(),
        }],
    };
    assert_eq!(all_pools_query, expected_all_pools_response);

    // Query get all tokens from factory
    let all_tokens_query: AllTokensResponse = factory_contract
        .query(&euclid::msgs::factory::QueryMsg::GetAllTokens {})
        .unwrap();
    let expected_all_tokens_response = AllTokensResponse {
        tokens: vec![
            Token::create("token.a".to_string()).unwrap(),
            Token::create("token.b".to_string()).unwrap(),
        ],
    };
    assert_eq!(all_tokens_query, expected_all_tokens_response);

    // Query partner fees collected from factory
    let partner_fees_collected_query: PartnerFeesCollectedResponse = factory_contract
        .query(&euclid::msgs::factory::QueryMsg::GetPartnerFeesCollected {})
        .unwrap();
    let expected_partner_fees_collected_response = PartnerFeesCollectedResponse {
        total: DenomFees {
            totals: HashMap::new(),
        },
    };
    assert_eq!(
        partner_fees_collected_query,
        expected_partner_fees_collected_response
    );

    // Remove liquidity
    let remove_liquidity_request = factory_contract
        .execute(
            &euclid::msgs::factory::ExecuteMsg::WithdrawVirtualBalance {
                token: token_a.token.clone(),
                amount: Uint128::from(100u128),
                cross_chain_addresses: vec![],
                timeout: None,
            },
            &[],
        )
        .unwrap();

    let res = relay_factory_router_factory(
        remove_liquidity_request.events,
        &factory_contract,
        &router_contract,
        &factory_chain_uid,
    )
    .unwrap();

    let wasm_event = res.iter().find(|event| {
        event.ty == "wasm"
            && event.attributes.iter().any(|attr| {
                attr.key == "reply_on_cosmos_receive_processing" && attr.value == "error"
            })
    });
    assert!(wasm_event.is_none(), "Expected wasm event without error");
}

#[test]
#[should_panic(expected = "Slippage Tolerance must be between 0 and 100")]
fn test_add_liquidity_fails_with_invalid_slippage_tolerance() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let _factory_chain = interchain.get_chain("osmosis").unwrap();
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = setup_router(&router_chain).unwrap();
    let factory = setup_factory(&interchain, "osmosis", "nibiru", &router).unwrap();

    let pair_info = PairWithDenomAndAmount {
        token_1: TokenWithDenomAndAmount {
            token: Token::create("eucl".to_string()).unwrap(),
            amount: Uint128::from(10_000u128),
            token_type: euclid::token::TokenType::Native {
                denom: "eucl".to_string(),
            },
        },
        token_2: TokenWithDenomAndAmount {
            token: Token::create("nibi".to_string()).unwrap(),
            amount: Uint128::from(100_000u128),
            token_type: euclid::token::TokenType::Native {
                denom: "nibi".to_string(),
            },
        },
    };
    register_token(&factory, &router, pair_info.token_1.to_token_with_denom()).unwrap();
    create_pool(
        &factory,
        &router,
        pair_info.clone(),
        BPS_1_PERCENT,
        PoolConfig::ConstantProduct {},
    )
    .unwrap();

    // adding funds
    let chain = interchain
        .get_chain(factory.environment().chain_id().as_str())
        .unwrap();
    let mut funds = vec![];
    for token in pair_info.get_vec_token_info() {
        faucet(
            &chain,
            chain.sender.as_str(),
            token.amount.u128(),
            token.token_type.clone(),
            &mut funds,
        );
    }

    add_liquidity(&interchain, &factory, &router, pair_info, 0, None, funds).unwrap();
}

#[test]
#[should_panic(expected = "Pool doesn't exist for this chain")]
fn test_add_liquidity_fails_when_pool_does_not_exit() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = setup_router(&router_chain).unwrap();
    let factory = setup_factory(&interchain, "osmosis", "nibiru", &router).unwrap();

    let pair_info = PairWithDenomAndAmount {
        token_1: TokenWithDenomAndAmount {
            token: Token::create("eucl".to_string()).unwrap(),
            amount: Uint128::from(10_000u128),
            token_type: euclid::token::TokenType::Native {
                denom: "eucl".to_string(),
            },
        },
        token_2: TokenWithDenomAndAmount {
            token: Token::create("nibi".to_string()).unwrap(),
            amount: Uint128::from(100_000u128),
            token_type: euclid::token::TokenType::Native {
                denom: "nibi".to_string(),
            },
        },
    };
    register_token(&factory, &router, pair_info.token_1.to_token_with_denom()).unwrap();
    register_token(&factory, &router, pair_info.token_2.to_token_with_denom()).unwrap();

    // adding funds
    let chain = interchain
        .get_chain(factory.environment().chain_id().as_str())
        .unwrap();
    let mut funds = vec![];
    for token in pair_info.get_vec_token_info() {
        faucet(
            &chain,
            chain.sender.as_str(),
            token.amount.u128(),
            token.token_type.clone(),
            &mut funds,
        );
    }

    add_liquidity(
        &interchain,
        &factory,
        &router,
        pair_info,
        BPS_100_PERCENT,
        None,
        funds,
    )
    .unwrap();
}

#[test]
#[should_panic(expected = "Amount cannot be zero")]
fn test_add_liquidity_fails_with_zero_liquidity_amount() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = setup_router(&router_chain).unwrap();
    let factory = setup_factory(&interchain, "osmosis", "nibiru", &router).unwrap();

    let pair_info = PairWithDenomAndAmount {
        token_1: TokenWithDenomAndAmount {
            token: Token::create("eucl".to_string()).unwrap(),
            amount: Uint128::from(0u128),
            token_type: euclid::token::TokenType::Native {
                denom: "eucl".to_string(),
            },
        },
        token_2: TokenWithDenomAndAmount {
            token: Token::create("nibi".to_string()).unwrap(),
            amount: Uint128::from(100_000u128),
            token_type: euclid::token::TokenType::Native {
                denom: "nibi".to_string(),
            },
        },
    };
    register_token(&factory, &router, pair_info.token_1.to_token_with_denom()).unwrap();

    create_pool(
        &factory,
        &router,
        pair_info.clone(),
        BPS_1_PERCENT,
        PoolConfig::ConstantProduct {},
    )
    .unwrap();

    // adding funds
    let chain = interchain
        .get_chain(factory.environment().chain_id().as_str())
        .unwrap();
    let mut funds = vec![];
    for token in pair_info.get_vec_token_info() {
        faucet(
            &chain,
            chain.sender.as_str(),
            token.amount.u128(),
            token.token_type.clone(),
            &mut funds,
        );
    }

    add_liquidity(
        &interchain,
        &factory,
        &router,
        pair_info,
        BPS_100_PERCENT,
        None,
        funds,
    )
    .unwrap();
}

#[test]
#[should_panic(expected = "The deposit amount is insufficient to add the liquidity")]
fn test_add_liquidity_fails_with_insufficient_deposit() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = setup_router(&router_chain).unwrap();
    let factory = setup_factory(&interchain, "osmosis", "nibiru", &router).unwrap();

    let pair_info = PairWithDenomAndAmount {
        token_1: TokenWithDenomAndAmount {
            token: Token::create("eucl".to_string()).unwrap(),
            amount: Uint128::from(100_000u128),
            token_type: euclid::token::TokenType::Native {
                denom: "eucl".to_string(),
            },
        },
        token_2: TokenWithDenomAndAmount {
            token: Token::create("nibi".to_string()).unwrap(),
            amount: Uint128::from(100_000u128),
            token_type: euclid::token::TokenType::Native {
                denom: "nibi".to_string(),
            },
        },
    };
    register_token(&factory, &router, pair_info.token_1.to_token_with_denom()).unwrap();

    create_pool(
        &factory,
        &router,
        pair_info.clone(),
        BPS_1_PERCENT,
        PoolConfig::ConstantProduct {},
    )
    .unwrap();

    add_liquidity(
        &interchain,
        &factory,
        &router,
        pair_info,
        BPS_100_PERCENT,
        None,
        vec![],
    )
    .unwrap();
}

#[test]
#[should_panic(expected = "UnsupportedDenomination")]
fn test_add_liquidity_fails_with_unsupported_token_denomination() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = setup_router(&router_chain).unwrap();
    let factory = setup_factory(&interchain, "osmosis", "nibiru", &router).unwrap();

    let mut pair_info = PairWithDenomAndAmount {
        token_1: TokenWithDenomAndAmount {
            token: Token::create("eucl".to_string()).unwrap(),
            amount: Uint128::from(100_000u128),
            token_type: euclid::token::TokenType::Native {
                denom: "eucl".to_string(),
            },
        },
        token_2: TokenWithDenomAndAmount {
            token: Token::create("nibi".to_string()).unwrap(),
            amount: Uint128::from(100_000u128),
            token_type: euclid::token::TokenType::Native {
                denom: "nibi".to_string(),
            },
        },
    };
    register_token(&factory, &router, pair_info.token_1.to_token_with_denom()).unwrap();

    create_pool(
        &factory,
        &router,
        pair_info.clone(),
        BPS_1_PERCENT,
        PoolConfig::ConstantProduct {},
    )
    .unwrap();

    // Attempt to add liquidity with tokens that aren't allowed by the escrow.
    pair_info.token_1.token_type = TokenType::Native {
        denom: "osmo".to_string(),
    };

    // adding funds
    let chain = interchain
        .get_chain(factory.environment().chain_id().as_str())
        .unwrap();
    let mut funds = vec![];
    for token in pair_info.get_vec_token_info() {
        faucet(
            &chain,
            chain.sender.as_str(),
            token.amount.u128(),
            token.token_type.clone(),
            &mut funds,
        );
    }

    add_liquidity(
        &interchain,
        &factory,
        &router,
        pair_info,
        BPS_100_PERCENT,
        None,
        funds,
    )
    .unwrap();
}

#[test]
#[should_panic(expected = "Extra funds are not allowed")]
fn test_add_liquidity_fails_with_extra_funds() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = setup_router(&router_chain).unwrap();
    let factory = setup_factory(&interchain, "osmosis", "nibiru", &router).unwrap();

    let pair_info = PairWithDenomAndAmount {
        token_1: TokenWithDenomAndAmount {
            token: Token::create("eucl".to_string()).unwrap(),
            amount: Uint128::from(10_000u128),
            token_type: euclid::token::TokenType::Native {
                denom: "eucl".to_string(),
            },
        },
        token_2: TokenWithDenomAndAmount {
            token: Token::create("nibi".to_string()).unwrap(),
            amount: Uint128::from(100_000u128),
            token_type: euclid::token::TokenType::Native {
                denom: "nibi".to_string(),
            },
        },
    };
    register_token(&factory, &router, pair_info.token_1.to_token_with_denom()).unwrap();

    create_pool(
        &factory,
        &router,
        pair_info.clone(),
        BPS_1_PERCENT,
        PoolConfig::ConstantProduct {},
    )
    .unwrap();

    // adding funds
    let chain = interchain
        .get_chain(factory.environment().chain_id().as_str())
        .unwrap();
    let mut funds = vec![];
    for token in pair_info.get_vec_token_info() {
        faucet(
            &chain,
            chain.sender.as_str(),
            token.amount.u128() + 10,
            token.token_type.clone(),
            &mut funds,
        );
    }

    add_liquidity(
        &interchain,
        &factory,
        &router,
        pair_info,
        BPS_100_PERCENT,
        None,
        funds,
    )
    .unwrap();
}

#[test]
fn test_add_liquidity_with_timeout() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = setup_router(&router_chain).unwrap();
    let factory = setup_factory(&interchain, "osmosis", "nibiru", &router).unwrap();

    let pair_info = PairWithDenomAndAmount {
        token_1: TokenWithDenomAndAmount {
            token: Token::create("eucl".to_string()).unwrap(),
            amount: Uint128::from(10_000u128),
            token_type: euclid::token::TokenType::Native {
                denom: "eucl".to_string(),
            },
        },
        token_2: TokenWithDenomAndAmount {
            token: Token::create("nibi".to_string()).unwrap(),
            amount: Uint128::from(100_000u128),
            token_type: euclid::token::TokenType::Native {
                denom: "nibi".to_string(),
            },
        },
    };
    register_token(&factory, &router, pair_info.token_1.to_token_with_denom()).unwrap();

    create_pool(
        &factory,
        &router,
        pair_info.clone(),
        BPS_1_PERCENT,
        PoolConfig::ConstantProduct {},
    )
    .unwrap();

    // adding funds
    let chain = interchain
        .get_chain(factory.environment().chain_id().as_str())
        .unwrap();
    let mut funds = vec![];
    for token in pair_info.get_vec_token_info() {
        faucet(
            &chain,
            chain.sender.as_str(),
            token.amount.u128(),
            token.token_type.clone(),
            &mut funds,
        );
    }

    add_liquidity(
        &interchain,
        &factory,
        &router,
        pair_info,
        BPS_100_PERCENT,
        Some(30),
        funds,
    )
    .unwrap();
}

#[test]
#[should_panic(expected = "Invalid Timeout")]
fn test_add_liquidity_with_invalid_timeout() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = setup_router(&router_chain).unwrap();
    let factory = setup_factory(&interchain, "osmosis", "nibiru", &router).unwrap();

    let pair_info = PairWithDenomAndAmount {
        token_1: TokenWithDenomAndAmount {
            token: Token::create("eucl".to_string()).unwrap(),
            amount: Uint128::from(10_000u128),
            token_type: euclid::token::TokenType::Native {
                denom: "eucl".to_string(),
            },
        },
        token_2: TokenWithDenomAndAmount {
            token: Token::create("nibi".to_string()).unwrap(),
            amount: Uint128::from(100_000u128),
            token_type: euclid::token::TokenType::Native {
                denom: "nibi".to_string(),
            },
        },
    };
    register_token(&factory, &router, pair_info.token_1.to_token_with_denom()).unwrap();

    create_pool(
        &factory,
        &router,
        pair_info.clone(),
        BPS_1_PERCENT,
        PoolConfig::ConstantProduct {},
    )
    .unwrap();

    // adding funds
    let chain = interchain
        .get_chain(factory.environment().chain_id().as_str())
        .unwrap();
    let mut funds = vec![];
    for token in pair_info.get_vec_token_info() {
        faucet(
            &chain,
            chain.sender.as_str(),
            token.amount.u128(),
            token.token_type.clone(),
            &mut funds,
        );
    }

    add_liquidity(
        &interchain,
        &factory,
        &router,
        pair_info,
        BPS_100_PERCENT,
        Some(241),
        funds,
    )
    .unwrap();
}

#[rstest]
#[case("osmosis", "nibiru")]
#[case("nibiru", "nibiru")]
fn test_swap_request(#[case] factory_chain_id: &str, #[case] router_chain_id: &str) {
    let sender = "sender_for_all_chains";
    let mut chains = vec![(factory_chain_id, sender)];
    if factory_chain_id != router_chain_id {
        chains.push((router_chain_id, sender));
    }
    let interchain = MockInterchainEnv::new(chains);
    let router_chain = interchain.get_chain(router_chain_id).unwrap();
    let router = setup_router(&router_chain).unwrap();

    let factory = setup_factory(&interchain, factory_chain_id, router_chain_id, &router).unwrap();
    run_test_swap_request_reusable(
        sender,
        &factory,
        &router,
        None,
        ChainUid::create(factory_chain_id.to_string()).unwrap(),
    )
    .unwrap();
}

pub struct SwapTestReusableOutput {
    // pub token_in: TokenWithDenom,
    pub token_out: TokenWithDenom,
    // pub amount_in: Uint128,
}

pub fn run_test_swap_request_reusable(
    sender: &str,
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    cross_chain_addresses: Option<Vec<CrossChainUserWithLimit>>,
    chain_uid: ChainUid,
) -> Result<SwapTestReusableOutput, CwEnvError> {
    let factory_chain = factory.environment();
    let router_chain = router.environment();

    let router_chain_id = router_chain.chain_id();
    let factory_chain_id = factory_chain.chain_id();
    let factory_chain_uid = ChainUid::create(factory_chain_id.to_string()).unwrap();

    let token_a = Token::create("token.a".to_string()).unwrap();
    let token_a = TokenWithDenom {
        token: token_a.clone(),
        token_type: euclid::token::TokenType::Native {
            denom: token_a.to_string(),
        },
    };
    let token_b = Token::create("token.b".to_string()).unwrap();
    let token_b = TokenWithDenom {
        token: token_b.clone(),
        token_type: euclid::token::TokenType::Native {
            denom: token_b.to_string(),
        },
    };
    let mut funds = vec![];
    let sender = factory_chain.addr_make(sender);
    for token in [token_a.clone(), token_b.clone()] {
        faucet(
            factory_chain,
            sender.as_str(),
            100_000_000_000u128,
            token.token_type,
            &mut funds,
        );
    }

    register_token(factory, router, token_a.clone())?;
    register_token(factory, router, token_b.clone())?;

    let pool_token_1 = token_a.with_amount(Uint128::from(10_000_000_000u128));
    let pool_token_2 = token_b.with_amount(Uint128::from(100_000_000_000u128));

    create_pool(
        factory,
        router,
        PairWithDenomAndAmount {
            token_1: pool_token_1.clone(),
            token_2: pool_token_2.clone(),
        },
        BPS_1_PERCENT,
        PoolConfig::ConstantProduct {},
    )?;

    let vlp_query =
        router.get_vlp(Pair::new(token_a.token.clone(), token_b.token.clone()).unwrap())?;

    let vlp_contract = get_vlp(router_chain, &Addr::unchecked(vlp_query.vlp));

    let liquidity_query: GetLiquidityResponse =
        vlp_contract.query(&euclid::msgs::vlp::QueryMsg::Liquidity {})?;
    assert_eq!(
        liquidity_query,
        GetLiquidityResponse {
            pair: Pair {
                token_1: token_a.token.clone(),
                token_2: token_b.token.clone(),
            },
            token_1_reserve: pool_token_1.amount,
            token_2_reserve: pool_token_2.amount,
            total_lp_tokens: Uint128::new(31_622_776_601),
        }
    );

    let escrow_token_a = get_escrow(factory, token_a.token.to_string().as_str());
    let escrow_token_b = get_escrow(factory, token_b.token.to_string().as_str());

    // Osmo escrow contract
    let escrow_query: EscrowStateResponse =
        escrow_token_a.query(&euclid::msgs::escrow::QueryMsg::State {})?;
    assert_eq!(
        escrow_query,
        EscrowStateResponse {
            token: token_a.token.clone(),
            factory_address: factory.address().unwrap(),
            total_amount: pool_token_1.amount,
        }
    );

    // This is the escrow for the Euclid token
    let escrow_query: EscrowStateResponse =
        escrow_token_b.query(&euclid::msgs::escrow::QueryMsg::State {})?;
    assert_eq!(
        escrow_query,
        EscrowStateResponse {
            token: token_b.token.clone(),
            factory_address: factory.address().unwrap(),
            total_amount: pool_token_2.amount,
        }
    );

    let amount_in = Uint128::new(1_000_000);

    let cross_chain_addresses = vec![CrossChainUserWithLimit {
        user: CrossChainUser::new(factory_chain_uid.clone(), sender.to_string()),
        limit: None,
        preferred_token_type: None,
        refund_address: None,
        forwarding_message: None,
        voucher_msg: None,
        unsafe_refund_voucher_to_recipient: None,
    }];

    let swap_request_msg = factory.execute(
        &euclid::msgs::factory::ExecuteMsg::ExecuteSwapRequest(ExecuteSwapRequest {
            sender: None,
            asset_in: token_a.clone(),
            amount_in,
            asset_out: token_b.token.clone(),
            min_amount_out: Uint128::new(50),
            timeout: None,
            swaps: vec![NextSwapPair {
                token_in: token_a.token.clone(),
                token_out: token_b.token.clone(),
                test_fail: None,
            }],
            cross_chain_addresses: cross_chain_addresses.clone(),
            partner_fee: None,
            meta: None,
        }),
        &[coin(amount_in.u128(), token_a.token.to_string())],
    )?;

    if router_chain_id != factory_chain_id {
        // Query pending swap request
        let pending_swap_request_query: GetPendingSwapsResponse = factory
            .query(&euclid::msgs::factory::QueryMsg::PendingSwapsUser {
                user: sender.clone(),
                pagination: Pagination::new(None, None, None, None),
            })
            .unwrap();
        println!(
            "pending swap request query: {:?}",
            pending_swap_request_query
        );
        let expected_pending_swap_request = GetPendingSwapsResponse {
        pending_swaps: vec![SwapRequest {
            sender: sender.to_string(),
            tx_id: "osmosis:cosmwasm1s3ul5svzwn3hamk4w434tch9tcqrgl3drjcsju768sk6dxzjvq0qe4umm9:osmosis:12345:0:4".to_string(),
            asset_in: TokenWithDenom {
                token: Token::create("token.a".to_string()).unwrap(),
                token_type: euclid::token::TokenType::Native {
                    denom: "token.a".to_string(),
                },
            },
            amount_in: Uint128::new(1_000_000),
            asset_out: Token::create("token.b".to_string()).unwrap(),
            min_amount_out: Uint128::new(50),
            swaps: vec![NextSwapPair {
                token_in: Token::create("token.a".to_string()).unwrap(),
                token_out: Token::create("token.b".to_string()).unwrap(),
                test_fail: None,
            }],
            timeout: IbcTimeout::with_timestamp(Timestamp::from_nanos(1571797479879305533)),
            cross_chain_addresses: vec![CrossChainUserWithLimit {
                user: CrossChainUser {
                    chain_uid: chain_uid,
                    address: sender.to_string(),
                },
                limit: cross_chain_addresses[0].limit.clone(),
                preferred_token_type: None,
                refund_address: None,
                unsafe_refund_voucher_to_recipient: None,
                forwarding_message: None,
                voucher_msg: None,
            }],
            partner_fee_amount: Uint128::zero(),
            partner_fee_recipient: Addr::unchecked("cosmwasm1s3ul5svzwn3hamk4w434tch9tcqrgl3drjcsju768sk6dxzjvq0qe4umm9"),
            }],
        };
        assert_eq!(pending_swap_request_query, expected_pending_swap_request);
    }

    relay_factory_router_factory(swap_request_msg.events, factory, router, &factory_chain_uid)?;

    Ok(SwapTestReusableOutput {
        // token_in: token_a,
        token_out: token_b,
        // amount_in,
    })
}

#[test]
fn test_multi_hop_swap_request_ibc() {
    run_test_multi_hop_swap_request("osmosis", "nibiru");
}

#[test]
fn test_multi_hop_swap_request_native() {
    run_test_multi_hop_swap_request("nibiru", "nibiru");
}

fn run_test_multi_hop_swap_request(factory_chain_id: &str, router_chain_id: &str) {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let mut chains = vec![(factory_chain_id, sender.as_str())];
    if factory_chain_id != router_chain_id {
        chains.push((router_chain_id, sender.as_str()));
    }
    let interchain = MockInterchainEnv::new(chains);
    let factory_chain = interchain.get_chain(factory_chain_id).unwrap();
    let factory_chain_uid = ChainUid::create(factory_chain_id.to_string()).unwrap();
    let router_chain = interchain.get_chain(router_chain_id).unwrap();
    let router = setup_router(&router_chain).unwrap();

    let factory = setup_factory(&interchain, factory_chain_id, router_chain_id, &router).unwrap();

    let token_a = Token::create("token.a".to_string()).unwrap();
    let token_a = TokenWithDenom {
        token: token_a.clone(),
        token_type: euclid::token::TokenType::Native {
            denom: token_a.to_string(),
        },
    };
    let token_b = Token::create("token.b".to_string()).unwrap();
    let token_b = TokenWithDenom {
        token: token_b.clone(),
        token_type: euclid::token::TokenType::Native {
            denom: token_b.to_string(),
        },
    };
    let token_c = Token::create("token.c".to_string()).unwrap();
    let token_c = TokenWithDenom {
        token: token_c.clone(),
        token_type: euclid::token::TokenType::Native {
            denom: token_c.to_string(),
        },
    };
    let mut funds = vec![];
    let sender = factory_chain.addr_make("sender_for_all_chains");
    for token in [token_a.clone(), token_b.clone(), token_c.clone()] {
        faucet(
            &factory_chain,
            sender.as_str(),
            100_000_000_000u128,
            token.token_type,
            &mut funds,
        );
    }

    register_token(&factory, &router, token_a.clone()).unwrap();
    register_token(&factory, &router, token_b.clone()).unwrap();
    register_token(&factory, &router, token_c.clone()).unwrap();

    let pools = vec![
        (token_a.clone(), token_b.clone()),
        (token_b.clone(), token_c.clone()),
    ];

    for (token_in, token_out) in pools {
        create_pool(
            &factory,
            &router,
            PairWithDenomAndAmount {
                token_1: token_in.with_amount(Uint128::from(10_000u128)),
                token_2: token_out.with_amount(Uint128::from(100_000u128)),
            },
            BPS_1_PERCENT,
            PoolConfig::ConstantProduct {},
        )
        .unwrap();

        let vlp_query = router
            .get_vlp(Pair::new(token_in.token.clone(), token_out.token.clone()).unwrap())
            .unwrap();

        let vlp_contract = get_vlp(&router_chain, &Addr::unchecked(vlp_query.vlp));

        let liquidity_query: GetLiquidityResponse = vlp_contract
            .query(&euclid::msgs::vlp::QueryMsg::Liquidity {})
            .unwrap();
        assert_eq!(
            liquidity_query,
            GetLiquidityResponse {
                pair: Pair {
                    token_1: token_in.token.clone(),
                    token_2: token_out.token.clone(),
                },
                token_1_reserve: Uint128::new(10_000),
                token_2_reserve: Uint128::new(100_000),
                total_lp_tokens: Uint128::new(31622),
            }
        );
    }

    let random_user = factory_chain.addr_make("random_user");

    let swap_request_msg = factory
        .execute(
            &euclid::msgs::factory::ExecuteMsg::ExecuteSwapRequest(ExecuteSwapRequest {
                sender: None,
                asset_in: token_a.clone(),
                amount_in: Uint128::new(100),
                asset_out: token_c.token.clone(),
                min_amount_out: Uint128::new(50),
                timeout: None,
                swaps: vec![
                    NextSwapPair {
                        token_in: token_a.token.clone(),
                        token_out: token_b.token.clone(),
                        test_fail: None,
                    },
                    NextSwapPair {
                        token_in: token_b.token.clone(),
                        token_out: token_c.token.clone(),
                        test_fail: None,
                    },
                ],
                cross_chain_addresses: vec![CrossChainUserWithLimit {
                    user: CrossChainUser::new(factory_chain_uid.clone(), random_user.to_string()),
                    limit: None,
                    preferred_token_type: None,
                    refund_address: None,
                    forwarding_message: None,
                    voucher_msg: None,
                    unsafe_refund_voucher_to_recipient: None,
                }],
                partner_fee: None,
                meta: None,
            }),
            &[coin(100u128, token_a.token.to_string())],
        )
        .unwrap();

    let received_events = relay_factory_router_factory(
        swap_request_msg.events,
        &factory,
        &router,
        &factory_chain_uid,
    )
    .unwrap();

    relay_router_factory_router(received_events, &factory, &factory_chain_uid, &router).unwrap();

    let random_user_balance = factory_chain
        .query_balance(
            &Addr::unchecked(random_user.to_string()),
            token_c.token.to_string().as_str(),
        )
        .unwrap();
    assert!(
        random_user_balance > Uint128::new(0),
        "Random user balance is {}",
        random_user_balance
    );
}

#[test]
fn test_swap_request_with_valid_partner_fee_ibc() {
    run_swap_request_with_valid_partner_fee("osmosis", "nibiru");
}

#[test]
fn test_swap_request_with_valid_partner_fee_native() {
    run_swap_request_with_valid_partner_fee("nibiru", "nibiru");
}

fn run_swap_request_with_valid_partner_fee(factory_chain_id: &str, router_chain_id: &str) {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let mut chains = vec![(factory_chain_id, sender.as_str())];
    if factory_chain_id != router_chain_id {
        chains.push((router_chain_id, sender.as_str()));
    }
    let interchain = MockInterchainEnv::new(chains);
    let router_chain = interchain.get_chain(router_chain_id).unwrap();

    let router = setup_router(&router_chain).unwrap();
    let factory = setup_factory(&interchain, factory_chain_id, router_chain_id, &router).unwrap();

    let pair_info = PairWithDenomAndAmount {
        token_1: TokenWithDenomAndAmount {
            token: Token::create("eucl".to_string()).unwrap(),
            amount: Uint128::from(10_000u128),
            token_type: euclid::token::TokenType::Native {
                denom: "eucl".to_string(),
            },
        },
        token_2: TokenWithDenomAndAmount {
            token: Token::create("nibi".to_string()).unwrap(),
            amount: Uint128::from(100_000u128),
            token_type: euclid::token::TokenType::Native {
                denom: "nibi".to_string(),
            },
        },
    };
    let partner_fee_recipient = factory
        .environment()
        .addr_make("partner_fee_recipient")
        .into_string();
    let old_partner_eucl_balance = factory
        .environment()
        .query_balance(&Addr::unchecked(partner_fee_recipient.clone()), "eucl")
        .unwrap();
    register_token(&factory, &router, pair_info.token_1.to_token_with_denom()).unwrap();
    create_pool(
        &factory,
        &router,
        pair_info.clone(),
        BPS_1_PERCENT,
        PoolConfig::ConstantProduct {},
    )
    .unwrap();

    let asset_in = TokenWithDenom {
        token: Token::create("eucl".to_string()).unwrap(),
        token_type: euclid::token::TokenType::Native {
            denom: "eucl".to_string(),
        },
    };

    // adding funds
    let chain = interchain
        .get_chain(factory.environment().chain_id().as_str())
        .unwrap();
    let mut funds = vec![];
    for token in pair_info.get_vec_token_info() {
        faucet(
            &chain,
            chain.sender.as_str(),
            token.amount.u128(),
            token.token_type.clone(),
            &mut funds,
        );
    }

    add_liquidity(
        &interchain,
        &factory,
        &router,
        pair_info,
        BPS_1_PERCENT,
        None,
        funds.clone(),
    )
    .unwrap();

    funds.clear();
    let amount_in = Uint128::new(10000);
    faucet(
        &chain,
        chain.sender.as_str(),
        amount_in.u128(),
        asset_in.token_type.clone(),
        &mut funds,
    );

    // swapping
    swap_request(
        &interchain,
        &factory,
        &router,
        None,
        asset_in,
        amount_in,
        Token::create("nibi".to_string()).unwrap(),
        Uint128::new(50),
        Some(60),
        // Set swaps such that first_swap.token_in doesn't match asset_in.token or
        // last_swap.token_out doesn't match asset_out.
        vec![NextSwapPair {
            token_in: Token::create("eucl".to_string()).unwrap(),
            token_out: Token::create("nibi".to_string()).unwrap(),
            test_fail: None,
        }],
        vec![],
        Some(PartnerFee {
            partner_fee_bps: 30,
            recipient: partner_fee_recipient.clone(),
        }),
        funds,
        None,
    )
    .unwrap();

    let new_partner_eucl_balance = factory
        .environment()
        .query_balance(&Addr::unchecked(partner_fee_recipient.clone()), "eucl")
        .unwrap();
    assert_eq!(
        new_partner_eucl_balance,
        old_partner_eucl_balance + Uint128::from(30u128)
    );
}

#[test]
#[should_panic(expected = "InvalidPartnerFee")]
fn test_swap_request_fails_with_invalid_partner_fee_bps() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = setup_router(&router_chain).unwrap();
    let factory = setup_factory(&interchain, "osmosis", "nibiru", &router).unwrap();

    let pair_info = PairWithDenomAndAmount {
        token_1: TokenWithDenomAndAmount {
            token: Token::create("eucl".to_string()).unwrap(),
            amount: Uint128::from(10_000u128),
            token_type: euclid::token::TokenType::Native {
                denom: "eucl".to_string(),
            },
        },
        token_2: TokenWithDenomAndAmount {
            token: Token::create("nibi".to_string()).unwrap(),
            amount: Uint128::from(100_000u128),
            token_type: euclid::token::TokenType::Native {
                denom: "nibi".to_string(),
            },
        },
    };
    register_token(&factory, &router, pair_info.token_1.to_token_with_denom()).unwrap();
    create_pool(
        &factory,
        &router,
        pair_info.clone(),
        BPS_1_PERCENT,
        PoolConfig::ConstantProduct {},
    )
    .unwrap();

    let asset_in = TokenWithDenom {
        token: Token::create("eucl".to_string()).unwrap(),
        token_type: euclid::token::TokenType::Native {
            denom: "eucl".to_string(),
        },
    };

    // adding funds
    let chain = interchain
        .get_chain(factory.environment().chain_id().as_str())
        .unwrap();
    let mut funds = vec![];
    for token in pair_info.get_vec_token_info() {
        faucet(
            &chain,
            chain.sender.as_str(),
            token.amount.u128(),
            token.token_type.clone(),
            &mut funds,
        );
    }

    add_liquidity(
        &interchain,
        &factory,
        &router,
        pair_info,
        BPS_1_PERCENT,
        None,
        funds.clone(),
    )
    .unwrap();

    funds.clear();
    faucet(
        &chain,
        chain.sender.as_str(),
        1000,
        asset_in.token_type.clone(),
        &mut funds,
    );

    // Providing a partner_fee_bps above MAX_PARTNER_FEE_BPS
    let partner_fee_recipient = factory
        .environment()
        .addr_make("partner_fee_recipient")
        .into_string();

    let old_partner_eucl_balance = factory
        .environment()
        .query_balance(&Addr::unchecked(partner_fee_recipient.clone()), "eucl")
        .unwrap();

    swap_request(
        &interchain,
        &factory,
        &router,
        None,
        asset_in,
        Uint128::new(1000),
        Token::create("nibi".to_string()).unwrap(),
        Uint128::new(50),
        None,
        vec![NextSwapPair {
            token_in: Token::create("eucl".to_string()).unwrap(),
            token_out: Token::create("nibi".to_string()).unwrap(),
            test_fail: None,
        }],
        vec![],
        Some(PartnerFee {
            // Partner fee BPS above MAX_PARTNER_FEE_BPS
            partner_fee_bps: MAX_PARTNER_FEE_BPS + 1,
            recipient: partner_fee_recipient.clone(),
        }),
        funds,
        None,
    )
    .unwrap();

    let new_partner_eucl_balance = factory
        .environment()
        .query_balance(&Addr::unchecked(partner_fee_recipient.clone()), "eucl")
        .unwrap();
    assert_eq!(new_partner_eucl_balance, old_partner_eucl_balance);
}

#[test]
#[should_panic(expected = "UnsupportedDenomination")]
fn test_swap_request_fails_for_unsupported_denomination_for_asset_in() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = setup_router(&router_chain).unwrap();
    let factory = setup_factory(&interchain, "osmosis", "nibiru", &router).unwrap();

    let pair_info = PairWithDenomAndAmount {
        token_1: TokenWithDenomAndAmount {
            token: Token::create("eucl".to_string()).unwrap(),
            amount: Uint128::from(10_000u128),
            token_type: euclid::token::TokenType::Native {
                denom: "eucl".to_string(),
            },
        },
        token_2: TokenWithDenomAndAmount {
            token: Token::create("nibi".to_string()).unwrap(),
            amount: Uint128::from(100_000u128),
            token_type: euclid::token::TokenType::Native {
                denom: "nibi".to_string(),
            },
        },
    };
    register_token(&factory, &router, pair_info.token_1.to_token_with_denom()).unwrap();
    create_pool(
        &factory,
        &router,
        pair_info.clone(),
        BPS_1_PERCENT,
        PoolConfig::ConstantProduct {},
    )
    .unwrap();

    // Use a token not supported by the escrow
    let asset_in = TokenWithDenom {
        token: Token::create("eucl".to_string()).unwrap(),
        token_type: euclid::token::TokenType::Native {
            denom: "juno".to_string(),
        },
    };

    // adding funds
    let chain = interchain
        .get_chain(factory.environment().chain_id().as_str())
        .unwrap();
    let mut funds = vec![];
    for token in pair_info.get_vec_token_info() {
        faucet(
            &chain,
            chain.sender.as_str(),
            token.amount.u128(),
            token.token_type.clone(),
            &mut funds,
        );
    }

    add_liquidity(
        &interchain,
        &factory,
        &router,
        pair_info,
        BPS_1_PERCENT,
        None,
        funds.clone(),
    )
    .unwrap();

    funds.clear();
    let amount_in = Uint128::new(1000);
    faucet(
        &chain,
        chain.sender.as_str(),
        amount_in.u128(),
        asset_in.token_type.clone(),
        &mut funds,
    );
    let sender = factory.environment().addr_make("sender_for_all_chains");
    let old_sender_eucl_balance = factory
        .environment()
        .query_balance(&Addr::unchecked(sender.clone()), "juno")
        .unwrap();

    // swapping for inavlid denom not registered on escrow
    swap_request(
        &interchain,
        &factory,
        &router,
        None,
        asset_in,
        amount_in,
        Token::create("nibi".to_string()).unwrap(),
        Uint128::new(50),
        None,
        vec![NextSwapPair {
            token_in: Token::create("eucl".to_string()).unwrap(),
            token_out: Token::create("nibi".to_string()).unwrap(),
            test_fail: None,
        }],
        vec![],
        None,
        funds,
        None,
    )
    .unwrap();

    let new_sender_eucl_balance = factory
        .environment()
        .query_balance(&Addr::unchecked(sender.clone()), "juno")
        .unwrap();
    assert_eq!(
        new_sender_eucl_balance, old_sender_eucl_balance,
        "Balance not refunded after failed swap"
    );
}

#[test]
#[should_panic(expected = "Cannot Swap 0 tokens")]
fn test_swap_request_fails_for_zero_min_amount_out() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = setup_router(&router_chain).unwrap();
    let factory = setup_factory(&interchain, "osmosis", "nibiru", &router).unwrap();

    let pair_info = PairWithDenomAndAmount {
        token_1: TokenWithDenomAndAmount {
            token: Token::create("eucl".to_string()).unwrap(),
            amount: Uint128::from(10_000u128),
            token_type: euclid::token::TokenType::Native {
                denom: "eucl".to_string(),
            },
        },
        token_2: TokenWithDenomAndAmount {
            token: Token::create("nibi".to_string()).unwrap(),
            amount: Uint128::from(100_000u128),
            token_type: euclid::token::TokenType::Native {
                denom: "nibi".to_string(),
            },
        },
    };
    register_token(&factory, &router, pair_info.token_1.to_token_with_denom()).unwrap();
    create_pool(
        &factory,
        &router,
        pair_info.clone(),
        BPS_1_PERCENT,
        PoolConfig::ConstantProduct {},
    )
    .unwrap();

    let asset_in = TokenWithDenom {
        token: Token::create("eucl".to_string()).unwrap(),
        token_type: euclid::token::TokenType::Native {
            denom: "eucl".to_string(),
        },
    };

    // adding funds
    let chain = interchain
        .get_chain(factory.environment().chain_id().as_str())
        .unwrap();
    let mut funds = vec![];
    for token in pair_info.get_vec_token_info() {
        faucet(
            &chain,
            chain.sender.as_str(),
            token.amount.u128(),
            token.token_type.clone(),
            &mut funds,
        );
    }

    add_liquidity(
        &interchain,
        &factory,
        &router,
        pair_info,
        BPS_1_PERCENT,
        None,
        funds.clone(),
    )
    .unwrap();

    funds.clear();
    faucet(
        &chain,
        chain.sender.as_str(),
        1000,
        asset_in.token_type.clone(),
        &mut funds,
    );

    // swapping
    swap_request(
        &interchain,
        &factory,
        &router,
        None,
        asset_in,
        Uint128::new(1000),
        Token::create("nibi".to_string()).unwrap(),
        Uint128::new(0),
        None,
        vec![NextSwapPair {
            token_in: Token::create("eucl".to_string()).unwrap(),
            token_out: Token::create("nibi".to_string()).unwrap(),
            test_fail: None,
        }],
        vec![],
        None,
        funds,
        None,
    )
    .unwrap();
}

#[test]
#[should_panic(expected = "Token in doesn't match swap route")]
fn test_swap_request_fails_for_invalid_swap_route() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = setup_router(&router_chain).unwrap();
    let factory = setup_factory(&interchain, "osmosis", "nibiru", &router).unwrap();

    let pair_info = PairWithDenomAndAmount {
        token_1: TokenWithDenomAndAmount {
            token: Token::create("eucl".to_string()).unwrap(),
            amount: Uint128::from(10_000u128),
            token_type: euclid::token::TokenType::Native {
                denom: "eucl".to_string(),
            },
        },
        token_2: TokenWithDenomAndAmount {
            token: Token::create("nibi".to_string()).unwrap(),
            amount: Uint128::from(100_000u128),
            token_type: euclid::token::TokenType::Native {
                denom: "nibi".to_string(),
            },
        },
    };
    register_token(&factory, &router, pair_info.token_1.to_token_with_denom()).unwrap();
    create_pool(
        &factory,
        &router,
        pair_info.clone(),
        BPS_1_PERCENT,
        PoolConfig::ConstantProduct {},
    )
    .unwrap();

    let asset_in = TokenWithDenom {
        token: Token::create("eucl".to_string()).unwrap(),
        token_type: euclid::token::TokenType::Native {
            denom: "eucl".to_string(),
        },
    };

    // adding funds
    let chain = interchain
        .get_chain(factory.environment().chain_id().as_str())
        .unwrap();
    let mut funds = vec![];
    for token in pair_info.get_vec_token_info() {
        faucet(
            &chain,
            chain.sender.as_str(),
            token.amount.u128(),
            token.token_type.clone(),
            &mut funds,
        );
    }

    add_liquidity(
        &interchain,
        &factory,
        &router,
        pair_info,
        BPS_1_PERCENT,
        None,
        funds.clone(),
    )
    .unwrap();

    funds.clear();
    faucet(
        &chain,
        chain.sender.as_str(),
        1000,
        asset_in.token_type.clone(),
        &mut funds,
    );

    let sender = factory.environment().addr_make("sender_for_all_chains");
    let old_sender_balance = factory
        .environment()
        .query_balance(&Addr::unchecked(sender.clone()), "eucl")
        .unwrap();

    // swapping
    swap_request(
        &interchain,
        &factory,
        &router,
        None,
        asset_in,
        Uint128::new(1000),
        Token::create("nibi".to_string()).unwrap(),
        Uint128::new(50),
        None,
        // Set swaps such that first_swap.token_in doesn't match asset_in.token or
        // last_swap.token_out doesn't match asset_out.
        vec![NextSwapPair {
            token_in: Token::create("osmo".to_string()).unwrap(),
            token_out: Token::create("nibi".to_string()).unwrap(),
            test_fail: None,
        }],
        vec![CrossChainUserWithLimit {
            user: CrossChainUser::new(
                ChainUid::create("nibiru".to_string()).unwrap(),
                sender.to_string(),
            ),
            limit: None,
            preferred_token_type: None,
            refund_address: None,
            forwarding_message: None,
            voucher_msg: None,
            unsafe_refund_voucher_to_recipient: None,
        }],
        None,
        funds,
        None,
    )
    .unwrap();
    let new_sender_balance = factory
        .environment()
        .query_balance(&Addr::unchecked(sender.clone()), "eucl")
        .unwrap();

    assert_eq!(
        old_sender_balance, new_sender_balance,
        "Refund not initiated with failed swap route"
    );
}

#[test]
fn test_swap_request_with_timeout() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = setup_router(&router_chain).unwrap();
    let factory = setup_factory(&interchain, "osmosis", "nibiru", &router).unwrap();

    let pair_info = PairWithDenomAndAmount {
        token_1: TokenWithDenomAndAmount {
            token: Token::create("eucl".to_string()).unwrap(),
            amount: Uint128::from(10_000u128),
            token_type: euclid::token::TokenType::Native {
                denom: "eucl".to_string(),
            },
        },
        token_2: TokenWithDenomAndAmount {
            token: Token::create("nibi".to_string()).unwrap(),
            amount: Uint128::from(100_000u128),
            token_type: euclid::token::TokenType::Native {
                denom: "nibi".to_string(),
            },
        },
    };
    register_token(&factory, &router, pair_info.token_1.to_token_with_denom()).unwrap();
    create_pool(
        &factory,
        &router,
        pair_info.clone(),
        BPS_1_PERCENT,
        PoolConfig::ConstantProduct {},
    )
    .unwrap();

    let asset_in = TokenWithDenom {
        token: Token::create("eucl".to_string()).unwrap(),
        token_type: euclid::token::TokenType::Native {
            denom: "eucl".to_string(),
        },
    };

    // adding funds
    let chain = interchain
        .get_chain(factory.environment().chain_id().as_str())
        .unwrap();
    let mut funds = vec![];
    for token in pair_info.get_vec_token_info() {
        faucet(
            &chain,
            chain.sender.as_str(),
            token.amount.u128(),
            token.token_type.clone(),
            &mut funds,
        );
    }

    add_liquidity(
        &interchain,
        &factory,
        &router,
        pair_info,
        BPS_1_PERCENT,
        None,
        funds.clone(),
    )
    .unwrap();

    funds.clear();
    faucet(
        &chain,
        chain.sender.as_str(),
        1000,
        asset_in.token_type.clone(),
        &mut funds,
    );

    // swapping
    swap_request(
        &interchain,
        &factory,
        &router,
        None,
        asset_in,
        Uint128::new(1000),
        Token::create("nibi".to_string()).unwrap(),
        Uint128::new(50),
        Some(60),
        // Set swaps such that first_swap.token_in doesn't match asset_in.token or
        // last_swap.token_out doesn't match asset_out.
        vec![NextSwapPair {
            token_in: Token::create("eucl".to_string()).unwrap(),
            token_out: Token::create("nibi".to_string()).unwrap(),
            test_fail: None,
        }],
        vec![],
        None,
        funds,
        None,
    )
    .unwrap();
}

#[rstest]
#[case("osmosis", "nibiru")]
#[case("nibiru", "osmosis")]
fn test_stable_pool_swap_request(#[case] factory_chain_id: &str, #[case] router_chain_id: &str) {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let mut chains = vec![(factory_chain_id, sender.as_str())];
    if factory_chain_id != router_chain_id {
        chains.push((router_chain_id, sender.as_str()));
    }
    let interchain = MockInterchainEnv::new(chains);
    let factory_chain = interchain.get_chain(factory_chain_id).unwrap();
    let factory_chain_uid = ChainUid::create(factory_chain_id.to_string()).unwrap();
    let router_chain = interchain.get_chain(router_chain_id).unwrap();
    let router = setup_router(&router_chain).unwrap();

    let factory = setup_factory(&interchain, factory_chain_id, router_chain_id, &router).unwrap();

    let token_a = Token::create("token.a".to_string()).unwrap();
    let token_a = TokenWithDenom {
        token: token_a.clone(),
        token_type: euclid::token::TokenType::Native {
            denom: token_a.to_string(),
        },
    };
    let token_b = Token::create("token.b".to_string()).unwrap();
    let token_b = TokenWithDenom {
        token: token_b.clone(),
        token_type: euclid::token::TokenType::Native {
            denom: token_b.to_string(),
        },
    };
    let mut funds = vec![];
    let sender = router_chain.addr_make("sender_for_all_chains");
    for token in [token_a.clone(), token_b.clone()] {
        faucet(
            &factory_chain,
            sender.as_str(),
            100_000_000_000u128,
            token.token_type,
            &mut funds,
        );
    }

    register_token(&factory, &router, token_a.clone()).unwrap();
    register_token(&factory, &router, token_b.clone()).unwrap();

    create_pool(
        &factory,
        &router,
        PairWithDenomAndAmount {
            token_1: token_a.with_amount(Uint128::from(10_000u128)),
            token_2: token_b.with_amount(Uint128::from(100_000u128)),
        },
        BPS_1_PERCENT,
        PoolConfig::Stable {
            amp_factor: Some(Uint64::from(100u64)),
        },
    )
    .unwrap();

    let vlp_query = router
        .get_vlp(Pair::new(token_a.token.clone(), token_b.token.clone()).unwrap())
        .unwrap();

    let vlp_contract = get_vlp(&router_chain, &Addr::unchecked(vlp_query.vlp));

    let liquidity_query: GetLiquidityResponse = vlp_contract
        .query(&euclid::msgs::vlp::QueryMsg::Liquidity {})
        .unwrap();
    assert_eq!(
        liquidity_query,
        GetLiquidityResponse {
            pair: Pair {
                token_1: token_a.token.clone(),
                token_2: token_b.token.clone(),
            },
            token_1_reserve: Uint128::new(10_000),
            token_2_reserve: Uint128::new(100_000),
            total_lp_tokens: Uint128::new(31622),
        }
    );

    let escrow_token_a = get_escrow(&factory, token_a.token.to_string().as_str());
    let escrow_token_b = get_escrow(&factory, token_b.token.to_string().as_str());

    // Osmo escrow contract
    let escrow_query: EscrowStateResponse = escrow_token_a
        .query(&euclid::msgs::escrow::QueryMsg::State {})
        .unwrap();
    assert_eq!(
        escrow_query,
        EscrowStateResponse {
            token: token_a.token.clone(),
            factory_address: factory.address().unwrap(),
            total_amount: Uint128::from(10_000u128),
        }
    );

    // This is the escrow for the Euclid token
    let escrow_query: EscrowStateResponse = escrow_token_b
        .query(&euclid::msgs::escrow::QueryMsg::State {})
        .unwrap();
    assert_eq!(
        escrow_query,
        EscrowStateResponse {
            token: token_b.token.clone(),
            factory_address: factory.address().unwrap(),
            total_amount: Uint128::from(100_000u128),
        }
    );

    let swap_request_msg = factory
        .execute(
            &euclid::msgs::factory::ExecuteMsg::ExecuteSwapRequest(ExecuteSwapRequest {
                sender: None,
                asset_in: token_a.clone(),
                amount_in: Uint128::new(100),
                asset_out: token_b.token.clone(),
                min_amount_out: Uint128::new(50),
                timeout: None,
                swaps: vec![NextSwapPair {
                    token_in: token_a.token.clone(),
                    token_out: token_b.token.clone(),
                    test_fail: None,
                }],
                cross_chain_addresses: vec![CrossChainUserWithLimit {
                    user: CrossChainUser::new(factory_chain_uid.clone(), sender.to_string()),
                    limit: None,
                    preferred_token_type: None,
                    refund_address: None,
                    forwarding_message: None,
                    voucher_msg: None,
                    unsafe_refund_voucher_to_recipient: None,
                }],
                partner_fee: None,
                meta: None,
            }),
            &[coin(100u128, token_a.token.to_string())],
        )
        .unwrap();

    relay_factory_router_factory(
        swap_request_msg.events,
        &factory,
        &router,
        &factory_chain_uid,
    )
    .unwrap();
}

#[rstest]
#[case(false)]
#[case(true)]
fn test_deposit_and_withdraw(#[case] disallow: bool) {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = setup_router(&router_chain).unwrap();
    let factory_chain_uid = ChainUid::create("osmosis".to_string()).unwrap();
    let factory = setup_factory(&interchain, &factory_chain_uid, "nibiru", &router).unwrap();

    let token = TokenWithDenomAndAmount {
        token: Token::create("osmo".to_string()).unwrap(),
        amount: Uint128::from(100_000u128),
        token_type: TokenType::Native {
            denom: "osmo".to_string(),
        },
    };

    // Register token
    register_token(&factory, &router, token.to_token_with_denom()).unwrap();

    // Deposit with claim msg
    deposit_token(
        &factory,
        &router,
        token.to_token_with_denom(),
        token.amount,
        None,
        None,
    )
    .unwrap();

    // Query escrow state after deposit
    let mut escrow_contract = get_escrow(&factory, token.token.to_string().as_str());
    let escrow_query = escrow_contract.state().unwrap();
    assert_eq!(
        escrow_query,
        EscrowStateResponse {
            token: token.token.clone(),
            factory_address: factory.address().unwrap(),
            total_amount: token.amount,
        }
    );

    // Optionally disallow denom
    if disallow {
        escrow_contract.set_sender(&factory.address().unwrap());
        escrow_contract
            .disallow_denom(token.token_type.clone())
            .unwrap();
    }

    // Withdraw tokens
    let withdraw_response = factory
        .execute(
            &euclid::msgs::factory::ExecuteMsg::WithdrawVirtualBalance {
                token: token.token.clone(),
                amount: token.amount,
                cross_chain_addresses: vec![CrossChainUserWithLimit {
                    user: CrossChainUser::new(
                        factory_chain_uid.clone(),
                        factory.environment().sender.to_string(),
                    ),
                    limit: None,
                    preferred_token_type: None,
                    refund_address: None,
                    forwarding_message: None,
                    voucher_msg: None,
                    unsafe_refund_voucher_to_recipient: None,
                }],
                timeout: None,
            },
            &[],
        )
        .unwrap();

    let relay_response = relay_factory_router_factory(
        withdraw_response.events,
        &factory,
        &router,
        &factory_chain_uid,
    )
    .unwrap();

    relay_router_factory_router(relay_response, &factory, &factory_chain_uid, &router).unwrap();

    // Query escrow state after withdrawal
    let escrow_query: EscrowStateResponse = escrow_contract.state().unwrap();

    assert_eq!(
        escrow_query,
        EscrowStateResponse {
            token: token.token.clone(),
            factory_address: factory.address().unwrap(),
            total_amount: Uint128::zero(),
        }
    );
}

#[test]
fn test_deposit_and_withdraw_multiple_chains() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![
        ("osmosis", &sender),
        ("nibiru", &sender),
        ("andromeda", &sender),
    ]);
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = setup_router(&router_chain).unwrap();
    let factory_chain_uid = ChainUid::create("osmosis".to_string()).unwrap();
    let factory = setup_factory(&interchain, &factory_chain_uid, "nibiru", &router).unwrap();

    let factory_chain_uid_2 = ChainUid::create("andromeda".to_string()).unwrap();
    let factory_2 = setup_factory(&interchain, &factory_chain_uid_2, "nibiru", &router).unwrap();

    let token = TokenWithDenomAndAmount {
        token: Token::create("osmo".to_string()).unwrap(),
        amount: Uint128::from(100_000u128),
        token_type: TokenType::Native {
            denom: "osmo".to_string(),
        },
    };

    // Register token
    register_token(&factory, &router, token.to_token_with_denom()).unwrap();

    // Deposit with claim msg
    deposit_token(
        &factory,
        &router,
        token.to_token_with_denom(),
        token.amount,
        None,
        // None,
        None,
    )
    .unwrap();

    // Query escrow state after deposit
    let escrow_contract = get_escrow(&factory, token.token.to_string().as_str());
    let escrow_query = escrow_contract.state().unwrap();
    assert_eq!(
        escrow_query,
        EscrowStateResponse {
            token: token.token.clone(),
            factory_address: factory.address().unwrap(),
            total_amount: token.amount,
        }
    );

    // Register token 2
    register_token(&factory_2, &router, token.to_token_with_denom()).unwrap();

    // Deposit with claim msg
    deposit_token(
        &factory_2,
        &router,
        token.to_token_with_denom(),
        token.amount,
        None,
        // None,
        None,
    )
    .unwrap();

    // Query escrow state after deposit
    let escrow_contract_2 = get_escrow(&factory_2, token.token.to_string().as_str());
    let escrow_query = escrow_contract_2.state().unwrap();
    assert_eq!(
        escrow_query,
        EscrowStateResponse {
            token: token.token.clone(),
            factory_address: factory_2.address().unwrap(),
            total_amount: token.amount,
        }
    );

    // Withdraw tokens
    let withdraw_amount = Uint128::new(1000);
    let user_1_limit = Uint128::new(500);
    let withdraw_response = factory
        .execute(
            &euclid::msgs::factory::ExecuteMsg::WithdrawVirtualBalance {
                token: token.token.clone(),
                amount: withdraw_amount,
                cross_chain_addresses: vec![
                    CrossChainUserWithLimit {
                        user: CrossChainUser::new(
                            factory_chain_uid.clone(),
                            factory.environment().sender.to_string(),
                        ),
                        limit: Some(euclid::chain::Limit::Equal(user_1_limit)),
                        preferred_token_type: None,
                        refund_address: None,
                        forwarding_message: None,
                        voucher_msg: None,
                        unsafe_refund_voucher_to_recipient: None,
                    },
                    CrossChainUserWithLimit {
                        user: CrossChainUser::new(
                            factory_chain_uid_2.clone(),
                            factory_2.environment().sender.to_string(),
                        ),
                        limit: None,
                        preferred_token_type: None,
                        refund_address: None,
                        forwarding_message: None,
                        voucher_msg: None,
                        unsafe_refund_voucher_to_recipient: None,
                    },
                ],
                timeout: None,
            },
            &[],
        )
        .unwrap();

    let relay_response = relay_factory_router_factory(
        withdraw_response.events.clone(),
        &factory,
        &router,
        &factory_chain_uid,
    )
    .unwrap();

    relay_router_factory_router(
        relay_response.clone(),
        &factory,
        &factory_chain_uid,
        &router,
    )
    .unwrap();
    relay_router_factory_router(relay_response, &factory_2, &factory_chain_uid_2, &router).unwrap();

    // Query escrow state after withdrawal
    let escrow_query: EscrowStateResponse = escrow_contract.state().unwrap();

    assert_eq!(
        escrow_query,
        EscrowStateResponse {
            token: token.token.clone(),
            factory_address: factory.address().unwrap(),
            total_amount: token.amount - user_1_limit,
        }
    );

    let escrow_query_2: EscrowStateResponse = escrow_contract_2.state().unwrap();
    assert_eq!(
        escrow_query_2,
        EscrowStateResponse {
            token: token.token.clone(),
            factory_address: factory_2.address().unwrap(),
            total_amount: token.amount - user_1_limit,
        }
    );
}

#[test]
fn test_deposit_and_withdraw_with_failure() {
    let sender_label = String::from("sender_for_all_chains");
    let interchain =
        MockInterchainEnv::new(vec![("osmosis", &sender_label), ("nibiru", &sender_label)]);
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = setup_router(&router_chain).unwrap();
    let factory_chain_uid = ChainUid::create("osmosis".to_string()).unwrap();
    let factory = setup_factory(&interchain, &factory_chain_uid, "nibiru", &router).unwrap();

    let token = TokenWithDenomAndAmount {
        token: Token::create("osmo".to_string()).unwrap(),
        amount: Uint128::from(100_000u128),
        token_type: TokenType::Native {
            denom: "osmo".to_string(),
        },
    };

    // Register token
    register_token(&factory, &router, token.to_token_with_denom()).unwrap();

    let factory_chain_uid = &factory.get_state().unwrap().chain_uid;
    let mut funds = vec![];
    faucet(
        factory.environment(),
        factory.environment().sender.as_str(),
        token.amount.u128(),
        token.token_type.clone(),
        &mut funds,
    );
    let tx_response = factory
        .execute(
            &euclid::msgs::factory::ExecuteMsg::DepositToken {
                asset_in: token.to_token_with_denom(),
                amount_in: token.amount,
                timeout: None,
                recipient: None,
                msg: None,
            },
            &funds,
        )
        .unwrap();
    // Relay only send packet, no ack. This way escrow won't have funds and our withdraw will fail.
    relay_factory_send_packet(tx_response.events, &router, factory_chain_uid).unwrap();

    let virtual_balance_contract = router.get_state().unwrap().virtual_balance_address.unwrap();

    let virtual_balance_contract = get_virtual_balance(&router_chain, &virtual_balance_contract);

    let balance_key = BalanceKey {
        cross_chain_user: CrossChainUser::new(
            factory_chain_uid.clone(),
            factory.environment().sender.to_string(),
        ),
        token_id: token.token.to_string(),
    };
    let balance = virtual_balance_contract
        .get_balance(balance_key.clone())
        .unwrap();
    assert_eq!(balance.amount, token.amount);

    // Query escrow state after deposit
    let escrow_contract = get_escrow(&factory, token.token.to_string().as_str());
    let escrow_query = escrow_contract.state().unwrap();
    assert_eq!(
        escrow_query,
        EscrowStateResponse {
            token: token.token.clone(),
            factory_address: factory.address().unwrap(),
            total_amount: Uint128::zero(),
        }
    );

    let new_recipient = CrossChainUser::new(
        factory_chain_uid.clone(),
        factory.environment().addr_make("new_recipient").to_string(),
    );

    // Withdraw tokens
    let withdraw_response = factory
        .execute(
            &euclid::msgs::factory::ExecuteMsg::WithdrawVirtualBalance {
                token: token.token.clone(),
                amount: token.amount,
                cross_chain_addresses: vec![CrossChainUserWithLimit {
                    user: new_recipient.clone(),
                    limit: None,
                    preferred_token_type: None,
                    refund_address: None,
                    forwarding_message: None,
                    voucher_msg: None,
                    unsafe_refund_voucher_to_recipient: None,
                }],
                timeout: None,
            },
            &[],
        )
        .unwrap();

    let relay_response = relay_factory_router_factory(
        withdraw_response.events,
        &factory,
        &router,
        factory_chain_uid,
    )
    .unwrap();

    relay_router_factory_router(relay_response, &factory, factory_chain_uid, &router).unwrap();
    let balance = virtual_balance_contract
        .get_balance(balance_key.clone())
        .unwrap();
    assert_eq!(
        balance.amount, token.amount,
        "Voucher not refunded to original sender (unsafe send to recipient false)"
    );

    // Withdraw tokens
    let withdraw_response = factory
        .execute(
            &euclid::msgs::factory::ExecuteMsg::WithdrawVirtualBalance {
                token: token.token.clone(),
                amount: token.amount,
                cross_chain_addresses: vec![CrossChainUserWithLimit {
                    user: new_recipient.clone(),
                    limit: None,
                    preferred_token_type: None,
                    refund_address: None,
                    forwarding_message: None,
                    voucher_msg: None,
                    unsafe_refund_voucher_to_recipient: Some(true),
                }],
                timeout: None,
            },
            &[],
        )
        .unwrap();

    let relay_response = relay_factory_router_factory(
        withdraw_response.events,
        &factory,
        &router,
        factory_chain_uid,
    )
    .unwrap();

    relay_router_factory_router(relay_response, &factory, factory_chain_uid, &router).unwrap();
    let balance = virtual_balance_contract
        .get_balance(balance_key.clone())
        .unwrap();
    assert_eq!(
        balance.amount,
        Uint128::zero(),
        "Voucher refunded  to original sender after failed withdraw (unsafe send to recipient true)",
    );

    let new_balance_key = BalanceKey {
        cross_chain_user: new_recipient.clone(),
        token_id: token.token.to_string(),
    };

    let balance = virtual_balance_contract
        .get_balance(new_balance_key.clone())
        .unwrap();
    assert_eq!(
        balance.amount, token.amount,
        "Voucher refunded to recipient after failed withdraw (unsafe send to recipient true)",
    );
}
