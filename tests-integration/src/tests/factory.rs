#![cfg(not(target_arch = "wasm32"))]
use std::collections::HashMap;

use cosmwasm_std::{coin, Addr, Coin, Uint128};
use cw20::Cw20Contract;
use cw_orch::prelude::{
    ContractInstance, CwOrchExecute, CwOrchInstantiate, CwOrchQuery, CwOrchUpload, Environment,
};
use cw_orch_interchain::{core::InterchainEnv, prelude::*};
use escrow::{mock::mock_escrow, EscrowContract};
use euclid::chain::CrossChainUser;
use euclid::chain::CrossChainUserWithLimit;
use euclid::fee::{PartnerFee, BPS_100_PERCENT};
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

use crate::helpers::factory::{add_liquidity, create_pool, faucet, register_token, swap_request};

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
fn test_create_pool_with_funds() {
    let sender = Addr::unchecked("sender_for_all_chains");
    let interchain = MockInterchainEnv::new(vec![
        ("osmosis", &sender.to_string()),
        ("nibiru", &sender.to_string()),
    ]);
    let osmosis = interchain.get_chain("osmosis").unwrap();
    let nibiru = interchain.get_chain("nibiru").unwrap();

    let osmosis_sender = osmosis.sender.clone();
    let nibiru_sender = nibiru.sender.clone();
    osmosis
        .set_balance(
            &osmosis_sender,
            vec![
                Coin::new(100000000000000u128, "osmo"),
                Coin::new(100000000000000u128, "eucl"),
            ],
        )
        .unwrap();

    nibiru
        .set_balance(
            &nibiru_sender,
            vec![
                Coin::new(100000000000000u128, "nibi"),
                Coin::new(100000000000000u128, "eucl"),
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
                constant_product_vlp_code_id: 3,
                stable_vlp_code_id: 4,
                virtual_balance_code_id: 2,
                mock_relayer_address: None,
            },
            None,
            &[],
        )
        .unwrap();

    factory_osmosis
        .instantiate(
            &euclid::msgs::factory::InstantiateMsg {
                router_contract: router_nibiru.address().unwrap().into_string(),
                chain_uid: ChainUid::create("osmosis".to_string()).unwrap(),
                escrow_code_id: 2,
                cw20_code_id: 3,
                is_native: false,
            },
            None,
            &[],
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
                chain_uid: ChainUid::create("osmosis".to_string()).unwrap(),
                chain_info: euclid::msgs::router::RegisterFactoryChainType::Ibc(
                    RegisterFactoryChainIbc {
                        channel: osmosis_channel.to_string(),
                        timeout: None,
                    },
                ),
            },
            &[],
        )
        .unwrap();

    let _ = interchain
        .await_packets("nibiru", register_factory_request)
        .unwrap();

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
            &[],
        )
        .unwrap();

    let _ = interchain
        .await_packets("osmosis", register_escrow_request)
        .unwrap();

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
    // Test Create pool without funds
    let create_pool_with_funds_request = factory_osmosis.execute(
        &euclid::msgs::factory::ExecuteMsg::RequestPoolCreation {
            pair: PairWithDenomAndAmount {
                token_1: TokenWithDenomAndAmount {
                    token: Token::create("eucl".to_string()).unwrap(),
                    amount: Uint128::from(0u128),
                    token_type: euclid::token::TokenType::Native {
                        denom: "eucl".to_string(),
                    },
                },
                token_2: TokenWithDenomAndAmount {
                    token: Token::create("osmo".to_string()).unwrap(),
                    amount: Uint128::from(0u128),
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
    let create_pool_with_funds_request = factory_osmosis
        .execute(
            &euclid::msgs::factory::ExecuteMsg::RequestPoolCreation {
                pair: PairWithDenomAndAmount {
                    token_1: TokenWithDenomAndAmount {
                        token: Token::create("eucl".to_string()).unwrap(),
                        amount: Uint128::new(10_000u128),
                        token_type: euclid::token::TokenType::Native {
                            denom: "eucl".to_string(),
                        },
                    },
                    token_2: TokenWithDenomAndAmount {
                        token: Token::create("osmo".to_string()).unwrap(),
                        amount: Uint128::new(100_000u128),
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
            &[coin(100_000u128, "osmo"), coin(10_000u128, "eucl")],
        )
        .unwrap();

    let packet_lifetime = interchain
        .await_packets("osmosis", create_pool_with_funds_request)
        .unwrap();

    // For testing a successful outcome of the first packet sent out in the tx, you can use:
    if let IbcPacketOutcome::Success { .. } = &packet_lifetime.packets[0] {
        // Packet has been successfully acknowledged and decoded, the transaction has gone through correctly
    } else {
        panic!("packet timed out");
        // There was a decode error or the packet timed out
        // Else the packet timed-out, you may have a relayer error or something is wrong in your application
    };

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

    let _vbalance_query: GetStateResponse = virtual_balance_nibiru
        .query(&euclid::msgs::virtual_balance::QueryMsg::GetState {})
        .unwrap();

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
            &[coin(100_000u128, "osmo"), coin(10_000u128, "eucl")],
        )
        .unwrap();

    let packet_lifetime = interchain
        .await_packets("osmosis", add_liquidity_request)
        .unwrap();

    // For testing a successful outcome of the first packet sent out in the tx, you can use:
    if let IbcPacketOutcome::Success { .. } = &packet_lifetime.packets[0] {
        // Packet has been successfully acknowledged and decoded, the transaction has gone through correctly
    } else {
        panic!("packet timed out");
        // There was a decode error or the packet timed out
        // Else the packet timed-out, you may have a relayer error or something is wrong in your application
    };
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

    // Same chain test, need to upload liquidity contracts on Hub
    let factory_nibiru = FactoryContract::new(nibiru.clone());
    let escrow_nibiru = EscrowContract::new(nibiru.clone());
    let cw20_nibiru = Cw20Contract::new(nibiru.clone());
    //5
    factory_nibiru.upload().unwrap();
    //6
    escrow_nibiru.upload().unwrap();
    //7
    cw20_nibiru.upload().unwrap();

    factory_nibiru
        .instantiate(
            &euclid::msgs::factory::InstantiateMsg {
                router_contract: router_nibiru.address().unwrap().into_string(),
                chain_uid: ChainUid::create("nibiru".to_string()).unwrap(),
                escrow_code_id: 6,
                cw20_code_id: 7,
                is_native: true,
            },
            None,
            &[],
        )
        .unwrap();

    router_nibiru
        .execute(
            &euclid::msgs::router::ExecuteMsg::RegisterFactory {
                chain_uid: ChainUid::create("nibiru".to_string()).unwrap(),
                chain_info: euclid::msgs::router::RegisterFactoryChainType::Native(
                    RegisterFactoryChainNative {
                        factory_address: factory_nibiru.address().unwrap().into_string(),
                    },
                ),
            },
            &[],
        )
        .unwrap();

    factory_nibiru
        .execute(
            &euclid::msgs::factory::ExecuteMsg::RequestRegisterDenom {
                token: TokenWithDenom {
                    token: Token::create("eucl".to_string()).unwrap(),
                    token_type: euclid::token::TokenType::Native {
                        denom: "eucl".to_string(),
                    },
                },
                timeout: None,
            },
            &[],
        )
        .unwrap();

    factory_nibiru
        .execute(
            &euclid::msgs::factory::ExecuteMsg::RequestPoolCreation {
                pair: PairWithDenomAndAmount {
                    token_1: TokenWithDenomAndAmount {
                        token: Token::create("nibi".to_string()).unwrap(),
                        amount: Uint128::from(100_000u128),
                        token_type: euclid::token::TokenType::Native {
                            denom: "nibi".to_string(),
                        },
                    },
                    token_2: TokenWithDenomAndAmount {
                        token: Token::create("eucl".to_string()).unwrap(),
                        amount: Uint128::from(10_000u128),
                        token_type: euclid::token::TokenType::Native {
                            denom: "eucl".to_string(),
                        },
                    },
                },
                slippage_tolerance_bps: 100,
                timeout: None,
                lp_token_name: "nibiru".to_string(),
                lp_token_symbol: "nibi".to_string(),
                lp_token_decimal: 6,
                lp_token_marketing: None,
                pool_config: PoolConfig::ConstantProduct {},
            },
            &[coin(100_000u128, "nibi"), coin(10_000u128, "eucl")],
        )
        .unwrap();

    // Validation checks //
    let all_pools_query: AllPoolsResponse = factory_nibiru
        .query(&euclid::msgs::factory::QueryMsg::GetAllPools {})
        .unwrap();
    assert_eq!(
        all_pools_query,
        AllPoolsResponse {
            pools: vec![PoolVlpResponse {
                pair: Pair::new(
                    Token::create("eucl".to_string()).unwrap(),
                    Token::create("nibi".to_string()).unwrap(),
                )
                .unwrap(),
                vlp: Addr::unchecked("contract5").into_string(),
            }],
        }
    );

    let vlp_query: VlpResponse = router_nibiru
        .query(&euclid::msgs::router::QueryMsg::GetVlp {
            pair: Pair::new(
                Token::create("nibi".to_string()).unwrap(),
                Token::create("eucl".to_string()).unwrap(),
            )
            .unwrap(),
        })
        .unwrap();
    assert_eq!(
        vlp_query,
        VlpResponse {
            vlp: Addr::unchecked("contract5").into_string(),
            token_1: Token::create("eucl".to_string()).unwrap(),
            token_2: Token::create("nibi".to_string()).unwrap(),
        }
    );

    // Got this address from the query above
    vlp_nibiru.set_address(&Addr::unchecked("contract5"));

    let liquidity_query: GetLiquidityResponse = vlp_nibiru
        .query(&euclid::msgs::vlp::QueryMsg::Liquidity {})
        .unwrap();
    assert_eq!(
        liquidity_query,
        GetLiquidityResponse {
            pair: Pair {
                token_1: Token::create("eucl".to_string()).unwrap(),
                token_2: Token::create("nibi".to_string()).unwrap(),
            },
            token_1_reserve: Uint128::new(10_000),
            token_2_reserve: Uint128::new(100_000),
            total_lp_tokens: Uint128::new(30622),
        }
    );
    virtual_balance_nibiru.set_address(&Addr::unchecked("contract1"));

    let _vbalance_query: GetStateResponse = virtual_balance_nibiru
        .query(&euclid::msgs::virtual_balance::QueryMsg::GetState {})
        .unwrap();

    // Nibiru escrow contract
    escrow_nibiru.set_address(&Addr::unchecked("contract6"));
    let escrow_query: EscrowStateResponse = escrow_nibiru
        .query(&euclid::msgs::escrow::QueryMsg::State {})
        .unwrap();
    assert_eq!(
        escrow_query,
        EscrowStateResponse {
            token: Token::create("nibi".to_string()).unwrap(),
            factory_address: Addr::unchecked("contract3"),
            total_amount: Uint128::from(100_000u128),
        }
    );

    // This is the escrow for the Euclid token
    escrow_nibiru.set_address(&Addr::unchecked("contract4"));
    let escrow_query: EscrowStateResponse = escrow_nibiru
        .query(&euclid::msgs::escrow::QueryMsg::State {})
        .unwrap();
    assert_eq!(
        escrow_query,
        EscrowStateResponse {
            token: Token::create("eucl".to_string()).unwrap(),
            factory_address: Addr::unchecked("contract3"),
            total_amount: Uint128::from(10_000u128),
        }
    );

    // Add Liquidity
    factory_nibiru
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
                        token: Token::create("nibi".to_string()).unwrap(),
                        amount: Uint128::from(100_000u128),
                        token_type: euclid::token::TokenType::Native {
                            denom: "nibi".to_string(),
                        },
                    },
                },
                slippage_tolerance_bps: 100, // 1% slippage tolerance
                timeout: None,               // 10 minutes in seconds
            },
            &[coin(100_000u128, "nibi"), coin(10_000u128, "eucl")],
        )
        .unwrap();

    let liquidity_query: GetLiquidityResponse = vlp_nibiru
        .query(&euclid::msgs::vlp::QueryMsg::Liquidity {})
        .unwrap();
    assert_eq!(
        liquidity_query,
        GetLiquidityResponse {
            pair: Pair {
                token_1: Token::create("eucl".to_string()).unwrap(),
                token_2: Token::create("nibi".to_string()).unwrap(),
            },
            token_1_reserve: Uint128::new(10_000u128 * 2),
            token_2_reserve: Uint128::new(100_000u128 * 2),
            total_lp_tokens: Uint128::new(30622u128 * 2),
        }
    );
    // Euclid escrow contract
    let escrow_query: EscrowStateResponse = escrow_nibiru
        .query(&euclid::msgs::escrow::QueryMsg::State {})
        .unwrap();
    assert_eq!(
        escrow_query,
        EscrowStateResponse {
            token: Token::create("eucl".to_string()).unwrap(),
            factory_address: Addr::unchecked("contract3"),
            total_amount: Uint128::from(10_000u128 * 2),
        }
    );
    // Osmo escrow contract
    escrow_nibiru.set_address(&Addr::unchecked("contract6"));
    let escrow_query: EscrowStateResponse = escrow_nibiru
        .query(&euclid::msgs::escrow::QueryMsg::State {})
        .unwrap();
    assert_eq!(
        escrow_query,
        EscrowStateResponse {
            token: Token::create("nibi".to_string()).unwrap(),
            factory_address: Addr::unchecked("contract3"),
            total_amount: Uint128::from(100_000u128 * 2),
        }
    );
    // Test swap
    let eucl_token = TokenWithDenom {
        token: Token::create("eucl".to_string()).unwrap(),
        token_type: euclid::token::TokenType::Native {
            denom: "eucl".to_string(),
        },
    };
    let nibi_token = TokenWithDenom {
        token: Token::create("nibi".to_string()).unwrap(),
        token_type: euclid::token::TokenType::Native {
            denom: "nibi".to_string(),
        },
    };
    factory_nibiru
        .execute(
            &euclid::msgs::factory::ExecuteMsg::ExecuteSwapRequest(ExecuteSwapRequest {
                sender: None,
                asset_in: eucl_token.clone(),
                amount_in: Uint128::from(1_000u128),
                asset_out: nibi_token.token.clone(),
                min_amount_out: Uint128::from(9000u128),
                timeout: None,
                swaps: vec![NextSwapPair {
                    token_in: eucl_token.token.clone(),
                    token_out: nibi_token.token,
                    test_fail: None,
                }],
                cross_chain_addresses: vec![CrossChainUserWithLimit {
                    user: CrossChainUser {
                        address: sender.to_string(),
                        chain_uid: ChainUid::create("nibiru".to_string()).unwrap(),
                    },
                    limit: None,
                    preferred_denom: None,
                    refund_address: None,
                    forwarding_message: None,
                }],
                partner_fee: None,
                meta: None,
            }),
            &[coin(1_000u128, "eucl")],
        )
        .unwrap();

    // Check balances after swap
    let escrow_query: EscrowStateResponse = escrow_nibiru
        .query(&euclid::msgs::escrow::QueryMsg::State {})
        .unwrap();
    assert_eq!(
        escrow_query,
        EscrowStateResponse {
            token: Token::create("nibi".to_string()).unwrap(),
            factory_address: Addr::unchecked("contract3"),
            // Total amount decreased by 9506
            total_amount: Uint128::from((100_000u128 * 2) - 9506),
        }
    );
    // This is the escrow for the Euclid token
    escrow_nibiru.set_address(&Addr::unchecked("contract4"));
    let escrow_query: EscrowStateResponse = escrow_nibiru
        .query(&euclid::msgs::escrow::QueryMsg::State {})
        .unwrap();
    assert_eq!(
        escrow_query,
        EscrowStateResponse {
            token: Token::create("eucl".to_string()).unwrap(),
            factory_address: Addr::unchecked("contract3"),
            // Total amount increased by 1000
            total_amount: Uint128::from((10_000u128 * 2) + 1000),
        }
    );

    // Test deposit
    factory_nibiru
        .execute(
            &euclid::msgs::factory::ExecuteMsg::DepositToken {
                amount_in: Uint128::from(100u128),
                asset_in: eucl_token.clone(),
                recipient: None,
                timeout: None,
            },
            &[coin(100, "eucl")],
        )
        .unwrap();

    let virtual_balance_query: GetBalanceResponse = virtual_balance_nibiru
        .query(&euclid::msgs::virtual_balance::QueryMsg::GetBalance {
            balance_key: BalanceKey {
                cross_chain_user: CrossChainUser {
                    address: nibiru_sender.to_string(),
                    chain_uid: ChainUid::create("nibiru".to_string()).unwrap(),
                },
                token_id: eucl_token.token.to_string(),
            },
        })
        .unwrap();
    assert_eq!(
        virtual_balance_query,
        GetBalanceResponse {
            amount: Uint128::from(100u128),
        }
    );

    // Test withdraw
    factory_nibiru
        .withdraw_virtual_balance(
            Uint128::new(50),
            vec![CrossChainUserWithLimit {
                user: CrossChainUser {
                    address: nibiru_sender.to_string(),
                    chain_uid: ChainUid::create("nibiru".to_string()).unwrap(),
                },
                limit: None,
                preferred_denom: None,
                refund_address: None,
                forwarding_message: None,
            }],
            Token::create("eucl".to_string()).unwrap(),
            None,
        )
        .unwrap();

    let virtual_balance_query: GetBalanceResponse = virtual_balance_nibiru
        .query(&euclid::msgs::virtual_balance::QueryMsg::GetBalance {
            balance_key: BalanceKey {
                cross_chain_user: CrossChainUser {
                    address: nibiru_sender.to_string(),
                    chain_uid: ChainUid::create("nibiru".to_string()).unwrap(),
                },
                token_id: eucl_token.token.to_string(),
            },
        })
        .unwrap();
    assert_eq!(
        virtual_balance_query,
        GetBalanceResponse {
            amount: Uint128::from(50u128),
        }
    );
}

#[test]
fn test_add_liquidity() {
    let sender = Addr::unchecked("sender_for_all_chains");
    let interchain = MockInterchainEnv::new(vec![
        ("osmosis", &sender.to_string()),
        ("nibiru", &sender.to_string()),
    ]);
    let osmosis = interchain.get_chain("osmosis").unwrap();
    let nibiru = interchain.get_chain("nibiru").unwrap();

    let osmosis_sender = osmosis.sender.clone();
    let nibiru_sender = nibiru.sender.clone();

    osmosis
        .set_balance(
            &osmosis_sender,
            vec![
                Coin::new(100000000000000u128, "osmo"),
                Coin::new(100000000000000u128, "eucl"),
            ],
        )
        .unwrap();

    nibiru
        .set_balance(
            &nibiru_sender,
            vec![
                Coin::new(100000000000000u128, "nibi"),
                Coin::new(100000000000000u128, "eucl"),
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
                mock_relayer_address: None,
                stable_vlp_code_id: 4,
            },
            None,
            &[],
        )
        .unwrap();

    factory_osmosis
        .instantiate(
            &euclid::msgs::factory::InstantiateMsg {
                router_contract: router_nibiru.address().unwrap().into_string(),
                chain_uid: ChainUid::create("osmosis".to_string()).unwrap(),
                escrow_code_id: 2,
                cw20_code_id: 3,
                is_native: false,
            },
            None,
            &[],
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
                chain_uid: ChainUid::create("osmosis".to_string()).unwrap(),
                chain_info: euclid::msgs::router::RegisterFactoryChainType::Ibc(
                    RegisterFactoryChainIbc {
                        channel: osmosis_channel.to_string(),
                        timeout: None,
                    },
                ),
            },
            &[],
        )
        .unwrap();

    let _ = interchain
        .await_packets("nibiru", register_factory_request)
        .unwrap();

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
            &[],
        )
        .unwrap();

    let _ = interchain
        .await_packets("osmosis", register_escrow_request)
        .unwrap();

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
            &[coin(100_000u128, "osmo"), coin(10_000u128, "eucl")],
        )
        .unwrap();

    let packet_lifetime = interchain
        .await_packets("osmosis", create_pool_with_funds_request)
        .unwrap();

    // For testing a successful outcome of the first packet sent out in the tx, you can use:
    if let IbcPacketOutcome::Success { ack_tx, .. } = &packet_lifetime.packets[0] {
        println!("{:?}", ack_tx.tx_id.response.events);
        // Packet has been successfully acknowledged and decoded, the transaction has gone through correctly
    } else {
        panic!("packet timed out");
        // There was a decode error or the packet timed out
        // Else the packet timed-out, you may have a relayer error or something is wrong in your application
    };

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
            &[coin(100_000u128, "osmo"), coin(10_000u128, "eucl")],
        )
        .unwrap();

    let packet_lifetime = interchain
        .await_packets("osmosis", add_liquidity_request)
        .unwrap();

    // For testing a successful outcome of the first packet sent out in the tx, you can use:
    if let IbcPacketOutcome::Success { .. } = &packet_lifetime.packets[0] {
        // Packet has been successfully acknowledged and decoded, the transaction has gone through correctly
    } else {
        panic!("packet timed out");
        // There was a decode error or the packet timed out
        // Else the packet timed-out, you may have a relayer error or something is wrong in your application
    };
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
    register_token(
        &interchain,
        &factory,
        pair_info.token_1.to_token_with_denom(),
    );
    create_pool(
        &interchain,
        &factory,
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

    add_liquidity(&interchain, &factory, pair_info, 0, None, funds);
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
    register_token(
        &interchain,
        &factory,
        pair_info.token_1.to_token_with_denom(),
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
    register_token(
        &interchain,
        &factory,
        pair_info.token_1.to_token_with_denom(),
    );

    create_pool(
        &interchain,
        &factory,
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
    register_token(
        &interchain,
        &factory,
        pair_info.token_1.to_token_with_denom(),
    );

    create_pool(
        &interchain,
        &factory,
        pair_info.clone(),
        BPS_1_PERCENT,
        PoolConfig::ConstantProduct {},
    );

    add_liquidity(
        &interchain,
        &factory,
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
    register_token(
        &interchain,
        &factory,
        pair_info.token_1.to_token_with_denom(),
    );

    // Attempt to add liquidity with tokens that aren't allowed by the escrow.
    pair_info.token_1.token_type = TokenType::Native {
        denom: "osmo".to_string(),
    };

    create_pool(
        &interchain,
        &factory,
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
    register_token(
        &interchain,
        &factory,
        pair_info.token_1.to_token_with_denom(),
    );

    create_pool(
        &interchain,
        &factory,
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
    register_token(
        &interchain,
        &factory,
        pair_info.token_1.to_token_with_denom(),
    );

    create_pool(
        &interchain,
        &factory,
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
    register_token(
        &interchain,
        &factory,
        pair_info.token_1.to_token_with_denom(),
    );

    create_pool(
        &interchain,
        &factory,
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
        pair_info,
        BPS_100_PERCENT,
        Some(241),
        funds,
    );
}
#[test]
fn test_swap_request() {
    let sender = Addr::unchecked("sender_for_all_chains");
    let interchain = MockInterchainEnv::new(vec![
        ("osmosis", &sender.to_string()),
        ("nibiru", &sender.to_string()),
    ]);
    let osmosis = interchain.get_chain("osmosis").unwrap();
    let nibiru = interchain.get_chain("nibiru").unwrap();

    let osmosis_sender = osmosis.sender.clone();
    let nibiru_sender = nibiru.sender.clone();

    osmosis
        .set_balance(
            &osmosis_sender,
            vec![
                Coin::new(100000000000000u128, "osmo"),
                Coin::new(100000000000000u128, "eucl"),
            ],
        )
        .unwrap();

    nibiru
        .set_balance(
            &nibiru_sender,
            vec![
                Coin::new(100000000000000u128, "nibi"),
                Coin::new(100000000000000u128, "eucl"),
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
                mock_relayer_address: None,
                stable_vlp_code_id: 4,
            },
            None,
            &[],
        )
        .unwrap();

    factory_osmosis
        .instantiate(
            &euclid::msgs::factory::InstantiateMsg {
                router_contract: router_nibiru.address().unwrap().into_string(),
                chain_uid: ChainUid::create("osmosis".to_string()).unwrap(),
                escrow_code_id: 2,
                cw20_code_id: 3,
                is_native: false,
            },
            None,
            &[],
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
                chain_uid: ChainUid::create("osmosis".to_string()).unwrap(),
                chain_info: euclid::msgs::router::RegisterFactoryChainType::Ibc(
                    RegisterFactoryChainIbc {
                        channel: osmosis_channel.to_string(),
                        timeout: None,
                    },
                ),
            },
            &[],
        )
        .unwrap();

    let _ = interchain
        .await_packets("nibiru", register_factory_request)
        .unwrap();

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
            &[],
        )
        .unwrap();

    let _ = interchain
        .await_packets("osmosis", register_escrow_request)
        .unwrap();

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
            &[coin(100_000u128, "osmo"), coin(10_000u128, "eucl")],
        )
        .unwrap();

    let packet_lifetime = interchain
        .await_packets("osmosis", create_pool_with_funds_request)
        .unwrap();

    // For testing a successful outcome of the first packet sent out in the tx, you can use:
    if let IbcPacketOutcome::Success { ack_tx, .. } = &packet_lifetime.packets[0] {
        println!("{:?}", ack_tx.tx_id.response.events);
        // Packet has been successfully acknowledged and decoded, the transaction has gone through correctly
    } else {
        panic!("packet timed out");
        // There was a decode error or the packet timed out
        // Else the packet timed-out, you may have a relayer error or something is wrong in your application
    };

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
                    user: CrossChainUser {
                        chain_uid: ChainUid::create("nibiru".to_string()).unwrap(),
                        address: sender.to_string(),
                    },
                    limit: None,
                    preferred_denom: None,
                    refund_address: None,
                    forwarding_message: None,
                }],
                partner_fee: None,
                meta: None,
            }),
            &[coin(100u128, "eucl")],
        )
        .unwrap();

    let packet_lifetime = interchain
        .await_packets("osmosis", swap_request_msg)
        .unwrap();

    // For testing a successful outcome of the first packet sent out in the tx, you can use:
    if let IbcPacketOutcome::Success { .. } = &packet_lifetime.packets[0] {
        // Packet has been successfully acknowledged and decoded, the transaction has gone through correctly
    } else {
        panic!("packet timed out");
        // There was a decode error or the packet timed out
        // Else the packet timed-out, you may have a relayer error or something is wrong in your application
    };
}

