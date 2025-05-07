#![cfg(not(target_arch = "wasm32"))]
use std::collections::HashMap;

use cosmwasm_std::{coin, Addr, Coin, Uint128};
use cw20::Cw20Contract;
use cw_orch::prelude::{
    ContractInstance, CwOrchExecute, CwOrchInstantiate, CwOrchQuery, CwOrchUpload, Environment,
};
use cw_orch_interchain::{prelude::*, InterchainEnv};
use escrow::{mock::mock_escrow, EscrowContract};
use euclid::chain::CrossChainUser;
use euclid::chain::CrossChainUserWithLimit;
use euclid::fee::MAX_PARTNER_FEE_BPS;
use euclid::fee::{PartnerFee, BPS_100_PERCENT};
use euclid::msgs::router::QueryMsgFns;
use euclid::pool::PoolConfig;
use euclid::swap::NextSwapPair;
use euclid::token::TokenType;
use euclid::{
    chain::ChainUid,
    error::ContractError,
    fee::{DenomFees, BPS_1_PERCENT},
    msgs::{
        escrow::StateResponse as EscrowStateResponse,
        factory::{
            AllPoolsResponse, ExecuteMsgFns, ExecuteSwapRequest, PoolVlpResponse, StateResponse,
        },
        router::{
            RegisterFactoryChainIbc, RegisterFactoryChainNative, TokenDenom, TokenDenomsResponse,
            VlpResponse,
        },
        virtual_balance::{GetBalanceResponse, GetStateResponse},
        vlp::GetLiquidityResponse,
    },
    token::{Pair, PairWithDenomAndAmount, Token, TokenWithDenom, TokenWithDenomAndAmount},
    virtual_balance::BalanceKey,
};
use factory::{
    mock::{mock_factory, MockFactory},
    FactoryContract,
};
use mock::{mock::mock_app, mock_builder::MockEuclidBuilder};
use router::RouterContract;
use stable_vlp::StableVlpContract;
use virtual_balance::VirtualBalanceContract;
use vlp::VlpContract;

