#![cfg(not(target_arch = "wasm32"))]

use std::collections::HashMap;

use super::chains::{get_virtual_balance, migrate_concentrated_vlp, upload_concentrated_vlp_code};
use crate::helpers::chains::{get_escrow, query_concentrated_migration_status};
use crate::helpers::relayer::relay_factory_router_factory;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{coin, Addr, StdError, Uint128};
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
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
use euclid::msgs::virtual_balance::msg::QueryMsgFns as VirtualBalanceQueryMsgFns;
use euclid::msgs::vlp::base::{PoolConfig, PoolKey, PoolType};
use euclid::msgs::vlp::concentrated::msg::{
    LegacyLiquidityMode, MigrateMsg as ConcentratedMigrateMsg, MigrationStatusResponse,
};
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
use position_token::PositionTokenContract;
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
            // Mint cw20 tokens to the recipient. Callers are responsible for
            // granting allowance to the factory (or using the cw20 Send hook)
            // when they need the factory to pull these tokens.
            let cw20 = LpTokenContract::new(chain.clone());
            cw20.set_address(&Addr::unchecked(contract_address));
            cw20.execute(
                &euclid::msgs::lp_token::msg::ExecuteMsg::Mint {
                    recipient: address.to_string(),
                    amount: Uint128::new(amount),
                },
                &[],
            )
            .unwrap();
        }
        _ => {}
    };
}

