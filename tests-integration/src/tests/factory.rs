#![cfg(not(target_arch = "wasm32"))]
use cosmwasm_std::{coin, to_json_binary, Addr, Coin, Uint128, Uint64};
use cw_orch::{
    core::CwEnvError,
    mock::MockBase,
    prelude::{CallAs, ContractInstance, CwOrchExecute, CwOrchQuery, Environment},
};
use cw_orch_interchain::core::InterchainEnv;
use cw_orch_interchain::prelude::*;
use escrow::mock::mock_escrow;
use euclid::{
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    error::ContractError,
    fee::{PartnerFee, BPS_100_PERCENT, BPS_1_PERCENT, MAX_PARTNER_FEE_BPS},
    limit::Limit,
    msgs::{
        cross_chain_config::CrossChainConfig,
        escrow::{QueryMsgFns as EscrowQueryMsgFns, StateResponse as EscrowStateResponse},
        factory::{
            cw20::FactoryCw20HookMsg, AllPoolsResponse, ExecuteSwapRequest,
            QueryMsgFns as FactoryQueryMsgFns, StateResponse,
        },
        lp_token::msg::{ExecuteMsgFns, QueryMsgFns},
        router::{
            AllEscrowsResponse, AllVlpResponse, QueryMsgFns as RouterQueryMsgFns,
            QueryTokenDenomsResponse, TokenDenom, VlpResponse,
        },
        virtual_balance::msg::QueryMsgFns as VirtualBalanceQueryMsgFns,
        vlp::base::{GetLiquidityQueryResponse, PoolConfig},
    },
    recipient::Recipient,
    swap::NextSwapPair,
    token::{
        Pair, PairWithDenomAndAmount, Token, TokenType, TokenWithDenom, TokenWithDenomAndAmount,
    },
    utils::pagination::Pagination,
    voucher::BalanceKey,
};
use factory::{
    mock::{mock_factory, MockFactory},
    FactoryContract,
};
use mock::{mock::mock_app, mock_builder::MockEuclidBuilder};
use router::RouterContract;

use crate::helpers::{
    chains::{get_escrow, get_lp_token, get_virtual_balance, get_vlp, setup_factory, setup_router},
    factory::{add_liquidity, create_pool, deposit_token, faucet, register_token, swap_request},
    relayer::{
        relay_factory_router_factory, relay_factory_send_packet, relay_router_factory_router,
    },
};

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
    let relayer_contract = Addr::unchecked("relayer_contract");
    let rate_limit_fee_recipient = Addr::unchecked("rate_limit_fee_recipient");
    let rate_limit_fee_denom = "rate_limit_fee_denom".to_string();
    let rate_limit_free_limit = Uint128::from(10u128);

    let mock_factory = MockFactory::instantiate(
        &mut factory,
        factory_code_id,
        owner.clone(),
        router_contract.clone(),
        chain_uid.clone(),
        escrow_code_id,
        cw20_code_id,
        true,
        relayer_contract.clone(),
        rate_limit_fee_recipient,
        rate_limit_fee_denom,
        rate_limit_free_limit,
    );

    let state_response = MockFactory::query_state(&mock_factory, &factory);
    let expected_state_id = StateResponse {
        chain_uid,
        router_contract,
        relayer_contract,
        admin: owner.clone().into_string(),
        escrow_code_id,
        lp_code_id: cw20_code_id,
        is_native: true,
    };
    assert_eq!(state_response, expected_state_id);
}