use crate::helpers::chains::get_escrow;
use crate::helpers::chains::get_virtual_balance;
use crate::helpers::chains::get_vlp;
use crate::helpers::chains::setup_factory;
use crate::helpers::chains::setup_router;
use crate::helpers::factory::{add_liquidity, create_pool, faucet, register_token, swap_request};
use crate::helpers::relayer::relay_factory_router_factory;
use crate::helpers::relayer::relay_router_factory_router;

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

    router
        .set_balance(
            sender.clone(),
            vec![
                Coin::new(100000000000000, token_a_id.clone()),
                Coin::new(100000000000000, token_b_id.clone()),
            ],
        )
        .unwrap();

    factory
        .set_balance(
            sender.clone(),
            vec![
                Coin::new(100000000000000, token_b_id.clone()),
                Coin::new(100000000000000, token_a_id.clone()),
            ],
        )
        .unwrap();

    let router_contract = setup_router(&router);
    let router_state = router_contract.get_state().unwrap();
    let virtual_balance_router =
        get_virtual_balance(&router, &router_state.virtual_balance_address.unwrap());

    let factory_chain_uid = ChainUid::create(factory_chain_id.to_string()).unwrap();
    let factory_contract = setup_factory(
        &interchain,
        factory_chain_id,
        router_chain_id,
        &router_contract,
    );

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
            &euclid::msgs::factory::ExecuteMsg::RequestRegisterDenom {
                token: token_a.clone(),
                timeout: None,
            },
            None,
        )
        .unwrap();

    relay_factory_router_factory(
        register_escrow_request.events,
        &factory_contract,
        &router_contract,
        &factory_chain_uid,
    );

    // let _ = interchain
    //     .await_packets("osmosis", register_escrow_request)
    //     .unwrap();

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
        None, // Some(&[coin(0u128, "osmo"), coin(0u128, "eucl")]),
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
            Some(&[
                coin(10_000u128, token_a.token.to_string()),
                coin(100_000u128, token_b.token.to_string()),
            ]),
        )
        .unwrap();

    relay_factory_router_factory(
        create_pool_with_funds_request.events,
        &factory_contract,
        &router_contract,
        &factory_chain_uid,
    );

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
            total_lp_tokens: Uint128::new(30622),
        }
    );

    let _vbalance_query: GetStateResponse = virtual_balance_router
        .query(&euclid::msgs::virtual_balance::QueryMsg::GetState {})
        .unwrap();

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
            Some(&[
                coin(10_000u128, token_a.token.to_string()),
                coin(100_000u128, token_b.token.to_string()),
            ]),
        )
        .unwrap();

    relay_factory_router_factory(
        add_liquidity_request.events,
        &factory_contract,
        &router_contract,
        &factory_chain_uid,
    );

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
            total_lp_tokens: Uint128::new(30622u128 * 2),
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
fn test_add_liquidity() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let osmosis = interchain.get_chain("osmosis").unwrap();
    let nibiru = interchain.get_chain("nibiru").unwrap();

    osmosis
        .set_balance(
            sender.clone(),
            vec![
                Coin::new(100000000000000, "osmo"),
                Coin::new(100000000000000, "eucl"),
            ],
        )
        .unwrap();

    nibiru
        .set_balance(
            sender.clone(),
            vec![
                Coin::new(100000000000000, "nibi"),
                Coin::new(100000000000000, "eucl"),
            ],
        )
        .unwrap();

    let router_nibiru = setup_router(&nibiru);
    let router_state = router_nibiru.get_state().unwrap();

    let osmosis_chain_uid = ChainUid::create("osmosis".to_string()).unwrap();
    let factory_osmosis = setup_factory(&interchain, "osmosis", "nibiru", &router_nibiru);

    // // Register escrow
    let register_escrow_request = factory_osmosis
        .execute(
            &euclid::msgs::factory::ExecuteMsg::RequestRegisterDenom {
                token: TokenWithDenom {
                    token: Token::create("osmo".to_string()).unwrap(),
                    token_type: euclid::token::TokenType::Native {
                        denom: "osmo".to_string(),
                    },
                },
                timeout: None,
            },
            None,
        )
        .unwrap();

    relay_factory_router_factory(
        register_escrow_request.events,
        &factory_osmosis,
        &router_nibiru,
        &osmosis_chain_uid,
    );

    // let _ = interchain
    //     .await_packets("osmosis", register_escrow_request)
    //     .unwrap();

    let token_denoms_response: TokenDenomsResponse = router_nibiru
        .query(&euclid::msgs::router::QueryMsg::QueryTokenDenoms {
            token: Token::create("osmo".to_string()).unwrap(),
        })
        .unwrap();

    assert_eq!(
        token_denoms_response,
        TokenDenomsResponse {
            denoms: vec![TokenDenom {
                chain_uid: ChainUid::create("osmosis".to_string()).unwrap(),
                token_type: euclid::token::TokenType::Native {
                    denom: "osmo".to_string(),
                },
            }],
        }
    );

    // create pool with funds
    let create_pool_with_funds_request = factory_osmosis
        .execute(
            &euclid::msgs::factory::ExecuteMsg::RequestPoolCreation {
                pair: PairWithDenomAndAmount {
                    token_1: TokenWithDenomAndAmount {
                        token: Token::create("eucl".to_string()).unwrap(),
                        amount: Uint128::from(10_000u128),
                        token_type: euclid::token::TokenType::Native {
                            denom: "eucl".to_string(),
                        },
                    },
                    token_2: TokenWithDenomAndAmount {
                        token: Token::create("osmo".to_string()).unwrap(),
                        amount: Uint128::from(100_000u128),
                        token_type: euclid::token::TokenType::Native {
                            denom: "osmo".to_string(),
                        },
                    },
                },
                slippage_tolerance_bps: 100,
                timeout: None,
                lp_token_name: "osmosis".to_string(),
                lp_token_symbol: "osmo".to_string(),
                lp_token_decimal: 6,
                lp_token_marketing: None,
                pool_config: PoolConfig::ConstantProduct {},
            },
            Some(&[coin(100_000u128, "osmo"), coin(10_000u128, "eucl")]),
        )
        .unwrap();

    relay_factory_router_factory(
        create_pool_with_funds_request.events,
        &factory_osmosis,
        &router_nibiru,
        &osmosis_chain_uid,
    );

    // let packet_lifetime = interchain
    //     .await_packets("osmosis", create_pool_with_funds_request)
    //     .unwrap();

    // // For testing a successful outcome of the first packet sent out in the tx, you can use:
    // if let IbcPacketOutcome::Success { ack_tx, .. } = &packet_lifetime.packets[0].outcome {
    //     println!("{:?}", ack_tx.tx_id.response.events);
    //     // Packet has been successfully acknowledged and decoded, the transaction has gone through correctly
    // } else {
    //     panic!("packet timed out");
    //     // There was a decode error or the packet timed out
    //     // Else the packet timed-out, you may have a relayer error or something is wrong in your application
    // };

    let all_pools_query: AllPoolsResponse = factory_osmosis
        .query(&euclid::msgs::factory::QueryMsg::GetAllPools {})
        .unwrap();
    assert_eq!(
        all_pools_query,
        AllPoolsResponse {
            pools: vec![PoolVlpResponse {
                pair: Pair::new(
                    Token::create("eucl".to_string()).unwrap(),
                    Token::create("osmo".to_string()).unwrap(),
                )
                .unwrap(),
                vlp: Addr::unchecked("contract2").into_string(),
            }],
        }
    );

    let vlp_query: VlpResponse = router_nibiru
        .query(&euclid::msgs::router::QueryMsg::GetVlp {
            pair: Pair::new(
                Token::create("osmo".to_string()).unwrap(),
                Token::create("eucl".to_string()).unwrap(),
            )
            .unwrap(),
        })
        .unwrap();
    assert_eq!(
        vlp_query,
        VlpResponse {
            vlp: Addr::unchecked("contract2").into_string(),
            token_1: Token::create("eucl".to_string()).unwrap(),
            token_2: Token::create("osmo".to_string()).unwrap(),
        }
    );

    // Got this address from the query above
    let vlp_nibiru = get_vlp(&nibiru, &Addr::unchecked(vlp_query.vlp));

    let liquidity_query: GetLiquidityResponse = vlp_nibiru
        .query(&euclid::msgs::vlp::QueryMsg::Liquidity {})
        .unwrap();
    assert_eq!(
        liquidity_query,
        GetLiquidityResponse {
            pair: Pair {
                token_1: Token::create("eucl".to_string()).unwrap(),
                token_2: Token::create("osmo".to_string()).unwrap(),
            },
            token_1_reserve: Uint128::new(10_000),
            token_2_reserve: Uint128::new(100_000),
            total_lp_tokens: Uint128::new(30622),
        }
    );
    let virtual_balance_nibiru =
        get_virtual_balance(&nibiru, &router_state.virtual_balance_address.unwrap());

    let vbalance_query: GetStateResponse = virtual_balance_nibiru
        .query(&euclid::msgs::virtual_balance::QueryMsg::GetState {})
        .unwrap();

    println!("vbalance state is: {:?}", vbalance_query);

    // Osmo escrow contract
    let escrow_osmosis = get_escrow(&factory_osmosis, "osmo");
    let escrow_query: EscrowStateResponse = escrow_osmosis
        .query(&euclid::msgs::escrow::QueryMsg::State {})
        .unwrap();
    assert_eq!(
        escrow_query,
        EscrowStateResponse {
            token: Token::create("osmo".to_string()).unwrap(),
            factory_address: Addr::unchecked("contract0"),
            total_amount: Uint128::from(100_000u128),
        }
    );

    // This is the escrow for the Euclid token
    escrow_osmosis.set_address(&Addr::unchecked("contract2"));
    let escrow_query: EscrowStateResponse = escrow_osmosis
        .query(&euclid::msgs::escrow::QueryMsg::State {})
        .unwrap();
    assert_eq!(
        escrow_query,
        EscrowStateResponse {
            token: Token::create("eucl".to_string()).unwrap(),
            factory_address: Addr::unchecked("contract0"),
            total_amount: Uint128::from(10_000u128),
        }
    );

    // Add Liquidity
    // Need to request register escrow first
    let add_liquidity_request = factory_osmosis
        .execute(
            &euclid::msgs::factory::ExecuteMsg::AddLiquidityRequest {
                pair_info: PairWithDenomAndAmount {
                    token_1: TokenWithDenomAndAmount {
                        token: Token::create("eucl".to_string()).unwrap(),
                        amount: Uint128::from(10_000u128),
                        token_type: euclid::token::TokenType::Native {
                            denom: "eucl".to_string(),
                        },
                    },
                    token_2: TokenWithDenomAndAmount {
                        token: Token::create("osmo".to_string()).unwrap(),
                        amount: Uint128::from(100_000u128),
                        token_type: euclid::token::TokenType::Native {
                            denom: "osmo".to_string(),
                        },
                    },
                },
                slippage_tolerance_bps: 100, // 1% slippage tolerance
                timeout: None,               // 10 minutes in seconds
            },
            Some(&[coin(100_000u128, "osmo"), coin(10_000u128, "eucl")]),
        )
        .unwrap();

    relay_factory_router_factory(
        add_liquidity_request.events,
        &factory_osmosis,
        &router_nibiru,
        &osmosis_chain_uid,
    );

    // let packet_lifetime = interchain
    //     .await_packets("osmosis", add_liquidity_request)
    //     .unwrap();

    // // For testing a successful outcome of the first packet sent out in the tx, you can use:
    // if let IbcPacketOutcome::Success { .. } = &packet_lifetime.packets[0].outcome {
    //     // Packet has been successfully acknowledged and decoded, the transaction has gone through correctly
    // } else {
    //     panic!("packet timed out");
    //     // There was a decode error or the packet timed out
    //     // Else the packet timed-out, you may have a relayer error or something is wrong in your application
    // };
    let liquidity_query: GetLiquidityResponse = vlp_nibiru
        .query(&euclid::msgs::vlp::QueryMsg::Liquidity {})
        .unwrap();
    assert_eq!(
        liquidity_query,
        GetLiquidityResponse {
            pair: Pair {
                token_1: Token::create("eucl".to_string()).unwrap(),
                token_2: Token::create("osmo".to_string()).unwrap(),
            },
            token_1_reserve: Uint128::new(10_000u128 * 2),
            token_2_reserve: Uint128::new(100_000u128 * 2),
            total_lp_tokens: Uint128::new(30622u128 * 2),
        }
    );
    // Euclid escrow contract
    let escrow_query: EscrowStateResponse = escrow_osmosis
        .query(&euclid::msgs::escrow::QueryMsg::State {})
        .unwrap();
    assert_eq!(
        escrow_query,
        EscrowStateResponse {
            token: Token::create("eucl".to_string()).unwrap(),
            factory_address: Addr::unchecked("contract0"),
            total_amount: Uint128::from(10_000u128 * 2),
        }
    );
    // Osmo escrow contract
    escrow_osmosis.set_address(&Addr::unchecked("contract1"));
    let escrow_query: EscrowStateResponse = escrow_osmosis
        .query(&euclid::msgs::escrow::QueryMsg::State {})
        .unwrap();
    assert_eq!(
        escrow_query,
        EscrowStateResponse {
            token: Token::create("osmo".to_string()).unwrap(),
            factory_address: Addr::unchecked("contract0"),
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

    let router = crate::helpers::chains::setup_router(&router_chain);
    let factory = crate::helpers::chains::setup_factory(&interchain, "osmosis", "nibiru", &router);

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
    register_token(&factory, &router, pair_info.token_1.to_token_with_denom());
    create_pool(
        &interchain,
        &factory,
        &router,
        pair_info.clone(),
        BPS_1_PERCENT,
        PoolConfig::ConstantProduct {},
    );

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

    add_liquidity(&interchain, &factory, &router, pair_info, 0, None, funds);
}

#[test]
#[should_panic(expected = "Pool doesn't exist for this chain")]
fn test_add_liquidity_fails_when_pool_does_not_exit() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = crate::helpers::chains::setup_router(&router_chain);
    let factory = crate::helpers::chains::setup_factory(&interchain, "osmosis", "nibiru", &router);

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
    register_token(&factory, &router, pair_info.token_1.to_token_with_denom());

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
    );
}