#[test]
fn test_swap_request_with_valid_partner_fee() {
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
    register_token(
        &interchain,
        &factory,
        pair_info.token_1.to_token_with_denom(),
    );
    create_pool(
        &interchain,
        &factory,
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
        vec![CrossChainUserWithLimit {
            user: CrossChainUser {
                chain_uid: ChainUid::create("nibiru".to_string()).unwrap(),
                address: router_chain.sender.to_string(),
            },
            limit: None,
            preferred_denom: None,
            refund_address: None,
            forwarding_message: None,
        }],
        Some(PartnerFee {
            partner_fee_bps: 30,
            recipient: router_chain.sender.to_string(),
        }),
        funds,
        None,
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
    register_token(
        &interchain,
        &factory,
        pair_info.token_1.to_token_with_denom(),
    );
    create_pool(
        &interchain,
        &factory,
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
    swap_request(
        &interchain,
        &factory,
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
        vec![CrossChainUserWithLimit {
            user: CrossChainUser {
                chain_uid: ChainUid::create("nibiru".to_string()).unwrap(),
                address: sender.clone(),
            },
            limit: None,
            preferred_denom: None,
            refund_address: None,
            forwarding_message: None,
        }],
        Some(PartnerFee {
            partner_fee_bps: 31,
            recipient: sender,
        }),
        funds,
        None,
    );
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
    register_token(
        &interchain,
        &factory,
        pair_info.token_1.to_token_with_denom(),
    );
    create_pool(
        &interchain,
        &factory,
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

    // swapping for inavlid denom not registered on escrow
    swap_request(
        &interchain,
        &factory,
        None,
        asset_in,
        Uint128::new(1000),
        Token::create("nibi".to_string()).unwrap(),
        Uint128::new(50),
        None,
        vec![],
        vec![CrossChainUserWithLimit {
            user: CrossChainUser {
                chain_uid: ChainUid::create("nibiru".to_string()).unwrap(),
                address: sender.clone(),
            },
            limit: None,
            preferred_denom: None,
            refund_address: None,
            forwarding_message: None,
        }],
        None,
        funds,
        None,
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
    register_token(
        &interchain,
        &factory,
        pair_info.token_1.to_token_with_denom(),
    );
    create_pool(
        &interchain,
        &factory,
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
        None,
        asset_in,
        Uint128::new(1000),
        Token::create("nibi".to_string()).unwrap(),
        Uint128::new(0),
        None,
        vec![],
        vec![CrossChainUserWithLimit {
            user: CrossChainUser {
                chain_uid: ChainUid::create("nibiru".to_string()).unwrap(),
                address: sender.clone(),
            },
            limit: None,
            preferred_denom: None,
            refund_address: None,
            forwarding_message: None,
        }],
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
    register_token(
        &interchain,
        &factory,
        pair_info.token_1.to_token_with_denom(),
    );
    create_pool(
        &interchain,
        &factory,
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
            user: CrossChainUser {
                chain_uid: ChainUid::create("nibiru".to_string()).unwrap(),
                address: sender.clone(),
            },
            limit: None,
            preferred_denom: None,
            refund_address: None,
            forwarding_message: None,
        }],
        None,
        funds,
        None,
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
    register_token(
        &interchain,
        &factory,
        pair_info.token_1.to_token_with_denom(),
    );
    create_pool(
        &interchain,
        &factory,
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
        vec![CrossChainUserWithLimit {
            user: CrossChainUser {
                chain_uid: ChainUid::create("nibiru".to_string()).unwrap(),
                address: sender.clone(),
            },
            limit: None,
            preferred_denom: None,
            refund_address: None,
            forwarding_message: None,
        }],
        None,
        funds,
        None,
    );
}

#[test]
#[should_panic(expected = "Invalid Timeout")]
fn test_swap_request_fails_with_timeout_greater_than_240s() {
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
    register_token(
        &interchain,
        &factory,
        pair_info.token_1.to_token_with_denom(),
    );
    create_pool(
        &interchain,
        &factory,
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
        None,
        asset_in,
        Uint128::new(1000),
        Token::create("nibi".to_string()).unwrap(),
        Uint128::new(50),
        Some(241),
        // Set swaps such that first_swap.token_in doesn’t match asset_in.token or
        // last_swap.token_out doesn’t match asset_out.
        vec![NextSwapPair {
            token_in: Token::create("eucl".to_string()).unwrap(),
            token_out: Token::create("nibi".to_string()).unwrap(),
            test_fail: None,
        }],
        vec![CrossChainUserWithLimit {
            user: CrossChainUser {
                chain_uid: ChainUid::create("nibiru".to_string()).unwrap(),
                address: sender.clone(),
            },
            limit: None,
            preferred_denom: None,
            refund_address: None,
            forwarding_message: None,
        }],
        None,
        funds,
        None,
    );
}

#[test]
fn test_stable_pool() {
    let sender = Addr::unchecked("sender_for_all_chains");
    let interchain = MockInterchainEnv::new(vec![
        ("osmosis", &sender.to_string()),
        ("nibiru", &sender.to_string()),
    ]);
    let osmosis = interchain.get_chain("osmosis").unwrap();
    let nibiru = interchain.get_chain("nibiru").unwrap();

    let osmosis_sender = osmosis.sender.clone();

    osmosis
        .set_balance(
            &osmosis_sender,
            vec![
                Coin::new(100000000000000u128, "osmo"),
                Coin::new(100000000000000u128, "eucl"),
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
                mock_relayer_address: None,
            },
            None,
            &[],
        )
        .unwrap();

    factory_osmosis
        .instantiate(
            &euclid::msgs::factory::InstantiateMsg {
                router_contract: router_nibiru.address().unwrap().into_string(),
                chain_uid: ChainUid::create("osmosis".to_string()).unwrap(),
                escrow_code_id: 2,
                cw20_code_id: 3,
                is_native: false,
            },
            None,
            &[],
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
                chain_uid: ChainUid::create("osmosis".to_string()).unwrap(),
                chain_info: euclid::msgs::router::RegisterFactoryChainType::Ibc(
                    RegisterFactoryChainIbc {
                        channel: osmosis_channel.to_string(),
                        timeout: None,
                    },
                ),
            },
            &[],
        )
        .unwrap();

    let _ = interchain
        .await_packets("nibiru", register_factory_request)
        .unwrap();

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
            &[],
        )
        .unwrap();

    let _ = interchain
        .await_packets("osmosis", register_escrow_request)
        .unwrap();

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
            &[coin(10_000u128, "osmo"), coin(10_000u128, "eucl")],
        )
        .unwrap();

    let packet_lifetime = interchain
        .await_packets("osmosis", create_pool_with_funds_request)
        .unwrap();

    // For testing a successful outcome of the first packet sent out in the tx, you can use:
    if let IbcPacketOutcome::Success { .. } = &packet_lifetime.packets[0] {

        // Packet has been successfully acknowledged and decoded, the transaction has gone through correctly
    } else {
        panic!("packet timed out");
        // There was a decode error or the packet timed out
        // Else the packet timed-out, you may have a relayer error or something is wrong in your application
    };

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
                sender: Some(CrossChainUser {
                    chain_uid: ChainUid::create("osmosis".to_string()).unwrap(),
                    address: Addr::unchecked("sender_for_all_chains").into_string(),
                }),
                meta: None,
            }),
            &[coin(1000u128, "eucl")],
        )
        .unwrap();

    let packet_lifetime = interchain.await_packets("osmosis", swap_request).unwrap();

    // For testing a successful outcome of the first packet sent out in the tx, you can use:
    if let IbcPacketOutcome::Success { .. } = &packet_lifetime.packets[0] {

        // Packet has been successfully acknowledged and decoded, the transaction has gone through correctly
    } else {
        panic!("packet timed out");
        // There was a decode error or the packet timed out
        // Else the packet timed-out, you may have a relayer error or something is wrong in your application
    };

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
            token_2_reserve: Uint128::new(9_009),
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