/// For each `TokenType::Smart` in `pair_with_denom`, granted an allowance to
/// the factory contract equal to the requested amount, executed as the current
/// chain sender. Native/voucher tokens are skipped.
pub fn approve_factory_for_smart_tokens(
    factory: &FactoryContract<MockBase>,
    pair_with_denom: &PairWithDenomAndAmount,
) -> Result<(), CwOrchError> {
    let chain = factory.environment();
    let factory_addr = factory.address()?.to_string();
    for token in pair_with_denom.get_vec_token_info() {
        if let TokenType::Smart { contract_address } = token.token_type {
            let cw20 = LpTokenContract::new(chain.clone());
            cw20.set_address(&Addr::unchecked(contract_address));
            cw20.execute(
                &euclid::msgs::lp_token::msg::ExecuteMsg::IncreaseAllowance {
                    spender: factory_addr.clone(),
                    amount: token.amount,
                    expires: None,
                },
                &[],
            )?;
        }
    }
    Ok(())
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
            chain,
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

pub fn concentrated_pool_key(
    pair_with_denom: &PairWithDenomAndAmount,
    fee_tier_bps: u64,
    tick_spacing: u64,
) -> PoolKey {
    PoolKey {
        pair: pair_with_denom.get_pair().unwrap(),
        pool_type: PoolType::Concentrated {
            fee_tier_bps,
            tick_spacing,
        },
    }
}

pub fn create_concentrated_pool(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    pair_with_denom: PairWithDenomAndAmount,
    fee_tier_bps: u64,
    tick_spacing: u64,
    slippage_tolerance_bps: u64,
) -> Result<PoolKey, CwOrchError> {
    create_concentrated_pool_with_tick(
        factory,
        router,
        pair_with_denom,
        fee_tier_bps,
        tick_spacing,
        slippage_tolerance_bps,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn create_concentrated_pool_with_tick(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    pair_with_denom: PairWithDenomAndAmount,
    fee_tier_bps: u64,
    tick_spacing: u64,
    slippage_tolerance_bps: u64,
    initial_tick: Option<i64>,
) -> Result<PoolKey, CwOrchError> {
    let chain = factory.environment();
    let mut funds = vec![];
    for token in pair_with_denom.get_vec_token_info() {
        faucet(
            chain,
            chain.sender.as_str(),
            token.amount.u128(),
            token.token_type.clone(),
            &mut funds,
        );
    }
    approve_factory_for_smart_tokens(factory, &pair_with_denom)?;
    let tx_response = factory.execute(
        &euclid::msgs::factory::ExecuteMsg::RequestConcentratedPoolCreation {
            pair_with_denom_and_amount: pair_with_denom.clone(),
            fee_tier_bps,
            tick_spacing,
            slippage_tolerance_bps,
            initial_tick,
            cross_chain_config: CrossChainConfig::default(),
        },
        &funds,
    )?;
    let factory_chain_uid = &factory.get_state().unwrap().chain_uid;
    relay_factory_router_factory(tx_response.events, factory, router, factory_chain_uid)?;

    let pool_key = concentrated_pool_key(&pair_with_denom, fee_tier_bps, tick_spacing);
    let registered_pool = factory.get_concentrated_vlp(pool_key.clone());
    assert!(registered_pool.is_ok(), "Concentrated pool not registered");
    Ok(pool_key)
}

#[cw_serde]
pub struct AddConcentratedLiquidityResponse {
    pub position_id: Uint128,
    pub liquidity_delta: Uint128,
    pub used_token_1: Uint128,
    pub used_token_2: Uint128,
}

#[allow(clippy::too_many_arguments)]
pub fn add_concentrated_liquidity(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    pair_with_denom: PairWithDenomAndAmount,
    pool_key: PoolKey,
    lower_tick_index: i64,
    upper_tick_index: i64,
    position_id: Option<Uint128>,
    slippage_tolerance_bps: u64,
) -> Result<AddConcentratedLiquidityResponse, CwOrchError> {
    let chain = factory.environment();
    let mut funds = vec![];
    for token in pair_with_denom.get_vec_token_info() {
        faucet(
            chain,
            chain.sender.as_str(),
            token.amount.u128(),
            token.token_type.clone(),
            &mut funds,
        );
    }
    approve_factory_for_smart_tokens(factory, &pair_with_denom)?;

    let tx_response = factory.execute(
        &euclid::msgs::factory::ExecuteMsg::AddConcentratedLiquidity {
            pair_with_denom_and_amount: pair_with_denom,
            pool_key,
            lower_tick_index,
            upper_tick_index,
            position_id,
            slippage_tolerance_bps,
            cross_chain_config: CrossChainConfig::default(),
        },
        &funds,
    )?;

    let factory_chain_uid = &factory.get_state().unwrap().chain_uid;
    let mut all_events = tx_response.events.clone();
    let relay_events =
        relay_factory_router_factory(tx_response.events, factory, router, factory_chain_uid)?;
    all_events.extend(relay_events);
    for event in all_events {
        if event
            .attributes
            .iter()
            .any(|attr| attr.key == "action" && attr.value == "clp_add_liquidity")
        {
            let attr_map = event
                .attributes
                .iter()
                .map(|attr| (attr.key.clone(), attr.value.clone()))
                .collect::<HashMap<String, String>>();
            let position_id = attr_map
                .get("position_id")
                .unwrap()
                .parse::<Uint128>()
                .unwrap();
            let liquidity_delta = attr_map
                .get("liquidity_delta")
                .unwrap()
                .parse::<Uint128>()
                .unwrap();
            let used_token_1 = attr_map
                .get("used_token_1")
                .unwrap()
                .parse::<Uint128>()
                .unwrap();
            let used_token_2 = attr_map
                .get("used_token_2")
                .unwrap()
                .parse::<Uint128>()
                .unwrap();
            return Ok(AddConcentratedLiquidityResponse {
                position_id,
                liquidity_delta,
                used_token_1,
                used_token_2,
            });
        }
    }
    Err(CwOrchError::CosmWasmError(StdError::generic_err(
        "Add Concentrated Liquidity event not found",
    )))
}

pub fn remove_concentrated_liquidity(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    pool_key: PoolKey,
    position_id: Uint128,
    liquidity_delta: Uint128,
) -> Result<(), CwOrchError> {
    let state = factory.get_state()?;
    let sender = CrossChainUser::new(state.chain_uid, factory.environment().sender.to_string());

    let tx_response = factory.execute(
        &euclid::msgs::factory::ExecuteMsg::RemoveConcentratedLiquidity {
            pool_key,
            position_id,
            liquidity_delta,
            recipient: sender,
            cross_chain_config: CrossChainConfig::default(),
        },
        &[],
    )?;

    let factory_chain_uid = &factory.get_state().unwrap().chain_uid;
    relay_factory_router_factory(tx_response.events, factory, router, factory_chain_uid)?;
    Ok(())
}

pub fn collect_concentrated_fees(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    pool_key: PoolKey,
    position_id: Uint128,
    recipient: CrossChainUser,
) -> Result<(), CwOrchError> {
    let tx_response = factory.execute(
        &euclid::msgs::factory::ExecuteMsg::CollectConcentratedFees {
            pool_key,
            position_id,
            recipient,
            cross_chain_config: CrossChainConfig::default(),
        },
        &[],
    )?;

    let factory_chain_uid = &factory.get_state().unwrap().chain_uid;
    relay_factory_router_factory(tx_response.events, factory, router, factory_chain_uid)?;
    Ok(())
}

pub fn collect_concentrated_protocol_fees(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    pool_key: PoolKey,
    recipient: CrossChainUser,
    amount_0_requested: Uint128,
    amount_1_requested: Uint128,
) -> Result<(), CwOrchError> {
    let tx_response = factory.execute(
        &euclid::msgs::factory::ExecuteMsg::CollectConcentratedProtocolFees {
            pool_key,
            recipient,
            amount_0_requested,
            amount_1_requested,
            cross_chain_config: CrossChainConfig::default(),
        },
        &[],
    )?;

    let factory_chain_uid = &factory.get_state().unwrap().chain_uid;
    relay_factory_router_factory(tx_response.events, factory, router, factory_chain_uid)?;
    Ok(())
}

pub fn migrate_concentrated_pool(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    pool_key: PoolKey,
    legacy_liquidity_mode: LegacyLiquidityMode,
    expected_prev_version: Option<String>,
    force_rebuild: Option<bool>,
) -> Result<(), CwOrchError> {
    let vlp_address = factory.get_concentrated_vlp(pool_key)?.vlp_address;
    let new_code_id = upload_concentrated_vlp_code(router.environment())?;
    migrate_concentrated_vlp(
        router.environment(),
        &Addr::unchecked(vlp_address),
        new_code_id,
        ConcentratedMigrateMsg {
            legacy_liquidity_mode,
            expected_prev_version,
            force_rebuild,
        },
    )?;
    Ok(())
}

pub fn query_concentrated_pool_migration_status(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    pool_key: PoolKey,
) -> Result<MigrationStatusResponse, CwOrchError> {
    let vlp_address = factory.get_concentrated_vlp(pool_key)?.vlp_address;
    query_concentrated_migration_status(router.environment(), &Addr::unchecked(vlp_address))
}

pub fn get_position_token(
    factory: &FactoryContract<MockBase>,
) -> Result<PositionTokenContract<MockBase>, CwOrchError> {
    let contract = PositionTokenContract::new(factory.environment().clone());
    let response = factory.get_position_token_contract()?;
    contract.set_address(&response.position_token_contract.unwrap());
    Ok(contract)
}

pub fn list_position_ids(factory: &FactoryContract<MockBase>) -> Result<Vec<String>, CwOrchError> {
    let contract = get_position_token(factory)?;
    if contract.address().is_err() {
        return Ok(vec![]);
    }
    let tokens: euclid::msgs::position_token::TokensResponse =
        contract.query(&euclid::msgs::position_token::QueryMsg::AllTokens {
            pagination: Pagination::default(),
        })?;
    Ok(tokens.tokens)
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
