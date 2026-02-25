#![cfg(not(target_arch = "wasm32"))]

use super::chains::get_virtual_balance;
use crate::helpers::chains::get_escrow;
use crate::helpers::relayer::relay_factory_router_factory;
use cosmwasm_std::{coin, Uint128};
use cw_orch::mock::MockBase;
use cw_orch::prelude::*;
use cw_orch_interchain::prelude::MockInterchainEnv;
use euclid::cross_chain_user::CrossChainUser;
use euclid::fee::PartnerFee;
use euclid::msgs::cross_chain_config::CrossChainConfig;
use euclid::msgs::escrow::QueryMsgFns as EscrowQueryMsgFns;
use euclid::msgs::factory::msg::ExecuteMsgFns as FactoryExecuteMsgFns;
use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
use euclid::msgs::factory::ExecuteSwapRequest;
use euclid::msgs::lp_token::msg::ExecuteMsgFns as LpTokenExecuteMsgFns;
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
use euclid::msgs::virtual_balance::msg::QueryMsgFns as VirtualBalanceQueryMsgFns;
use euclid::msgs::vlp::base::PoolConfig;
use euclid::recipient::Recipient;
use euclid::swap::NextSwapPair;
use euclid::token::PairWithDenomAndAmount;
use euclid::token::Token;
use euclid::token::TokenType;
use euclid::token::TokenWithDenom;
use euclid::utils::pagination::Pagination;
use euclid::voucher::BalanceKey;
use factory::FactoryContract;
use lp_token::LpTokenContract;
use router::RouterContract;

pub fn register_token(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    token: TokenWithDenom,
) -> Result<(), CwOrchError> {
    let factory_chain_uid = &factory.get_state().unwrap().chain_uid;
    let tx_response = factory.register_denom(CrossChainConfig::default(), token.clone())?;
    relay_factory_router_factory(tx_response.events, factory, router, factory_chain_uid)?;
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
    recipients: Vec<Recipient>,
) -> Result<(), CwOrchError> {
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
        &euclid::msgs::factory::msg::ExecuteMsg::DepositToken {
            asset_in: token.clone(),
            amount_in: amount,
            recipients,
            cross_chain_config: CrossChainConfig::default(),
        },
        &funds,
    )?;
    let escrow_contract = get_escrow(factory, token.token.as_str());
    let old_escrow_balance = escrow_contract.state().unwrap();

    let old_router_escrow_balance = router.query_token_escrows(
        Pagination::new(Some(factory_chain_uid.clone()), None, None, Some(1)),
        token.token.clone(),
    )?;
    let old_balance = match old_router_escrow_balance.chains.first() {
        Some(chain) => chain.balance,
        None => Uint128::zero(),
    };
    relay_factory_router_factory(tx_response.events, factory, router, factory_chain_uid)?;
    let new_router_escrow_balance = router.query_token_escrows(
        Pagination::new(Some(factory_chain_uid.clone()), None, None, Some(1)),
        token.token.clone(),
    )?;
    let new_balance = match new_router_escrow_balance.chains.first() {
        Some(chain) => chain.balance,
        None => Uint128::zero(),
    };
    assert_eq!(
        new_balance,
        old_balance + amount,
        "Router escrow balance not updated properly"
    );
    let new_escrow_balance = escrow_contract.state().unwrap();
    assert_eq!(
        new_escrow_balance.total_amount,
        old_escrow_balance.total_amount + amount,
        "Escrow balance not updated properly"
    );

    Ok(())
}

pub fn transfer_token_vcoin(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    token: Token,
    amount: Uint128,
    recipients: Vec<Recipient>,
) -> Result<(), CwOrchError> {
    let virtual_balance_address = router.get_state().unwrap().virtual_balance_address;
    let virtual_balance_contract =
        get_virtual_balance(router.environment(), &virtual_balance_address);

    let sender = CrossChainUser::new(
        factory.get_state().unwrap().chain_uid,
        factory.environment().sender.to_string(),
    );

    let old_balance = virtual_balance_contract.get_balance(BalanceKey {
        cross_chain_user: sender.clone(),
        token_id: token.to_string(),
    })?;
    let factory_chain_uid = &factory.get_state().unwrap().chain_uid;

    let tx_response = factory.execute(
        &euclid::msgs::factory::ExecuteMsg::TransferVoucher {
            token_id: token.clone(),
            amount,
            from: None,
            recipients,
            cross_chain_config: CrossChainConfig::default(),
        },
        &[],
    )?;
    relay_factory_router_factory(tx_response.events, factory, router, factory_chain_uid)?;

    let new_balance = virtual_balance_contract.get_balance(BalanceKey {
        cross_chain_user: sender.clone(),
        token_id: token.to_string(),
    })?;

    assert_eq!(
        new_balance.amount.u128() + amount.u128(),
        old_balance.amount.u128(),
        "Virtual balance not transferred properly, old balance: {}, new balance: {}, amount: {}",
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
            let cw20 = LpTokenContract::new(chain.clone());
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
            pair_with_denom_and_amount: pair_with_denom.clone(),
            slippage_tolerance_bps,
            lp_token_name: "LPNAME".to_string(),
            lp_token_symbol: "LPSYMBOL".to_string(),
            lp_token_decimal: 6,
            lp_token_marketing: None,
            pool_config,
            cross_chain_config: CrossChainConfig::default(),
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
    funds: Vec<Coin>,
) -> Result<(), CwOrchError> {
    let tx_response = factory.execute(
        &euclid::msgs::factory::ExecuteMsg::AddLiquidity {
            pair_with_denom_and_amount: pair_with_denom.clone(),
            slippage_tolerance_bps,
            cross_chain_config: CrossChainConfig::default(),
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
    asset_in: TokenWithDenom,
    amount_in: Uint128,
    asset_out: Token,
    min_amount_out: Uint128,
    swaps: Vec<NextSwapPair>,
    recipients: Vec<Recipient>,
    partner_fee: Option<PartnerFee>,
    funds: Vec<Coin>,
) -> Result<(), CwOrchError> {
    let tx_response = factory.execute(
        &euclid::msgs::factory::ExecuteMsg::ExecuteSwapRequest(ExecuteSwapRequest {
            amount_in,
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