#[test]
#[should_panic(expected = "Amount cannot be zero")]
fn test_add_liquidity_fails_with_zero_liquidity_amount() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = crate::helpers::chains::setup_router(&router_chain);
    let factory = crate::helpers::chains::setup_factory(&interchain, "osmosis", "nibiru", &router);

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
    register_token(&factory, &router, pair_info.token_1.to_token_with_denom());

    create_pool(
        &interchain,
        &factory,
        &router,
        pair_info.clone(),
        BPS_1_PERCENT,
        PoolConfig::ConstantProduct {},
    );

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
    );
}

#[test]
#[should_panic(expected = "The deposit amount is insufficient to add the liquidity")]
fn test_add_liquidity_fails_with_insufficient_deposit() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = crate::helpers::chains::setup_router(&router_chain);
    let factory = crate::helpers::chains::setup_factory(&interchain, "osmosis", "nibiru", &router);

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
    register_token(&factory, &router, pair_info.token_1.to_token_with_denom());

    create_pool(
        &interchain,
        &factory,
        &router,
        pair_info.clone(),
        BPS_1_PERCENT,
        PoolConfig::ConstantProduct {},
    );

    add_liquidity(
        &interchain,
        &factory,
        &router,
        pair_info,
        BPS_100_PERCENT,
        None,
        vec![],
    );
}

#[test]
#[should_panic(expected = "UnsupportedDenomination")]
fn test_add_liquidity_fails_with_unsupported_token_denomination() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = crate::helpers::chains::setup_router(&router_chain);
    let factory = crate::helpers::chains::setup_factory(&interchain, "osmosis", "nibiru", &router);

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
    register_token(&factory, &router, pair_info.token_1.to_token_with_denom());

    // Attempt to add liquidity with tokens that aren't allowed by the escrow.
    pair_info.token_1.token_type = TokenType::Native {
        denom: "osmo".to_string(),
    };

    create_pool(
        &interchain,
        &factory,
        &router,
        pair_info.clone(),
        BPS_1_PERCENT,
        PoolConfig::ConstantProduct {},
    );

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
    );
}

#[test]
#[should_panic(expected = "Extra funds are not allowed")]
fn test_add_liquidity_fails_with_extra_funds() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = crate::helpers::chains::setup_router(&router_chain);
    let factory = crate::helpers::chains::setup_factory(&interchain, "osmosis", "nibiru", &router);

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
    register_token(&factory, &router, pair_info.token_1.to_token_with_denom());

    create_pool(
        &interchain,
        &factory,
        &router,
        pair_info.clone(),
        BPS_1_PERCENT,
        PoolConfig::ConstantProduct {},
    );

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
    );
}