#[test]
fn test_ping_pong_flow_ibc() {
    let sender = Addr::unchecked("sender_for_all_chains");
    let interchain = MockInterchainEnv::new(vec![
        ("nibiru", sender.as_str()),
        ("osmosis", sender.as_str()),
    ]);
    let router = interchain.get_chain("nibiru").unwrap();
    let factory = interchain.get_chain("osmosis").unwrap();

    let router_contract = setup_router(&router).unwrap();
    let mut factory_contract =
        setup_factory(&interchain, "osmosis", "nibiru", &router_contract).unwrap();
    let factory_chain_uid = ChainUid::create("osmosis".to_string()).unwrap();

    let factory_block = factory_contract.environment().app.borrow().block_info();
    let not_admin = factory.addr_make("not_admin");
    factory_contract.set_sender(&not_admin);
    let ping_response = factory_contract
        .execute(
            &euclid::msgs::factory::ExecuteMsg::PingRouter {
                cross_chain_config: CrossChainConfig::default(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        ContractError::Unauthorized {},
        ping_response.downcast().unwrap()
    );
    let factory_admin = factory_contract.get_state().unwrap().admin;
    factory_contract.set_sender(&Addr::unchecked(factory_admin));
    let ping_response = factory_contract
        .execute(
            &euclid::msgs::factory::ExecuteMsg::PingRouter {
                cross_chain_config: CrossChainConfig::default(),
            },
            &[],
        )
        .unwrap();

    let router_block = router_contract.environment().app.borrow().block_info();
    relay_factory_router_factory(
        ping_response.events,
        &factory_contract,
        &router_contract,
        &factory_chain_uid,
    )
    .unwrap();

    let ping_response = factory_contract.get_latest_ping().unwrap();
    let pong_response = factory_contract.get_latest_pong().unwrap();
    let ping = ping_response.ping.expect("latest ping missing");
    let pong = pong_response.pong.expect("latest pong missing");

    assert_eq!(ping.tx_id, pong.tx_id);
    assert_eq!(ping.block_height, factory_block.height);
    assert_eq!(ping.timestamp, factory_block.time.seconds());
    assert_eq!(pong.block_height, router_block.height);
    assert_eq!(pong.timestamp, router_block.time.seconds());
}

#[test]
fn test_create_pool_with_funds_ibc() {
    run_create_pool_with_funds("nibiru", "osmosis");
}

#[test]
fn test_create_pool_with_funds_native() {
    run_create_pool_with_funds("nibiru", "nibiru");
}

fn run_create_pool_with_funds(router_chain_id: &str, factory_chain_id: &str) {
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

    let router_contract = setup_router(&router, vec![factory_chain_id]).unwrap();
    let router_state = router_contract.get_state().unwrap();

    let _virtual_balance_router =
        get_virtual_balance(&router, &router_state.virtual_balance_address);

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

    // // Register escrow
    let register_escrow_request = factory_contract
        .execute(
            &euclid::msgs::factory::ExecuteMsg::RegisterDenom {
                token_with_denom: token_a.clone(),
                cross_chain_config: CrossChainConfig::default(),
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

    // let _ = interchain
    //     .await_packets("osmosis", register_escrow_request)
    //     .unwrap();

    let token_denoms_response: QueryTokenDenomsResponse = router_contract
        .query(&euclid::msgs::router::QueryMsg::QueryTokenDenoms {
            token: token_a.token.clone(),
        })
        .unwrap();

    assert_eq!(
        token_denoms_response,
        QueryTokenDenomsResponse {
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
            pair_with_denom_and_amount: PairWithDenomAndAmount {
                token_1: token_a.with_amount(Uint128::from(0u128)),
                token_2: token_b.with_amount(Uint128::from(0u128)),
            },
            slippage_tolerance_bps: 100,
            lp_token_name: "lp".to_string(),
            lp_token_symbol: "lp".to_string(),
            lp_token_decimal: 6,
            lp_token_marketing: None,
            pool_config: PoolConfig::ConstantProduct {},
            cross_chain_config: CrossChainConfig::default(),
        },
        &[], // Some(&[coin(0u128, "osmo"), coin(0u128, "eucl")]),
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
                pair_with_denom_and_amount: PairWithDenomAndAmount {
                    token_1: token_a.with_amount(Uint128::from(10_000u128)),
                    token_2: token_b.with_amount(Uint128::from(100_000u128)),
                },
                slippage_tolerance_bps: 100,
                lp_token_name: "lpname".to_string(),
                lp_token_symbol: "lpsymbol".to_string(),
                lp_token_decimal: 6,
                lp_token_marketing: None,
                pool_config: PoolConfig::ConstantProduct {},
                cross_chain_config: CrossChainConfig::default(),
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

    let liquidity_query: GetLiquidityQueryResponse = vlp_contract
        .query(&euclid::msgs::vlp::cp::QueryMsg::Liquidity {})
        .unwrap();
    assert_eq!(
        liquidity_query,
        GetLiquidityQueryResponse {
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
            &euclid::msgs::factory::ExecuteMsg::AddLiquidity {
                pair_with_denom_and_amount: PairWithDenomAndAmount {
                    token_1: token_a.with_amount(Uint128::from(10_000u128)),
                    token_2: token_b.with_amount(Uint128::from(100_000u128)),
                },
                slippage_tolerance_bps: 100,
                cross_chain_config: CrossChainConfig::default(),
            },
            &[
                coin(10_000u128, token_a.token.to_string()),
                coin(100_000u128, token_b.token.to_string()),
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

    let liquidity_query: GetLiquidityQueryResponse = vlp_contract
        .query(&euclid::msgs::vlp::cp::QueryMsg::Liquidity {})
        .unwrap();
    assert_eq!(
        liquidity_query,
        GetLiquidityQueryResponse {
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

    let _resp: AllEscrowsResponse = router_contract
        .query(&euclid::msgs::router::QueryMsg::QueryAllEscrows {
            pagination: Pagination::new(None, None, None, None),
        })
        .unwrap();

    let _resp: AllVlpResponse = router_contract
        .query(&euclid::msgs::router::QueryMsg::GetAllVlps {
            pagination: Pagination::new(None, None, None, None),
        })
        .unwrap();
}

#[test]
fn test_add_liquidity_ibc() {
    run_add_liquidity("osmosis", "nibiru");
}

#[test]
fn test_add_liquidity_native() {
    run_add_liquidity("nibiru", "nibiru");
}

fn run_add_liquidity(factory_chain_id: &str, router_chain_id: &str) {
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
            &Addr::unchecked(sender.clone()),
            vec![
                Coin::new(100000000000000u128, token_a_id.clone()),
                Coin::new(100000000000000u128, token_b_id.clone()),
            ],
        )
        .unwrap();

    router_chain
        .set_balance(
            &Addr::unchecked(sender.clone()),
            vec![
                Coin::new(100000000000000u128, token_a_id.clone()),
                Coin::new(100000000000000u128, token_b_id.clone()),
            ],
        )
        .unwrap();

    let router_contract = setup_router(&router_chain, vec![factory_chain_id]).unwrap();
    let _router_state = router_contract.get_state().unwrap();

    let factory_chain_uid = ChainUid::create(factory_chain_id.to_string()).unwrap();
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
            &euclid::msgs::factory::ExecuteMsg::RegisterDenom {
                token_with_denom: token_a.clone(),
                cross_chain_config: CrossChainConfig::default(),
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

    let token_denoms_response: QueryTokenDenomsResponse = router_contract
        .query(&euclid::msgs::router::QueryMsg::QueryTokenDenoms {
            token: token_a.token.clone(),
        })
        .unwrap();

    assert_eq!(
        token_denoms_response,
        QueryTokenDenomsResponse {
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
                pair_with_denom_and_amount: PairWithDenomAndAmount {
                    token_1: token_a.with_amount(Uint128::from(10_000u128)),
                    token_2: token_b.with_amount(Uint128::from(100_000u128)),
                },
                slippage_tolerance_bps: 100,
                lp_token_name: "lpname".to_string(),
                lp_token_symbol: "lpsymbol".to_string(),
                lp_token_decimal: 6,
                lp_token_marketing: None,
                pool_config: PoolConfig::ConstantProduct {},
                cross_chain_config: CrossChainConfig::default(),
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

    let liquidity_query: GetLiquidityQueryResponse = vlp_contract
        .query(&euclid::msgs::vlp::cp::QueryMsg::Liquidity {})
        .unwrap();
    assert_eq!(
        liquidity_query,
        GetLiquidityQueryResponse {
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

    // Add Liquidity
    // Need to request register escrow first
    let add_liquidity_request = factory_contract
        .execute(
            &euclid::msgs::factory::ExecuteMsg::AddLiquidity {
                pair_with_denom_and_amount: PairWithDenomAndAmount {
                    token_1: token_a.with_amount(Uint128::from(10_000u128)),
                    token_2: token_b.with_amount(Uint128::from(100_000u128)),
                },
                slippage_tolerance_bps: 100, // 1% slippage tolerance
                cross_chain_config: CrossChainConfig::default(),
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

    let liquidity_query: GetLiquidityQueryResponse = vlp_contract
        .query(&euclid::msgs::vlp::cp::QueryMsg::Liquidity {})
        .unwrap();
    assert_eq!(
        liquidity_query,
        GetLiquidityQueryResponse {
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
}

#[test]
#[should_panic(expected = "Slippage Tolerance must be between 0 and 100")]
fn test_add_liquidity_fails_with_invalid_slippage_tolerance() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let _factory_chain = interchain.get_chain("osmosis").unwrap();
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = setup_router(&router_chain, vec!["osmosis", "nibiru"]).unwrap();
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

    add_liquidity(&interchain, &factory, &router, pair_info, 0, funds).unwrap();
}

#[test]
#[should_panic(expected = "Pool doesn't exist for this chain")]
fn test_add_liquidity_fails_when_pool_does_not_exit() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = setup_router(&router_chain, vec!["osmosis", "nibiru"]).unwrap();
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

    let router = setup_router(&router_chain, vec!["osmosis", "nibiru"]).unwrap();
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

    let router = setup_router(&router_chain, vec!["osmosis", "nibiru"]).unwrap();
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

    let router = setup_router(&router_chain, vec!["osmosis", "nibiru"]).unwrap();
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

    let router = setup_router(&router_chain, vec!["osmosis", "nibiru"]).unwrap();
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
        funds,
    )
    .unwrap();
}

#[test]
fn test_remove_liquidity_ibc() {
    run_remove_liquidity("osmosis", "nibiru");
}

#[test]
fn test_remove_liquidity_native() {
    run_remove_liquidity("nibiru", "nibiru");
}

fn run_remove_liquidity(factory_chain_id: &str, router_chain_id: &str) {
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
            &Addr::unchecked(sender.clone()),
            vec![
                Coin::new(100000000000000u128, token_a_id.clone()),
                Coin::new(100000000000000u128, token_b_id.clone()),
            ],
        )
        .unwrap();

    router_chain
        .set_balance(
            &Addr::unchecked(sender.clone()),
            vec![
                Coin::new(100000000000000u128, token_a_id.clone()),
                Coin::new(100000000000000u128, token_b_id.clone()),
            ],
        )
        .unwrap();

    let router_contract = setup_router(&router_chain, vec![factory_chain_id]).unwrap();
    let _router_state = router_contract.get_state().unwrap();

    let factory_chain_uid = ChainUid::create(factory_chain_id.to_string()).unwrap();
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
            &euclid::msgs::factory::ExecuteMsg::RegisterDenom {
                token_with_denom: token_a.clone(),
                cross_chain_config: CrossChainConfig::default(),
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

    let token_denoms_response: QueryTokenDenomsResponse = router_contract
        .query(&euclid::msgs::router::QueryMsg::QueryTokenDenoms {
            token: token_a.token.clone(),
        })
        .unwrap();

    assert_eq!(
        token_denoms_response,
        QueryTokenDenomsResponse {
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
                pair_with_denom_and_amount: PairWithDenomAndAmount {
                    token_1: token_a.with_amount(Uint128::from(10_000u128)),
                    token_2: token_b.with_amount(Uint128::from(100_000u128)),
                },
                slippage_tolerance_bps: 100,
                lp_token_name: "lpname".to_string(),
                lp_token_symbol: "lpsymbol".to_string(),
                lp_token_decimal: 6,
                lp_token_marketing: None,
                pool_config: PoolConfig::ConstantProduct {},
                cross_chain_config: CrossChainConfig::default(),
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

    let vlp_contract = get_vlp(&router_chain, &Addr::unchecked(vlp_query.vlp.clone()));

    let liquidity_query: GetLiquidityQueryResponse = vlp_contract
        .query(&euclid::msgs::vlp::cp::QueryMsg::Liquidity {})
        .unwrap();
    assert_eq!(
        liquidity_query,
        GetLiquidityQueryResponse {
            pair: Pair {
                token_1: token_a.token.clone(),
                token_2: token_b.token.clone(),
            },
            token_1_reserve: Uint128::new(10_000),
            token_2_reserve: Uint128::new(100_000),
            total_lp_tokens: Uint128::new(31622),
        }
    );

    let lp_token_address_response = factory_contract
        .get_lp_token(vlp_query.vlp.clone())
        .unwrap();
    let lp_token_address = lp_token_address_response.token_address;
    let lp_token_contract = get_lp_token(&factory_chain, &Addr::unchecked(lp_token_address));

    let lp_token_balance_response = lp_token_contract
        .balance(factory_chain.sender.clone())
        .unwrap();
    let user_lp_balance = lp_token_balance_response.balance;
    assert_eq!(user_lp_balance, Uint128::from(30622u128));

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

    // Add Liquidity
    // Need to request register escrow first
    let add_liquidity_request = factory_contract
        .execute(
            &euclid::msgs::factory::ExecuteMsg::AddLiquidity {
                pair_with_denom_and_amount: PairWithDenomAndAmount {
                    token_1: token_a.with_amount(Uint128::from(10_000u128)),
                    token_2: token_b.with_amount(Uint128::from(100_000u128)),
                },
                slippage_tolerance_bps: 100, // 1% slippage tolerance
                cross_chain_config: CrossChainConfig::default(),
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

    let liquidity_query: GetLiquidityQueryResponse = vlp_contract
        .query(&euclid::msgs::vlp::cp::QueryMsg::Liquidity {})
        .unwrap();
    assert_eq!(
        liquidity_query,
        GetLiquidityQueryResponse {
            pair: Pair {
                token_1: token_a.token.clone(),
                token_2: token_b.token.clone(),
            },
            token_1_reserve: Uint128::new(10_000u128 * 2),
            token_2_reserve: Uint128::new(100_000u128 * 2),
            total_lp_tokens: Uint128::new(31622u128 * 2),
        }
    );
    let lp_token_balance_response = lp_token_contract
        .balance(factory_chain.sender.clone())
        .unwrap();
    let user_lp_balance = lp_token_balance_response.balance;
    assert_eq!(user_lp_balance, Uint128::from(62244u128));
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
    let remove_msg = FactoryCw20HookMsg::RemoveLiquidity {
        pair: Pair::new(token_a.token.clone(), token_b.token.clone()).unwrap(),
        recipient: CrossChainUser::new(factory_chain_uid.clone(), factory_chain.sender.to_string()),
        cross_chain_config: CrossChainConfig::default(),
    };
    // let lp_to_remove = Uint128::from(1000u128);
    let lp_to_remove = user_lp_balance;
    let remove_request = lp_token_contract
        .send(
            lp_to_remove.u128(),
            factory_contract.address().unwrap(),
            to_json_binary(&remove_msg).unwrap(),
        )
        .unwrap();
    println!("remove_request: {:?}", remove_request.events);
    relay_factory_router_factory(
        remove_request.events,
        &factory_contract,
        &router_contract,
        &factory_chain_uid,
    )
    .unwrap();

    let lp_token_balance_response = lp_token_contract
        .balance(factory_chain.sender.clone())
        .unwrap();
    let user_lp_balance = lp_token_balance_response.balance;
    assert_eq!(user_lp_balance, Uint128::zero());

    let liquidity_query: GetLiquidityQueryResponse = vlp_contract
        .query(&euclid::msgs::vlp::cp::QueryMsg::Liquidity {})
        .unwrap();
    assert_eq!(liquidity_query.total_lp_tokens, Uint128::new(1000));
}

#[test]
fn test_swap_request_ibc() {
    run_test_swap_request("osmosis", "nibiru");
}

#[test]
fn test_swap_request_native() {
    run_test_swap_request("nibiru", "nibiru");
}

fn run_test_swap_request(factory_chain_id: &str, router_chain_id: &str) {
    let sender = "sender_for_all_chains";
    let mut chains = vec![(factory_chain_id, sender)];
    if factory_chain_id != router_chain_id {
        chains.push((router_chain_id, sender));
    }
    let interchain = MockInterchainEnv::new(chains);
    let router_chain = interchain.get_chain(router_chain_id).unwrap();
    let router = setup_router(&router_chain, vec![factory_chain_id]).unwrap();

    let factory = setup_factory(&interchain, factory_chain_id, router_chain_id, &router).unwrap();

    run_test_swap_request_reusable(sender, &factory, &router, vec![]).unwrap();
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
    recipients: Vec<Recipient>,
) -> Result<SwapTestReusableOutput, CwEnvError> {
    let factory_chain = factory.environment();
    let router_chain = router.environment();

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

    let liquidity_query: GetLiquidityQueryResponse =
        vlp_contract.query(&euclid::msgs::vlp::cp::QueryMsg::Liquidity {})?;
    assert_eq!(
        liquidity_query,
        GetLiquidityQueryResponse {
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

    let swap_request_msg = factory.execute(
        &euclid::msgs::factory::ExecuteMsg::ExecuteSwapRequest(ExecuteSwapRequest {
            amount_in,
            asset_in: token_a.clone(),
            asset_out: token_b.token.clone(),
            min_amount_out: Uint128::new(50),
            swaps: vec![NextSwapPair {
                token_in: token_a.token.clone(),
                token_out: token_b.token.clone(),
                test_fail: None,
            }],
            recipients,
            partner_fee: None,
            cross_chain_config: CrossChainConfig::default(),
        }),
        &[coin(amount_in.u128(), token_a.token.to_string())],
    )?;

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
    let router = setup_router(&router_chain, vec![factory_chain_id]).unwrap();

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

        let liquidity_query: GetLiquidityQueryResponse = vlp_contract
            .query(&euclid::msgs::vlp::cp::QueryMsg::Liquidity {})
            .unwrap();
        assert_eq!(
            liquidity_query,
            GetLiquidityQueryResponse {
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
                asset_in: token_a.clone(),
                amount_in: Uint128::new(100),
                asset_out: token_c.token.clone(),
                min_amount_out: Uint128::new(50),
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
                recipients: vec![Recipient {
                    recipient: CrossChainUser::new(
                        ChainUid::create(factory_chain_id.to_string()).unwrap(),
                        random_user.to_string(),
                    ),
                    amount: Limit::GreaterThanOrEqual(Uint128::new(50)),
                    denom: token_c.token_type.clone(),
                    forwarding_message: None,
                    unsafe_refund_as_voucher: None,
                }],
                partner_fee: None,
                cross_chain_config: CrossChainConfig::default(),
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

    let router = setup_router(&router_chain, vec![factory_chain_id]).unwrap();
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
        asset_in,
        amount_in,
        Token::create("nibi".to_string()).unwrap(),
        Uint128::new(50),
        // Set swaps such that first_swap.token_in doesn’t match asset_in.token or
        // last_swap.token_out doesn’t match asset_out.
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

    let router = setup_router(&router_chain, vec!["osmosis", "nibiru"]).unwrap();
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
        asset_in,
        amount_in,
        Token::create("nibi".to_string()).unwrap(),
        Uint128::new(50),
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

    let router = setup_router(&router_chain, vec!["osmosis", "nibiru"]).unwrap();
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
        asset_in,
        amount_in,
        Token::create("nibi".to_string()).unwrap(),
        Uint128::new(50),
        vec![NextSwapPair {
            token_in: Token::create("eucl".to_string()).unwrap(),
            token_out: Token::create("nibi".to_string()).unwrap(),
            test_fail: None,
        }],
        vec![],
        None,
        funds,
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

    let router = setup_router(&router_chain, vec!["osmosis", "nibiru"]).unwrap();
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

    // swapping
    swap_request(
        &interchain,
        &factory,
        &router,
        asset_in,
        amount_in,
        Token::create("nibi".to_string()).unwrap(),
        Uint128::new(0),
        vec![NextSwapPair {
            token_in: Token::create("eucl".to_string()).unwrap(),
            token_out: Token::create("nibi".to_string()).unwrap(),
            test_fail: None,
        }],
        vec![],
        None,
        funds,
    )
    .unwrap();
}

#[test]
#[should_panic(expected = "Token in doesn't match swap route")]
fn test_swap_request_fails_for_invalid_swap_route() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = setup_router(&router_chain, vec!["osmosis", "nibiru"]).unwrap();
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
    let old_sender_balance = factory
        .environment()
        .query_balance(&Addr::unchecked(sender.clone()), "eucl")
        .unwrap();

    // swapping
    swap_request(
        &interchain,
        &factory,
        &router,
        asset_in,
        amount_in,
        Token::create("nibi".to_string()).unwrap(),
        Uint128::new(50),
        // Set swaps such that first_swap.token_in doesn’t match asset_in.token or
        // last_swap.token_out doesn’t match asset_out.
        vec![NextSwapPair {
            token_in: Token::create("osmo".to_string()).unwrap(),
            token_out: Token::create("nibi".to_string()).unwrap(),
            test_fail: None,
        }],
        vec![Recipient {
            recipient: CrossChainUser::new(
                ChainUid::create("nibiru".to_string()).unwrap(),
                sender.to_string(),
            ),
            amount: Limit::GreaterThanOrEqual(Uint128::one()),
            denom: TokenType::Native {
                denom: "nibi".to_string(),
            },
            forwarding_message: None,
            unsafe_refund_as_voucher: None,
        }],
        None,
        funds,
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

// #[test]
// #[should_panic(expected = "Invalid Timeout")]
// fn test_swap_request_fails_with_timeout_greater_than_240s() {
//     let sender = Addr::unchecked("sender_for_all_chains").into_string();
//     let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
//     let router_chain = interchain.get_chain("nibiru").unwrap();

//     let router = crate::helpers::chains::setup_router(&router_chain);
//     let factory = crate::helpers::chains::setup_factory(&interchain, "osmosis", "nibiru", &router);

//     let pair_info = PairWithDenomAndAmount {
//         token_1: TokenWithDenomAndAmount {
//             token: Token::create("eucl".to_string()).unwrap(),
//             amount: Uint128::from(10_000u128),
//             token_type: euclid::token::TokenType::Native {
//                 denom: "eucl".to_string(),
//             },
//         },
//         token_2: TokenWithDenomAndAmount {
//             token: Token::create("nibi".to_string()).unwrap(),
//             amount: Uint128::from(100_000u128),
//             token_type: euclid::token::TokenType::Native {
//                 denom: "nibi".to_string(),
//             },
//         },
//     };
//     register_token(&factory, &router, pair_info.token_1.to_token_with_denom());
//     create_pool(
//         &interchain,
//         &factory,
//         pair_info.clone(),
//         BPS_1_PERCENT,
//         PoolConfig::ConstantProduct {},
//     );

//     let asset_in = TokenWithDenom {
//         token: Token::create("eucl".to_string()).unwrap(),
//         token_type: euclid::token::TokenType::Native {
//             denom: "eucl".to_string(),
//         },
//     };

//     // adding funds
//     let chain = interchain
//         .get_chain(factory.environment().chain_id().as_str())
//         .unwrap();
//     let mut funds = vec![];
//     for token in pair_info.get_vec_token_info() {
//         faucet(
//             &chain,
//             chain.sender.as_str(),
//             token.amount.u128(),
//             token.token_type.clone(),
//             &mut funds,
//         );
//     }

//     add_liquidity(
//         &interchain,
//         &factory,
//         pair_info,
//         BPS_1_PERCENT,
//         None,
//         funds.clone(),
//     );

//     funds.clear();
//     faucet(
//         &chain,
//         chain.sender.as_str(),
//         1000,
//         asset_in.token_type.clone(),
//         &mut funds,
//     );

//     // swapping
//     swap_request(
//         &interchain,
//         &factory,
//         None,
//         asset_in,
//         Uint128::new(1000),
//         Token::create("nibi".to_string()).unwrap(),
//         Uint128::new(50),
//         Some(241),
//         // Set swaps such that first_swap.token_in doesn’t match asset_in.token or
//         // last_swap.token_out doesn’t match asset_out.
//         vec![NextSwapPair {
//             token_in: Token::create("eucl".to_string()).unwrap(),
//             token_out: Token::create("nibi".to_string()).unwrap(),
//             test_fail: None,
//         }],
//         vec![CrossChainUserWithLimit {
//             user: CrossChainUser {
//                 chain_uid: ChainUid::create("nibiru".to_string()).unwrap(),
//                 address: sender.clone(),
//             },
//             limit: None,
//             preferred_denom: None,
//             refund_address: None,
//             forwarding_message: None,
//         }],
//         None,
//         funds,
//         None,
//     );
// }

#[test]
fn test_stable_pool_swap_request_ibc() {
    run_test_stable_pool_swap_request("osmosis", "nibiru");
}

#[test]
fn test_stable_pool_swap_request_native() {
    run_test_stable_pool_swap_request("nibiru", "osmosis");
}

fn run_test_stable_pool_swap_request(factory_chain_id: &str, router_chain_id: &str) {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let mut chains = vec![(factory_chain_id, sender.as_str())];
    if factory_chain_id != router_chain_id {
        chains.push((router_chain_id, sender.as_str()));
    }
    let interchain = MockInterchainEnv::new(chains);
    let factory_chain = interchain.get_chain(factory_chain_id).unwrap();
    let factory_chain_uid = ChainUid::create(factory_chain_id.to_string()).unwrap();
    let router_chain = interchain.get_chain(router_chain_id).unwrap();
    let router = setup_router(&router_chain, vec![factory_chain_id]).unwrap();

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

    let liquidity_query: GetLiquidityQueryResponse = vlp_contract
        .query(&euclid::msgs::vlp::cp::QueryMsg::Liquidity {})
        .unwrap();
    assert_eq!(
        liquidity_query,
        GetLiquidityQueryResponse {
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
                asset_in: token_a.clone(),
                amount_in: Uint128::new(100),
                asset_out: token_b.token.clone(),
                min_amount_out: Uint128::new(50),
                swaps: vec![NextSwapPair {
                    token_in: token_a.token.clone(),
                    token_out: token_b.token.clone(),
                    test_fail: None,
                }],
                recipients: vec![],
                partner_fee: None,
                cross_chain_config: CrossChainConfig::default(),
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

#[test]
fn test_deposit_and_withdraw() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = setup_router(&router_chain, vec!["osmosis", "nibiru"]).unwrap();
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

    println!("Try deposit - ");
    deposit_token(
        &factory,
        &router,
        token.to_token_with_denom(),
        token.amount,
        vec![],
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

    println!("Try withdraw - ");

    // Withdraw tokens
    let withdraw_response = factory
        .execute(
            &euclid::msgs::factory::ExecuteMsg::TransferVoucher {
                token_id: token.token.clone(),
                amount: token.amount,
                recipients: vec![Recipient {
                    recipient: CrossChainUser::new(
                        factory_chain_uid.clone(),
                        factory.environment().sender.to_string(),
                    ),
                    amount: Limit::Equal(token.amount),
                    denom: TokenType::Native {
                        denom: "osmo".to_string(),
                    },
                    forwarding_message: None,
                    unsafe_refund_as_voucher: None,
                }],
                from: None,
                cross_chain_config: CrossChainConfig::default(),
            },
            &[],
        )
        .unwrap();

    println!("Relay deposit events - ");
    let relay_response = relay_factory_router_factory(
        withdraw_response.events,
        &factory,
        &router,
        &factory_chain_uid,
    )
    .unwrap();

    println!("Relay release events - ");
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
fn test_deposit_and_withdraw_with_failure() {
    let sender_label = String::from("sender_for_all_chains");
    let interchain =
        MockInterchainEnv::new(vec![("osmosis", &sender_label), ("nibiru", &sender_label)]);
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = setup_router(&router_chain, vec!["osmosis", "nibiru"]).unwrap();
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
                recipients: vec![],
                cross_chain_config: CrossChainConfig::default(),
            },
            &funds,
        )
        .unwrap();
    // Relay only send packet, no ack. This way escrow won't have funds and our withdraw will fail.
    relay_factory_send_packet(tx_response.events, &router).unwrap();

    let virtual_balance_contract = router.get_state().unwrap().virtual_balance_address;

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
            &euclid::msgs::factory::ExecuteMsg::TransferVoucher {
                token_id: token.token.clone(),
                amount: token.amount,
                recipients: vec![Recipient {
                    recipient: new_recipient.clone(),
                    amount: Limit::Equal(token.amount),
                    denom: TokenType::Native {
                        denom: "osmo".to_string(),
                    },
                    forwarding_message: None,
                    unsafe_refund_as_voucher: None,
                }],
                from: None,
                cross_chain_config: CrossChainConfig::default(),
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
        "Vcoin not refunded to original sender (unsafe send to recipient false)"
    );

    // Withdraw tokens
    let withdraw_response = factory
        .execute(
            &euclid::msgs::factory::ExecuteMsg::TransferVoucher {
                token_id: token.token.clone(),
                amount: token.amount,
                recipients: vec![Recipient {
                    recipient: new_recipient.clone(),
                    amount: Limit::Equal(token.amount),
                    denom: TokenType::Native {
                        denom: "osmo".to_string(),
                    },
                    forwarding_message: None,
                    unsafe_refund_as_voucher: Some(true),
                }],
                from: None,
                cross_chain_config: CrossChainConfig::default(),
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
        "Vcoin refunded  to original sender after failed withdraw (unsafe send to recipient true)",
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
        "Vcoin refunded to recipient after failed withdraw (unsafe send to recipient true)",
    );
}
