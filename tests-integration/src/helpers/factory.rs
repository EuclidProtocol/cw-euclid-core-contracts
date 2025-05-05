#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{coin, Uint128};
use cw20::Cw20Contract;
use cw_orch::{mock::MockBase, prelude::*};
use cw_orch_interchain::{IbcQueryHandler, InterchainEnv, MockInterchainEnv};
use euclid::{
    chain::{CrossChainUser, CrossChainUserWithLimit},
    fee::PartnerFee,
    msgs::{
        cw20::ExecuteMsgFns,
        factory::{
            ExecuteMsgFns as FactoryExecuteMsgFns, ExecuteSwapRequest,
            QueryMsgFns as FactoryQueryMsgFns,
        },
    },
    pool::PoolConfig,
    swap::NextSwapPair,
    token::{PairWithDenomAndAmount, Token, TokenType, TokenWithDenom},
};
use factory::FactoryContract;
use router::RouterContract;

use crate::helpers::relayer::relay_factory_router_factory;

pub fn register_token(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    token: TokenWithDenom,
) {
    let factory_chain_uid = &factory.get_state().unwrap().chain_uid;
    let tx_response = factory.request_register_denom(token.clone(), None).unwrap();
    relay_factory_router_factory(tx_response.events, factory, router, factory_chain_uid);

    // let _ = interchain
    //     .await_packets(factory.environment().chain_id().as_str(), tx_response)
    //     .unwrap();

    let escrow_response = factory.get_escrow(token.token.to_string());
    assert!(escrow_response.is_ok(), "Escrow not registered");

    assert!(
        escrow_response
            .unwrap()
            .denoms
            .iter()
            .any(|d| d == &token.token_type),
        "Escrow found but denom not registered"
    );
}

pub fn faucet(
    chain: &MockBase,
    address: &str,
    amount: u128,
    token_type: TokenType,
    funds: &mut Vec<Coin>,
) {
    match token_type {
        TokenType::Native { denom } => {
            chain
                .add_balance(address, vec![coin(amount, denom.clone())])
                .unwrap();
            // attach native token to the message
            funds.push(coin(amount, denom));
        }
        TokenType::Smart { contract_address } => {
            let cw20 = Cw20Contract::new(chain.clone());
            cw20.set_address(&Addr::unchecked(contract_address));
            // Increase allowance
            cw20.increase_allowance(amount, address, None).unwrap();
        }
        _ => {}
    };
}

pub fn create_pool(
    interchain: &MockInterchainEnv,
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    pair_with_denom: PairWithDenomAndAmount,
    slippage_tolerance_bps: u64,
    pool_config: PoolConfig,
) {
    let chain = interchain
        .get_chain(factory.environment().chain_id().as_str())
        .unwrap();
    let mut funds = vec![];
    for token in pair_with_denom.get_vec_token_info() {
        faucet(
            &chain,
            chain.sender.as_str(),
            token.amount.u128(),
            token.token_type.clone(),
            &mut funds,
        );
    }
    let tx_response = factory
        .execute(
            &euclid::msgs::factory::ExecuteMsg::RequestPoolCreation {
                pair: pair_with_denom.clone(),
                slippage_tolerance_bps,
                timeout: None,
                lp_token_name: "LPNAME".to_string(),
                lp_token_symbol: "LPSYMBOL".to_string(),
                lp_token_decimal: 6,
                lp_token_marketing: None,
                pool_config,
            },
            Some(&funds),
        )
        .unwrap();
    let factory_chain_uid = &factory.get_state().unwrap().chain_uid;
    relay_factory_router_factory(tx_response.events, factory, router, factory_chain_uid);

    // let _ = interchain
    //     .await_packets(factory.environment().chain_id().as_str(), tx_response)
    //     .unwrap();

    let registered_pool = factory.get_vlp(pair_with_denom.get_pair().unwrap());
    assert!(registered_pool.is_ok(), "Pool not registered");
}

pub fn add_liquidity(
    _interchain: &MockInterchainEnv,
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    pair_with_denom: PairWithDenomAndAmount,
    slippage_tolerance_bps: u64,
    timeout: Option<u64>,
    funds: Vec<Coin>,
) {
    let tx_response = factory
        .execute(
            &euclid::msgs::factory::ExecuteMsg::AddLiquidityRequest {
                pair_info: pair_with_denom.clone(),
                slippage_tolerance_bps,
                timeout,
            },
            Some(&funds),
        )
        .unwrap();

    let factory_chain_uid = &factory.get_state().unwrap().chain_uid;
    relay_factory_router_factory(tx_response.events, factory, router, factory_chain_uid);

    // let _ = interchain
    //     .await_packets(factory.environment().chain_id().as_str(), tx_response)
    //     .unwrap();
}

pub fn swap_request(
    _interchain: &MockInterchainEnv,
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    sender: Option<CrossChainUser>,
    asset_in: TokenWithDenom,
    amount_in: Uint128,
    asset_out: Token,
    min_amount_out: Uint128,
    timeout: Option<u64>,
    swaps: Vec<NextSwapPair>,
    cross_chain_addresses: Vec<CrossChainUserWithLimit>,
    partner_fee: Option<PartnerFee>,
    funds: Vec<Coin>,
    meta: Option<String>,
) {
    let tx_response = factory
        .execute(
            &euclid::msgs::factory::ExecuteMsg::ExecuteSwapRequest(ExecuteSwapRequest {
                sender,
                asset_in,
                amount_in,
                asset_out,
                min_amount_out,
                timeout,
                swaps,
                cross_chain_addresses,
                partner_fee,
                meta,
            }),
            Some(&funds),
        )
        .unwrap();

    let factory_chain_uid = &factory.get_state().unwrap().chain_uid;
    relay_factory_router_factory(tx_response.events, factory, router, factory_chain_uid);

    // let _ = interchain
    //     .await_packets(factory.environment().chain_id().as_str(), tx_response)
    //     .unwrap();
}