#[test]
fn test_add_liquidity_with_timeout() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = crate::helpers::chains::setup_router(&router_chain);
    let factory = crate::helpers::chains::setup_factory(&interchain, "osmosis", "nibiru", &router);

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
    register_token(&factory, &router, pair_info.token_1.to_token_with_denom());

    create_pool(
        &interchain,
        &factory,
        &router,
        pair_info.clone(),
        BPS_1_PERCENT,
        PoolConfig::ConstantProduct {},
    );

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
    );
}

#[test]
#[should_panic(expected = "Invalid Timeout")]
fn test_add_liquidity_with_invalid_timeout() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = crate::helpers::chains::setup_router(&router_chain);
    let factory = crate::helpers::chains::setup_factory(&interchain, "osmosis", "nibiru", &router);

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
    register_token(&factory, &router, pair_info.token_1.to_token_with_denom());

    create_pool(
        &interchain,
        &factory,
        &router,
        pair_info.clone(),
        BPS_1_PERCENT,
        PoolConfig::ConstantProduct {},
    );

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
    );
}
#[test]
fn test_swap_request() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let osmosis = interchain.get_chain("osmosis").unwrap();
    let nibiru = interchain.get_chain("nibiru").unwrap();

    osmosis
        .set_balance(
            sender.clone(),
            vec![
                Coin::new(100000000000000, "osmo"),
                Coin::new(100000000000000, "eucl"),
            ],
        )
        .unwrap();

    nibiru
        .set_balance(
            sender.clone(),
            vec![
                Coin::new(100000000000000, "nibi"),
                Coin::new(100000000000000, "eucl"),
            ],
        )
        .unwrap();

    let factory_osmosis = FactoryContract::new(osmosis.clone());
    let escrow_osmosis = EscrowContract::new(osmosis.clone());
    let cw20_osmosis = Cw20Contract::new(osmosis.clone());
    let router_nibiru = RouterContract::new(nibiru.clone());
    let virtual_balance_nibiru = VirtualBalanceContract::new(nibiru.clone());
    let vlp_nibiru = VlpContract::new(nibiru.clone());

    factory_osmosis.upload().unwrap();
    escrow_osmosis.upload().unwrap();
    cw20_osmosis.upload().unwrap();
    router_nibiru.upload().unwrap();
    virtual_balance_nibiru.upload().unwrap();
    vlp_nibiru.upload().unwrap();

    router_nibiru
        .instantiate(
            &euclid::msgs::router::InstantiateMsg {
                constant_product_vlp_code_id: 3,
                virtual_balance_code_id: 2,
                mock_relayer_addresses: Some(vec![router_nibiru.environment().sender.to_string()]),
                stable_vlp_code_id: 4,
            },
            None,
            None,
        )
        .unwrap();

    let osmosis_chain_uid = ChainUid::create("osmosis".to_string()).unwrap();
    factory_osmosis
        .instantiate(
            &euclid::msgs::factory::InstantiateMsg {
                router_contract: router_nibiru.address().unwrap().into_string(),
                chain_uid: osmosis_chain_uid.clone(),
                escrow_code_id: 2,
                cw20_code_id: 3,
                is_native: false,
                mock_relayer_address: Some(factory_osmosis.environment().sender.to_string()),
            },
            None,
            None,
        )
        .unwrap();

    // Set up channel from osmosis to nibiru
    let channel_receipt = interchain
        .create_contract_channel(&factory_osmosis, &router_nibiru, "counter-1", None)
        .unwrap();

    // After channel creation is complete, we get the channel id, which is necessary for ICA remote execution
    let osmosis_channel = channel_receipt
        .interchain_channel
        .get_chain("osmosis")
        .unwrap()
        .channel
        .unwrap();

    // Update Hub Channel
    factory_osmosis
        .update_hub_channel(osmosis_channel.to_string())
        .unwrap();

    let register_factory_request = router_nibiru
        .execute(
            &euclid::msgs::router::ExecuteMsg::RegisterFactory {
                chain_uid: osmosis_chain_uid.clone(),
                chain_info: euclid::msgs::router::RegisterFactoryChainType::Ibc(
                    RegisterFactoryChainIbc {
                        channel: osmosis_channel.to_string(),
                        timeout: None,
                        factory_address: factory_osmosis.address().unwrap().into_string(),
                        factory_chain_id: factory_osmosis.environment().chain_id(),
                    },
                ),
            },
            None,
        )
        .unwrap();

    relay_router_factory_router(
        register_factory_request.events,
        &factory_osmosis,
        &osmosis_chain_uid,
        &router_nibiru,
    );

    // let _ = interchain
    //     .await_packets("nibiru", register_factory_request)
    //     .unwrap();

    // // Register escrow
    let register_escrow_request = factory_osmosis
        .execute(
            &euclid::msgs::factory::ExecuteMsg::RequestRegisterDenom {
                token: TokenWithDenom {
                    token: Token::create("osmo".to_string()).unwrap(),
                    token_type: euclid::token::TokenType::Native {
                        denom: "osmo".to_string(),
                    },
                },
                timeout: None,
            },
            None,
        )
        .unwrap();

    relay_factory_router_factory(
        register_escrow_request.events,
        &factory_osmosis,
        &router_nibiru,
        &osmosis_chain_uid,
    );
    // let _ = interchain
    //     .await_packets("osmosis", register_escrow_request)
    //     .unwrap();

    let token_denoms_response: TokenDenomsResponse = router_nibiru
        .query(&euclid::msgs::router::QueryMsg::QueryTokenDenoms {
            token: Token::create("osmo".to_string()).unwrap(),
        })
        .unwrap();

    assert_eq!(
        token_denoms_response,
        TokenDenomsResponse {
            denoms: vec![TokenDenom {
                chain_uid: ChainUid::create("osmosis".to_string()).unwrap(),
                token_type: euclid::token::TokenType::Native {
                    denom: "osmo".to_string(),
                },
            }],
        }
    );

    // create pool with funds
    let create_pool_with_funds_request = factory_osmosis
        .execute(
            &euclid::msgs::factory::ExecuteMsg::RequestPoolCreation {
                pair: PairWithDenomAndAmount {
                    token_1: TokenWithDenomAndAmount {
                        token: Token::create("eucl".to_string()).unwrap(),
                        amount: Uint128::from(10_000u128),
                        token_type: euclid::token::TokenType::Native {
                            denom: "eucl".to_string(),
                        },
                    },
                    token_2: TokenWithDenomAndAmount {
                        token: Token::create("osmo".to_string()).unwrap(),
                        amount: Uint128::from(100_000u128),
                        token_type: euclid::token::TokenType::Native {
                            denom: "osmo".to_string(),
                        },
                    },
                },
                slippage_tolerance_bps: 100,
                timeout: None,
                lp_token_name: "osmosis".to_string(),
                lp_token_symbol: "osmo".to_string(),
                lp_token_decimal: 6,
                lp_token_marketing: None,
                pool_config: PoolConfig::ConstantProduct {},
            },
            Some(&[coin(100_000u128, "osmo"), coin(10_000u128, "eucl")]),
        )
        .unwrap();

    relay_factory_router_factory(
        create_pool_with_funds_request.events,
        &factory_osmosis,
        &router_nibiru,
        &osmosis_chain_uid,
    );

    // let packet_lifetime = interchain
    //     .await_packets("osmosis", create_pool_with_funds_request)
    //     .unwrap();

    // // For testing a successful outcome of the first packet sent out in the tx, you can use:
    // if let IbcPacketOutcome::Success { ack_tx, .. } = &packet_lifetime.packets[0].outcome {
    //     println!("{:?}", ack_tx.tx_id.response.events);
    //     // Packet has been successfully acknowledged and decoded, the transaction has gone through correctly
    // } else {
    //     panic!("packet timed out");
    //     // There was a decode error or the packet timed out
    //     // Else the packet timed-out, you may have a relayer error or something is wrong in your application
    // };

    let all_pools_query: AllPoolsResponse = factory_osmosis
        .query(&euclid::msgs::factory::QueryMsg::GetAllPools {})
        .unwrap();
    assert_eq!(
        all_pools_query,
        AllPoolsResponse {
            pools: vec![PoolVlpResponse {
                pair: Pair::new(
                    Token::create("eucl".to_string()).unwrap(),
                    Token::create("osmo".to_string()).unwrap(),
                )
                .unwrap(),
                vlp: Addr::unchecked("contract2").into_string(),
            }],
        }
    );

    let vlp_query: VlpResponse = router_nibiru
        .query(&euclid::msgs::router::QueryMsg::GetVlp {
            pair: Pair::new(
                Token::create("osmo".to_string()).unwrap(),
                Token::create("eucl".to_string()).unwrap(),
            )
            .unwrap(),
        })
        .unwrap();
    assert_eq!(
        vlp_query,
        VlpResponse {
            vlp: Addr::unchecked("contract2").into_string(),
            token_1: Token::create("eucl".to_string()).unwrap(),
            token_2: Token::create("osmo".to_string()).unwrap(),
        }
    );

    // Got this address from the query above
    vlp_nibiru.set_address(&Addr::unchecked("contract2"));

    let liquidity_query: GetLiquidityResponse = vlp_nibiru
        .query(&euclid::msgs::vlp::QueryMsg::Liquidity {})
        .unwrap();
    assert_eq!(
        liquidity_query,
        GetLiquidityResponse {
            pair: Pair {
                token_1: Token::create("eucl".to_string()).unwrap(),
                token_2: Token::create("osmo".to_string()).unwrap(),
            },
            token_1_reserve: Uint128::new(10_000),
            token_2_reserve: Uint128::new(100_000),
            total_lp_tokens: Uint128::new(30622),
        }
    );
    virtual_balance_nibiru.set_address(&Addr::unchecked("contract1"));

    let vbalance_query: GetStateResponse = virtual_balance_nibiru
        .query(&euclid::msgs::virtual_balance::QueryMsg::GetState {})
        .unwrap();

    println!("vbalance state is: {:?}", vbalance_query);

    // Osmo escrow contract
    escrow_osmosis.set_address(&Addr::unchecked("contract1"));
    let escrow_query: EscrowStateResponse = escrow_osmosis
        .query(&euclid::msgs::escrow::QueryMsg::State {})
        .unwrap();
    assert_eq!(
        escrow_query,
        EscrowStateResponse {
            token: Token::create("osmo".to_string()).unwrap(),
            factory_address: Addr::unchecked("contract0"),
            total_amount: Uint128::from(100_000u128),
        }
    );

    // This is the escrow for the Euclid token
    escrow_osmosis.set_address(&Addr::unchecked("contract2"));
    let escrow_query: EscrowStateResponse = escrow_osmosis
        .query(&euclid::msgs::escrow::QueryMsg::State {})
        .unwrap();
    assert_eq!(
        escrow_query,
        EscrowStateResponse {
            token: Token::create("eucl".to_string()).unwrap(),
            factory_address: Addr::unchecked("contract0"),
            total_amount: Uint128::from(10_000u128),
        }
    );

    let swap_request_msg = factory_osmosis
        .execute(
            &euclid::msgs::factory::ExecuteMsg::ExecuteSwapRequest(ExecuteSwapRequest {
                sender: None,
                asset_in: TokenWithDenom {
                    token: Token::create("eucl".to_string()).unwrap(),
                    token_type: euclid::token::TokenType::Native {
                        denom: "eucl".to_string(),
                    },
                },
                amount_in: Uint128::new(100),
                asset_out: Token::create("nibi".to_string()).unwrap(),
                min_amount_out: Uint128::new(50),
                timeout: None,
                swaps: vec![NextSwapPair {
                    token_in: Token::create("eucl".to_string()).unwrap(),
                    token_out: Token::create("nibi".to_string()).unwrap(),
                    test_fail: None,
                }],
                cross_chain_addresses: vec![CrossChainUserWithLimit {
                    user: CrossChainUser::new(
                        ChainUid::create("nibiru".to_string()).unwrap(),
                        sender.clone(),
                    ),
                    limit: None,
                    preferred_denom: None,
                    refund_address: None,
                    forwarding_message: None,
                }],
                partner_fee: None,
                meta: None,
            }),
            Some(&[coin(100u128, "eucl")]),
        )
        .unwrap();

    relay_factory_router_factory(
        swap_request_msg.events,
        &factory_osmosis,
        &router_nibiru,
        &osmosis_chain_uid,
    );
    // let packet_lifetime = interchain
    //     .await_packets("osmosis", swap_request_msg)
    //     .unwrap();

    // // For testing a successful outcome of the first packet sent out in the tx, you can use:
    // if let IbcPacketOutcome::Success { .. } = &packet_lifetime.packets[0].outcome {
    //     // Packet has been successfully acknowledged and decoded, the transaction has gone through correctly
    // } else {
    //     panic!("packet timed out");
    //     // There was a decode error or the packet timed out
    //     // Else the packet timed-out, you may have a relayer error or something is wrong in your application
    // };
}

