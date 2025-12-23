#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{coin, Binary, Uint128};
use cw20::Cw20Contract;
use cw_orch::mock::MockBase;
use cw_orch::prelude::*;
use euclid::chain::{CrossChainUser, CrossChainUserWithLimit};
use euclid::fee::PartnerFee;
use euclid::msgs::cw20::ExecuteMsgFns;
use euclid::msgs::factory::{
    ExecuteMsgFns as FactoryExecuteMsgFns, ExecuteSwapRequest, QueryMsgFns as FactoryQueryMsgFns,
};

use cw_orch_interchain::prelude::MockInterchainEnv;
use euclid::msgs::router::QueryMsgFns;
use euclid::msgs::virtual_balance::QueryMsgFns as VirtualBalanceQueryMsgFns;
use euclid::pool::PoolConfig;
use euclid::swap::NextSwapPair;
use euclid::token::TokenType;
use euclid::token::TokenWithDenom;
use euclid::token::{PairWithDenomAndAmount, Token};
use euclid::virtual_balance::BalanceKey;
use factory::FactoryContract;
use router::RouterContract;

use crate::helpers::relayer::{relay_factory_router_factory, relay_factory_router_factory_evm};

use super::chains::get_virtual_balance;

pub fn register_token(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    token: TokenWithDenom,
) -> Result<(), CwOrchError> {
    println!("here1");
    let factory_chain_uid = &factory.get_state().unwrap().chain_uid;
    println!("here2");
    let tx_response = factory.request_register_denom(token.clone(), None)?;
    println!("here3");
    relay_factory_router_factory(tx_response.events, factory, router, factory_chain_uid)?;
    println!("here4");
    let escrow_response = factory.get_escrow(token.token.to_string())?;
    println!("here5");
    assert!(
        escrow_response
            .denoms
            .iter()
            .any(|d| d == &token.token_type),
        "Escrow found but denom not registered"
    );

    Ok(())
}

pub fn register_token_evm(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    token: TokenWithDenom,
) -> Result<(), CwOrchError> {
    let factory_chain_uid = &factory.get_state().unwrap().chain_uid;
    let tx_response = factory.request_register_denom(token.clone(), None)?;
    relay_factory_router_factory_evm(tx_response.events, factory, router, factory_chain_uid)?;
    let escrow_response = factory.get_escrow(token.token.to_string())?;
    assert!(
        escrow_response
            .denoms
            .iter()
            .any(|d| d == &token.token_type),
        "Escrow found but denom not registered"
    );

    Ok(())
}

pub fn deposit_token(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    token: TokenWithDenom,
    amount: Uint128,
    recipient: Option<CrossChainUser>,
    msg: Option<Binary>,
) -> Result<(), CwOrchError> {
    let virtual_balance_address = router.get_state().unwrap().virtual_balance_address.unwrap();
    let virtual_balance_contract =
        get_virtual_balance(router.environment(), &virtual_balance_address);

    let actual_recipient = recipient.clone().unwrap_or(CrossChainUser::new(
        factory.get_state().unwrap().chain_uid,
        factory.environment().sender.to_string(),
    ));

    let old_balance = virtual_balance_contract.get_balance(BalanceKey {
        cross_chain_user: actual_recipient.clone(),
        token_id: token.token.to_string(),
    })?;
    let factory_chain_uid = &factory.get_state().unwrap().chain_uid;
    let mut funds = vec![];
    faucet(
        factory.environment(),
        factory.environment().sender.as_str(),
        amount.u128(),
        token.token_type.clone(),
        &mut funds,
    );
    let tx_response = factory.execute(
        &euclid::msgs::factory::ExecuteMsg::DepositToken {
            asset_in: token.clone(),
            amount_in: amount,
            timeout: None,
            recipient,
            msg,
        },
        &funds,
    )?;
    relay_factory_router_factory(tx_response.events, factory, router, factory_chain_uid)?;

    let new_balance = virtual_balance_contract.get_balance(BalanceKey {
        cross_chain_user: actual_recipient,
        token_id: token.token.to_string(),
    })?;

    assert!(
        new_balance.amount.u128() == old_balance.amount.u128() + amount.u128(),
        "Virtual balance not deposited properly, old balance: {}, new balance: {}, amount: {}",
        old_balance.amount.u128(),
        new_balance.amount.u128(),
        amount.u128()
    );
    Ok(())
}