#[test]
fn test_swap_request_with_valid_partner_fee() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let partner_fee_recipient = Addr::unchecked("partner_fee_recipient").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = crate::helpers::chains::setup_router(&router_chain);
    let factory = crate::helpers::chains::setup_factory(&interchain, "osmosis", "nibiru", &router);

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
    let old_partner_eucl_balance = factory
        .environment()
        .query_balance(partner_fee_recipient.clone(), "eucl")
        .unwrap();
    register_token(&factory, &router, pair_info.token_1.to_token_with_denom());
    create_pool(
        &interchain,
        &factory,
        &router,
        pair_info.clone(),
        BPS_1_PERCENT,
        PoolConfig::ConstantProduct {},
    );

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
    );

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
        None,
    );

    let new_partner_eucl_balance = factory
        .environment()
        .query_balance(partner_fee_recipient.clone(), "eucl")
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

    let router = crate::helpers::chains::setup_router(&router_chain);
    let factory = crate::helpers::chains::setup_factory(&interchain, "osmosis", "nibiru", &router);

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
    register_token(&factory, &router, pair_info.token_1.to_token_with_denom());
    create_pool(
        &interchain,
        &factory,
        &router,
        pair_info.clone(),
        BPS_1_PERCENT,
        PoolConfig::ConstantProduct {},
    );

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
    );

    funds.clear();
    faucet(
        &chain,
        chain.sender.as_str(),
        1000,
        asset_in.token_type.clone(),
        &mut funds,
    );

    // Providing a partner_fee_bps above MAX_PARTNER_FEE_BPS
    let partner_fee_recipient = Addr::unchecked("partner_fee_recipient").into_string();
    let old_partner_eucl_balance = factory
        .environment()
        .query_balance(partner_fee_recipient.clone(), "eucl")
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
    );

    let new_partner_eucl_balance = factory
        .environment()
        .query_balance(partner_fee_recipient.clone(), "eucl")
        .unwrap();
    assert_eq!(new_partner_eucl_balance, old_partner_eucl_balance);
}

#[test]
#[should_panic(expected = "UnsupportedDenomination")]
fn test_swap_request_fails_for_unsupported_denomination_for_asset_in() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = crate::helpers::chains::setup_router(&router_chain);
    let factory = crate::helpers::chains::setup_factory(&interchain, "osmosis", "nibiru", &router);

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
    register_token(&factory, &router, pair_info.token_1.to_token_with_denom());
    create_pool(
        &interchain,
        &factory,
        &router,
        pair_info.clone(),
        BPS_1_PERCENT,
        PoolConfig::ConstantProduct {},
    );

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
    );

    funds.clear();
    let amount_in = Uint128::new(1000);
    faucet(
        &chain,
        chain.sender.as_str(),
        amount_in.u128(),
        asset_in.token_type.clone(),
        &mut funds,
    );

    let old_sender_eucl_balance = factory
        .environment()
        .query_balance(sender.clone(), "juno")
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
    );

    let new_sender_eucl_balance = factory
        .environment()
        .query_balance(sender.clone(), "juno")
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

    let router = crate::helpers::chains::setup_router(&router_chain);
    let factory = crate::helpers::chains::setup_factory(&interchain, "osmosis", "nibiru", &router);

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
    register_token(&factory, &router, pair_info.token_1.to_token_with_denom());
    create_pool(
        &interchain,
        &factory,
        &router,
        pair_info.clone(),
        BPS_1_PERCENT,
        PoolConfig::ConstantProduct {},
    );

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
    );

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
    );
}