pub fn deposit_token_evm(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    token: TokenWithDenom,
    amount: Uint128,
    recipient: Option<CrossChainUser>,
    msg: Option<Binary>,
) -> Result<(), CwOrchError> {
    let virtual_balance_address = router.get_state().unwrap().virtual_balance_address.unwrap();
    let virtual_balance_contract =
        get_virtual_balance(router.environment(), &virtual_balance_address);

    let actual_recipient = recipient.clone().unwrap_or(CrossChainUser::new(
        factory.get_state().unwrap().chain_uid,
        factory.environment().sender.to_string(),
    ));

    let old_balance = virtual_balance_contract.get_balance(BalanceKey {
        cross_chain_user: actual_recipient.clone(),
        token_id: token.token.to_string(),
    })?;
    let factory_chain_uid = &factory.get_state().unwrap().chain_uid;
    let mut funds = vec![];
    faucet(
        factory.environment(),
        factory.environment().sender.as_str(),
        amount.u128(),
        token.token_type.clone(),
        &mut funds,
    );
    let tx_response = factory.execute(
        &euclid::msgs::factory::ExecuteMsg::DepositToken {
            asset_in: token.clone(),
            amount_in: amount,
            timeout: None,
            recipient,
            msg,
        },
        &funds,
    )?;
    relay_factory_router_factory_evm(tx_response.events, factory, router, factory_chain_uid)?;

    let new_balance = virtual_balance_contract.get_balance(BalanceKey {
        cross_chain_user: actual_recipient,
        token_id: token.token.to_string(),
    })?;

    assert!(
        new_balance.amount.u128() == old_balance.amount.u128() + amount.u128(),
        "Virtual balance not deposited properly, old balance: {}, new balance: {}, amount: {}",
        old_balance.amount.u128(),
        new_balance.amount.u128(),
        amount.u128()
    );
    Ok(())
}

pub fn transfer_token_vcoin(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    token: Token,
    amount: Uint128,
    recipient: CrossChainUser,
    msg: Option<Binary>,
) -> Result<(), CwOrchError> {
    let virtual_balance_address = router.get_state().unwrap().virtual_balance_address.unwrap();
    let virtual_balance_contract =
        get_virtual_balance(router.environment(), &virtual_balance_address);

    let old_balance = virtual_balance_contract.get_balance(BalanceKey {
        cross_chain_user: recipient.clone(),
        token_id: token.to_string(),
    })?;
    let factory_chain_uid = &factory.get_state().unwrap().chain_uid;

    let tx_response = factory.execute(
        &euclid::msgs::factory::ExecuteMsg::TransferVirtualBalance {
            token: token.clone(),
            amount,
            recipient_address: recipient.clone(),
            timeout: None,
            from: None,
            msg,
        },
        &[],
    )?;
    relay_factory_router_factory(tx_response.events, factory, router, factory_chain_uid)?;

    let new_balance = virtual_balance_contract.get_balance(BalanceKey {
        cross_chain_user: recipient,
        token_id: token.to_string(),
    })?;

    assert!(
        new_balance.amount.u128() == old_balance.amount.u128() + amount.u128(),
        "Virtual balance not deposited properly, old balance: {}, new balance: {}, amount: {}",
        old_balance.amount.u128(),
        new_balance.amount.u128(),
        amount.u128()
    );
    Ok(())
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
                .add_balance(&Addr::unchecked(address), vec![coin(amount, denom.clone())])
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
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    pair_with_denom: PairWithDenomAndAmount,
    slippage_tolerance_bps: u64,
    pool_config: PoolConfig,
) -> Result<(), CwOrchError> {
    let chain = factory.environment();
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
    let tx_response = factory.execute(
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
        &funds,
    )?;
    let factory_chain_uid = &factory.get_state().unwrap().chain_uid;
    relay_factory_router_factory(tx_response.events, factory, router, factory_chain_uid)?;

    let registered_pool = factory.get_vlp(pair_with_denom.get_pair().unwrap());
    assert!(registered_pool.is_ok(), "Pool not registered");
    Ok(())
}

pub fn add_liquidity(
    _interchain: &MockInterchainEnv,
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    pair_with_denom: PairWithDenomAndAmount,
    slippage_tolerance_bps: u64,
    timeout: Option<u64>,
    funds: Vec<Coin>,
) -> Result<(), CwOrchError> {
    let tx_response = factory.execute(
        &euclid::msgs::factory::ExecuteMsg::AddLiquidityRequest {
            pair_info: pair_with_denom.clone(),
            slippage_tolerance_bps,
            timeout,
        },
        &funds,
    )?;

    let factory_chain_uid = &factory.get_state().unwrap().chain_uid;
    relay_factory_router_factory(tx_response.events, factory, router, factory_chain_uid)?;

    Ok(())
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
) -> Result<(), CwOrchError> {
    let tx_response = factory.execute(
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
        &funds,
    )?;

    let factory_chain_uid = &factory.get_state().unwrap().chain_uid;
    relay_factory_router_factory(tx_response.events, factory, router, factory_chain_uid)?;

    Ok(())
}