#[test]
#[should_panic(expected = "Token in doesn't match swap route")]
fn test_swap_request_fails_for_invalid_swap_route() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = crate::helpers::chains::setup_router(&router_chain);
    let factory = crate::helpers::chains::setup_factory(&interchain, "osmosis", "nibiru", &router);

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
    register_token(&factory, &router, pair_info.token_1.to_token_with_denom());
    create_pool(
        &interchain,
        &factory,
        &router,
        pair_info.clone(),
        BPS_1_PERCENT,
        PoolConfig::ConstantProduct {},
    );

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
    );

    funds.clear();
    faucet(
        &chain,
        chain.sender.as_str(),
        1000,
        asset_in.token_type.clone(),
        &mut funds,
    );

    let old_sender_balance = factory
        .environment()
        .query_balance(sender.clone(), "eucl")
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
        // Set swaps such that first_swap.token_in doesn’t match asset_in.token or
        // last_swap.token_out doesn’t match asset_out.
        vec![NextSwapPair {
            token_in: Token::create("osmo".to_string()).unwrap(),
            token_out: Token::create("nibi".to_string()).unwrap(),
            test_fail: None,
        }],
        vec![CrossChainUserWithLimit {
            user: CrossChainUser::new(
                ChainUid::create("nibiru".to_string()).unwrap(),
                sender.clone(),
            ),
            limit: None,
            preferred_denom: None,
            refund_address: None,
            forwarding_message: None,
        }],
        None,
        funds,
        None,
    );
    let new_sender_balance = factory
        .environment()
        .query_balance(sender.clone(), "eucl")
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

    let router = crate::helpers::chains::setup_router(&router_chain);
    let factory = crate::helpers::chains::setup_factory(&interchain, "osmosis", "nibiru", &router);

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
    register_token(&factory, &router, pair_info.token_1.to_token_with_denom());
    create_pool(
        &interchain,
        &factory,
        &router,
        pair_info.clone(),
        BPS_1_PERCENT,
        PoolConfig::ConstantProduct {},
    );

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
    );

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
        // Set swaps such that first_swap.token_in doesn’t match asset_in.token or
        // last_swap.token_out doesn’t match asset_out.
        vec![NextSwapPair {
            token_in: Token::create("eucl".to_string()).unwrap(),
            token_out: Token::create("nibi".to_string()).unwrap(),
            test_fail: None,
        }],
        vec![],
        None,
        funds,
        None,
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
fn test_stable_pool() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let osmosis = interchain.get_chain("osmosis").unwrap();
    let nibiru = interchain.get_chain("nibiru").unwrap();

    osmosis
        .set_balance(
            sender.clone(),
            vec![
                Coin::new(100000000000000, "osmo"),
                Coin::new(100000000000000, "eucl"),
            ],
        )
        .unwrap();

    let factory_osmosis = FactoryContract::new(osmosis.clone());
    let escrow_osmosis = EscrowContract::new(osmosis.clone());
    let cw20_osmosis = Cw20Contract::new(osmosis.clone());
    let router_nibiru = RouterContract::new(nibiru.clone());
    let virtual_balance_nibiru = VirtualBalanceContract::new(nibiru.clone());
    let vlp_nibiru = VlpContract::new(nibiru.clone());
    let stable_vlp_nibiru = StableVlpContract::new(nibiru.clone());

    factory_osmosis.upload().unwrap();
    escrow_osmosis.upload().unwrap();
    cw20_osmosis.upload().unwrap();
    router_nibiru.upload().unwrap();
    virtual_balance_nibiru.upload().unwrap();
    vlp_nibiru.upload().unwrap();
    stable_vlp_nibiru.upload().unwrap();

    router_nibiru
        .instantiate(
            &euclid::msgs::router::InstantiateMsg {
                constant_product_vlp_code_id: vlp_nibiru.code_id().unwrap(),
                stable_vlp_code_id: stable_vlp_nibiru.code_id().unwrap(),
                virtual_balance_code_id: virtual_balance_nibiru.code_id().unwrap(),
                mock_relayer_addresses: Some(vec![router_nibiru.environment().sender.to_string()]),
            },
            None,
            None,
        )
        .unwrap();

    let factory_osmosis_chain_uid = ChainUid::create("osmosis".to_string()).unwrap();

    factory_osmosis
        .instantiate(
            &euclid::msgs::factory::InstantiateMsg {
                router_contract: router_nibiru.address().unwrap().into_string(),
                chain_uid: factory_osmosis_chain_uid.clone(),
                escrow_code_id: 2,
                cw20_code_id: 3,
                is_native: false,
                mock_relayer_address: Some(factory_osmosis.environment().sender.to_string()),
            },
            None,
            None,
        )
        .unwrap();

    // Set up channel from osmosis to nibiru
    let channel_receipt = interchain
        .create_contract_channel(&factory_osmosis, &router_nibiru, "counter-1", None)
        .unwrap();

    // After channel creation is complete, we get the channel id, which is necessary for ICA remote execution
    let osmosis_channel = channel_receipt
        .interchain_channel
        .get_chain("osmosis")
        .unwrap()
        .channel
        .unwrap();

    // Update Hub Channel
    factory_osmosis
        .update_hub_channel(osmosis_channel.to_string())
        .unwrap();

    let register_factory_request = router_nibiru
        .execute(
            &euclid::msgs::router::ExecuteMsg::RegisterFactory {
                chain_uid: factory_osmosis_chain_uid.clone(),
                chain_info: euclid::msgs::router::RegisterFactoryChainType::Ibc(
                    RegisterFactoryChainIbc {
                        channel: osmosis_channel.to_string(),
                        timeout: None,
                        factory_address: factory_osmosis.address().unwrap().into_string(),
                        factory_chain_id: factory_osmosis.environment().chain_id(),
                    },
                ),
            },
            None,
        )
        .unwrap();

    relay_router_factory_router(
        register_factory_request.events,
        &factory_osmosis,
        &factory_osmosis_chain_uid,
        &router_nibiru,
    );

    // let _ = interchain
    //     .await_packets("nibiru", register_factory_request)
    //     .unwrap();

    // // Register escrow
    let register_escrow_request = factory_osmosis
        .execute(
            &euclid::msgs::factory::ExecuteMsg::RequestRegisterDenom {
                token: TokenWithDenom {
                    token: Token::create("osmo".to_string()).unwrap(),
                    token_type: euclid::token::TokenType::Native {
                        denom: "osmo".to_string(),
                    },
                },
                timeout: None,
            },
            None,
        )
        .unwrap();

    relay_factory_router_factory(
        register_escrow_request.events,
        &factory_osmosis,
        &router_nibiru,
        &factory_osmosis_chain_uid,
    );

    // let _ = interchain
    //     .await_packets("osmosis", register_escrow_request)
    //     .unwrap();

    let token_denoms_response: TokenDenomsResponse = router_nibiru
        .query(&euclid::msgs::router::QueryMsg::QueryTokenDenoms {
            token: Token::create("osmo".to_string()).unwrap(),
        })
        .unwrap();

    assert_eq!(
        token_denoms_response,
        TokenDenomsResponse {
            denoms: vec![TokenDenom {
                chain_uid: ChainUid::create("osmosis".to_string()).unwrap(),
                token_type: euclid::token::TokenType::Native {
                    denom: "osmo".to_string(),
                },
            }],
        }
    );

    // Need to request register escrow first
    let create_pool_with_funds_request = factory_osmosis
        .execute(
            &euclid::msgs::factory::ExecuteMsg::RequestPoolCreation {
                pair: PairWithDenomAndAmount {
                    token_1: TokenWithDenomAndAmount {
                        token: Token::create("eucl".to_string()).unwrap(),
                        amount: Uint128::from(10_000u128),
                        token_type: euclid::token::TokenType::Native {
                            denom: "eucl".to_string(),
                        },
                    },
                    token_2: TokenWithDenomAndAmount {
                        token: Token::create("osmo".to_string()).unwrap(),
                        amount: Uint128::from(10_000u128),
                        token_type: euclid::token::TokenType::Native {
                            denom: "osmo".to_string(),
                        },
                    },
                },
                slippage_tolerance_bps: 100,
                timeout: None,
                lp_token_name: "osmosis".to_string(),
                lp_token_symbol: "osmo".to_string(),
                lp_token_decimal: 6,
                lp_token_marketing: None,
                pool_config: PoolConfig::Stable { amp_factor: None },
            },
            Some(&[coin(10_000u128, "osmo"), coin(10_000u128, "eucl")]),
        )
        .unwrap();

    relay_factory_router_factory(
        create_pool_with_funds_request.events,
        &factory_osmosis,
        &router_nibiru,
        &factory_osmosis_chain_uid,
    );

    // let packet_lifetime = interchain
    //     .await_packets("osmosis", create_pool_with_funds_request)
    //     .unwrap();

    // // For testing a successful outcome of the first packet sent out in the tx, you can use:
    // if let IbcPacketOutcome::Success { .. } = &packet_lifetime.packets[0].outcome {

    //     // Packet has been successfully acknowledged and decoded, the transaction has gone through correctly
    // } else {
    //     panic!("packet timed out");
    //     // There was a decode error or the packet timed out
    //     // Else the packet timed-out, you may have a relayer error or something is wrong in your application
    // };

    let all_pools_query: AllPoolsResponse = factory_osmosis
        .query(&euclid::msgs::factory::QueryMsg::GetAllPools {})
        .unwrap();
    assert_eq!(
        all_pools_query,
        AllPoolsResponse {
            pools: vec![PoolVlpResponse {
                pair: Pair::new(
                    Token::create("eucl".to_string()).unwrap(),
                    Token::create("osmo".to_string()).unwrap(),
                )
                .unwrap(),
                vlp: Addr::unchecked("contract2").into_string(),
            }],
        }
    );

    let vlp_query: VlpResponse = router_nibiru
        .query(&euclid::msgs::router::QueryMsg::GetVlp {
            pair: Pair::new(
                Token::create("osmo".to_string()).unwrap(),
                Token::create("eucl".to_string()).unwrap(),
            )
            .unwrap(),
        })
        .unwrap();
    assert_eq!(
        vlp_query,
        VlpResponse {
            vlp: Addr::unchecked("contract2").into_string(),
            token_1: Token::create("eucl".to_string()).unwrap(),
            token_2: Token::create("osmo".to_string()).unwrap(),
        }
    );

    // Got this address from the query above
    vlp_nibiru.set_address(&Addr::unchecked("contract2"));

    let liquidity_query: GetLiquidityResponse = vlp_nibiru
        .query(&euclid::msgs::vlp::QueryMsg::Liquidity {})
        .unwrap();
    assert_eq!(
        liquidity_query,
        GetLiquidityResponse {
            pair: Pair {
                token_1: Token::create("eucl".to_string()).unwrap(),
                token_2: Token::create("osmo".to_string()).unwrap(),
            },
            token_1_reserve: Uint128::new(10_000),
            token_2_reserve: Uint128::new(10_000),
            total_lp_tokens: Uint128::new(9000),
        }
    );
    virtual_balance_nibiru.set_address(&Addr::unchecked("contract1"));

    let vbalance_query: GetStateResponse = virtual_balance_nibiru
        .query(&euclid::msgs::virtual_balance::QueryMsg::GetState {})
        .unwrap();

    println!("vbalance state is: {:?}", vbalance_query);

    // Osmo escrow contract
    escrow_osmosis.set_address(&Addr::unchecked("contract1"));
    let escrow_query: EscrowStateResponse = escrow_osmosis
        .query(&euclid::msgs::escrow::QueryMsg::State {})
        .unwrap();
    assert_eq!(
        escrow_query,
        EscrowStateResponse {
            token: Token::create("osmo".to_string()).unwrap(),
            factory_address: Addr::unchecked("contract0"),
            total_amount: Uint128::from(10_000u128),
        }
    );

    // This is the escrow for the Euclid token
    escrow_osmosis.set_address(&Addr::unchecked("contract2"));
    let escrow_query: EscrowStateResponse = escrow_osmosis
        .query(&euclid::msgs::escrow::QueryMsg::State {})
        .unwrap();
    assert_eq!(
        escrow_query,
        EscrowStateResponse {
            token: Token::create("eucl".to_string()).unwrap(),
            factory_address: Addr::unchecked("contract0"),
            total_amount: Uint128::from(10_000u128),
        }
    );

    // Swap
    let swap_request = factory_osmosis
        .execute(
            &euclid::msgs::factory::ExecuteMsg::ExecuteSwapRequest(ExecuteSwapRequest {
                asset_in: TokenWithDenom {
                    token: Token::create("eucl".to_string()).unwrap(),
                    token_type: euclid::token::TokenType::Native {
                        denom: "eucl".to_string(),
                    },
                },
                amount_in: Uint128::from(1000u128),
                asset_out: Token::create("osmo".to_string()).unwrap(),
                min_amount_out: Uint128::new(900),
                timeout: None,
                swaps: vec![NextSwapPair {
                    token_in: Token::create("eucl".to_string()).unwrap(),
                    token_out: Token::create("osmo".to_string()).unwrap(),
                    test_fail: None,
                }],
                cross_chain_addresses: vec![],
                partner_fee: None,
                sender: Some(CrossChainUser::new(
                    ChainUid::create("osmosis".to_string()).unwrap(),
                    Addr::unchecked("sender_for_all_chains").into_string(),
                )),
                meta: None,
            }),
            Some(&[coin(1000u128, "eucl")]),
        )
        .unwrap();

    relay_factory_router_factory(
        swap_request.events,
        &factory_osmosis,
        &router_nibiru,
        &factory_osmosis_chain_uid,
    );

    // let packet_lifetime = interchain.await_packets("osmosis", swap_request).unwrap();

    // // For testing a successful outcome of the first packet sent out in the tx, you can use:
    // if let IbcPacketOutcome::Success { .. } = &packet_lifetime.packets[0].outcome {

    //     // Packet has been successfully acknowledged and decoded, the transaction has gone through correctly
    // } else {
    //     panic!("packet timed out");
    //     // There was a decode error or the packet timed out
    //     // Else the packet timed-out, you may have a relayer error or something is wrong in your application
    // };

    let liquidity_query: GetLiquidityResponse = vlp_nibiru
        .query(&euclid::msgs::vlp::QueryMsg::Liquidity {})
        .unwrap();
    assert_eq!(
        liquidity_query,
        GetLiquidityResponse {
            pair: Pair {
                token_1: Token::create("eucl".to_string()).unwrap(),
                token_2: Token::create("osmo".to_string()).unwrap(),
            },
            token_1_reserve: Uint128::new(10_999),
            token_2_reserve: Uint128::new(9011),
            total_lp_tokens: Uint128::new(9000),
        }
    );

    escrow_osmosis.set_address(&Addr::unchecked("contract1"));
    let escrow_query: EscrowStateResponse = escrow_osmosis
        .query(&euclid::msgs::escrow::QueryMsg::State {})
        .unwrap();
    assert_eq!(
        escrow_query,
        EscrowStateResponse {
            token: Token::create("osmo".to_string()).unwrap(),
            factory_address: Addr::unchecked("contract0"),
            total_amount: Uint128::from(10_000u128),
        }
    );

    // This is the escrow for the Euclid token
    escrow_osmosis.set_address(&Addr::unchecked("contract2"));
    let escrow_query: EscrowStateResponse = escrow_osmosis
        .query(&euclid::msgs::escrow::QueryMsg::State {})
        .unwrap();
    assert_eq!(
        escrow_query,
        EscrowStateResponse {
            token: Token::create("eucl".to_string()).unwrap(),
            factory_address: Addr::unchecked("contract0"),
            total_amount: Uint128::from(11_000u128),
        }
    );
}
