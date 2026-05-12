use crate::chains::get_concentrated_vlp;
use crate::relayer::relay_factory_router_factory;
use cosmwasm_std::{coin, Addr, Coin, Uint128, Uint256};
use cw_orch::mock::MockBase;
use cw_orch::prelude::*;
use euclid::cross_chain_user::CrossChainUser;
use euclid::msgs::cross_chain_config::CrossChainConfig;
use euclid::msgs::factory::msg::ExecuteMsgFns as FactoryExecuteMsgFns;
use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
use euclid::msgs::lp_token::msg::ExecuteMsgFns as LpTokenExecuteMsgFns;
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
use euclid::msgs::virtual_balance::msg::{
    ExecuteApprove, ExecuteMsg as VirtualBalanceExecuteMsg,
    QueryMsgFns as VirtualBalanceQueryMsgFns,
};
use euclid::msgs::vlp::base::{PoolKey, PoolType, VlpSwapMsg};
use euclid::token::{PairWithDenomAndAmount, Token, TokenType, TokenWithDenom};
use euclid::voucher::BalanceKey;
use factory::FactoryContract;
use lp_token::LpTokenContract;
use position_token::PositionTokenContract;
use router::RouterContract;

use crate::chains::get_virtual_balance;

/// Get the chain UID from the factory state.
/// Extracts the repeated `factory.get_state().unwrap().chain_uid` pattern.
fn factory_chain_uid(factory: &FactoryContract<MockBase>) -> euclid::chain::ChainUid {
    factory
        .get_state()
        .expect("factory state should exist")
        .chain_uid
}

/// Add native or CW20 tokens to a sender's balance and build the funds vec.
pub fn faucet(
    chain: &MockBase,
    address: &str,
    amount: u128,
    token_type: TokenType,
    funds: &mut Vec<Coin>,
) {
    match token_type {
        TokenType::Native { denom, .. } => {
            chain
                .add_balance(&Addr::unchecked(address), vec![coin(amount, denom.clone())])
                .expect("adding native balance should succeed");
            funds.push(coin(amount, denom));
        }
        TokenType::Smart {
            contract_address, ..
        } => {
            let cw20 = LpTokenContract::new(chain.clone());
            cw20.set_address(&Addr::unchecked(contract_address));
            cw20.increase_allowance(amount, address, None)
                .expect("CW20 allowance increase should succeed");
        }
        _ => {}
    };
}

/// Register a token denom with the factory and relay to router.
pub fn register_denom(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    token: TokenWithDenom,
) -> Result<(), CwOrchError> {
    let chain_uid = factory_chain_uid(factory);
    let tx_response = factory.register_denom(CrossChainConfig::default(), token)?;
    relay_factory_router_factory(tx_response.events, factory, router, &chain_uid)?;
    Ok(())
}

/// Deposit a token into escrow via factory→router relay.
pub fn deposit_token(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    token: TokenWithDenom,
    amount: Uint256,
    recipients: Vec<euclid::recipient::Recipient>,
) -> Result<(), CwOrchError> {
    let chain_uid = factory_chain_uid(factory);
    let mut funds = vec![];
    let amount_u128 = Uint128::try_from(amount)
        .expect("deposit amount should fit in u128")
        .u128();
    faucet(
        factory.environment(),
        factory.environment().sender.as_str(),
        amount_u128,
        token.token_type.clone(),
        &mut funds,
    );
    let tx_response = factory.execute(
        &euclid::msgs::factory::msg::ExecuteMsg::DepositToken {
            asset_in: token,
            amount_in: amount,
            recipients,
            cross_chain_config: CrossChainConfig::default(),
        },
        &funds,
    )?;
    relay_factory_router_factory(tx_response.events, factory, router, &chain_uid)?;
    Ok(())
}

/// Set up a full concentrated liquidity test environment: interchain, factory, router, two tokens.
pub fn setup_concentrated_env() -> (
    cw_orch_interchain::mock::MockInterchainEnv,
    FactoryContract<MockBase>,
    RouterContract<MockBase>,
    TokenWithDenom,
    TokenWithDenom,
) {
    use crate::chains::{setup_factory_native, setup_router, ROUTER_CHAIN_ID};
    use cw_orch_interchain::core::InterchainEnv;
    use cw_orch_interchain::mock::MockInterchainEnv;

    let sender = "sender_for_all_chains";
    let interchain = MockInterchainEnv::new(vec![(ROUTER_CHAIN_ID, sender)]);
    let router_chain = interchain
        .get_chain(ROUTER_CHAIN_ID)
        .expect("router chain should exist");
    let router =
        setup_router(&router_chain, vec![ROUTER_CHAIN_ID]).expect("router setup should succeed");
    let factory = setup_factory_native(&interchain, &router).expect("factory setup should succeed");

    let token_a = TokenWithDenom {
        token: euclid::token::Token::create("conc.token.a".to_string())
            .expect("token_a creation should succeed"),
        token_type: TokenType::Native {
            denom: "conc.token.a".to_string(),
            decimals: Some(6),
        },
    };
    let token_b = TokenWithDenom {
        token: euclid::token::Token::create("conc.token.b".to_string())
            .expect("token_b creation should succeed"),
        token_type: TokenType::Native {
            denom: "conc.token.b".to_string(),
            decimals: Some(6),
        },
    };
    register_denom(&factory, &router, token_a.clone())
        .expect("token_a registration should succeed");
    register_denom(&factory, &router, token_b.clone())
        .expect("token_b registration should succeed");

    (interchain, factory, router, token_a, token_b)
}

/// Build a PoolKey for a concentrated pool from a pair and fee parameters.
pub fn concentrated_pool_key(
    pair_with_denom: &PairWithDenomAndAmount,
    fee_tier_bps: u64,
    tick_spacing: u64,
) -> PoolKey {
    PoolKey {
        pair: pair_with_denom
            .get_pair()
            .expect("pair should be constructable from denom"),
        pool_type: PoolType::Concentrated {
            fee_tier_bps,
            tick_spacing,
        },
    }
}

/// Create a concentrated liquidity pool via factory→router relay.
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

/// Create a concentrated liquidity pool with an optional initial tick via factory→router relay.
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
            Uint128::try_from(token.amount)
                .expect("token amount should fit in u128")
                .u128(),
            token.token_type.clone(),
            &mut funds,
        );
    }
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
    let chain_uid = factory_chain_uid(factory);
    relay_factory_router_factory(tx_response.events, factory, router, &chain_uid)?;

    let pool_key = concentrated_pool_key(&pair_with_denom, fee_tier_bps, tick_spacing);
    let registered_pool = factory.get_concentrated_vlp(pool_key.clone());
    assert!(registered_pool.is_ok(), "Concentrated pool not registered");
    Ok(pool_key)
}

/// Add concentrated liquidity to a position via factory→router relay.
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
) -> Result<Uint128, CwOrchError> {
    let chain = factory.environment();
    let mut funds = vec![];
    for token in pair_with_denom.get_vec_token_info() {
        faucet(
            chain,
            chain.sender.as_str(),
            Uint128::try_from(token.amount)
                .expect("token amount should fit in u128")
                .u128(),
            token.token_type.clone(),
            &mut funds,
        );
    }

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

    let chain_uid = factory_chain_uid(factory);
    let ack_events = relay_factory_router_factory(tx_response.events, factory, router, &chain_uid)?;

    // Extract position_id from the ack events
    let pos_id = ack_events
        .iter()
        .flat_map(|e| &e.attributes)
        .find(|a| a.key == "position_id")
        .and_then(|a| a.value.parse::<u128>().ok())
        .map(Uint128::new)
        .unwrap_or_default();

    Ok(pos_id)
}

/// Remove concentrated liquidity from a position via factory→router relay.
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

    let chain_uid = factory_chain_uid(factory);
    relay_factory_router_factory(tx_response.events, factory, router, &chain_uid)?;
    Ok(())
}

/// Collect accrued fees for a concentrated liquidity position via factory→router relay.
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

    let chain_uid = factory_chain_uid(factory);
    relay_factory_router_factory(tx_response.events, factory, router, &chain_uid)?;
    Ok(())
}

/// Build a PairWithDenomAndAmount from two tokens and their amounts.
pub fn pair_with_amounts(
    token_a: &TokenWithDenom,
    token_b: &TokenWithDenom,
    amount_a: u128,
    amount_b: u128,
) -> PairWithDenomAndAmount {
    PairWithDenomAndAmount {
        token_1: token_a.with_amount(Uint256::from(amount_a)),
        token_2: token_b.with_amount(Uint256::from(amount_b)),
    }
}

/// Execute a concentrated swap directly through VLP (bypassing factory message flow).
///
/// `raw_amount_in` is in raw token units. This helper deposits the raw amount
/// (router normalizes during deposit), then normalizes the swap amount to
/// voucher units (24 decimals) for approve and VLP swap. Returns the output
/// amount in voucher units.
pub fn execute_concentrated_swap(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    pool_key: PoolKey,
    asset_in: TokenWithDenom,
    asset_out: Token,
    raw_amount_in: Uint256,
) -> Result<Uint256, CwOrchError> {
    let voucher_amount = if asset_in.token_type.is_voucher() {
        raw_amount_in
    } else {
        let decimals = asset_in
            .token_type
            .get_decimals()
            .expect("token type should have decimals");
        euclid::normalize::normalize_token_to_voucher(raw_amount_in, decimals)
            .expect("normalization should succeed")
    };
    execute_concentrated_swap_voucher(
        factory,
        router,
        pool_key,
        asset_in,
        asset_out,
        raw_amount_in,
        voucher_amount,
    )
}

/// Execute a concentrated swap using pre-normalized voucher amounts.
///
/// `raw_amount_deposit` is the raw token amount to deposit (router normalizes internally).
/// `voucher_amount` is the voucher-unit amount for approve and VLP swap.
/// Returns the output amount in voucher units.
pub fn execute_concentrated_swap_voucher(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    pool_key: PoolKey,
    asset_in: TokenWithDenom,
    asset_out: Token,
    raw_amount_deposit: Uint256,
    voucher_amount: Uint256,
) -> Result<Uint256, CwOrchError> {
    let chain_uid = factory_chain_uid(factory);
    let sender = CrossChainUser::new(chain_uid, factory.environment().sender.to_string());
    if !raw_amount_deposit.is_zero() {
        deposit_token(
            factory,
            router,
            asset_in.clone(),
            raw_amount_deposit,
            vec![],
        )?;
    }

    let vlp_address = router.get_vlp_by_pool_key(pool_key)?.vlp;
    let mut vlp = get_concentrated_vlp(router.environment(), &Addr::unchecked(vlp_address.clone()));

    let mut virtual_balance = get_virtual_balance(
        router.environment(),
        &router
            .get_state()
            .expect("router state should exist")
            .virtual_balance_address,
    );
    virtual_balance.set_sender(&router.address().expect("router should have address"));
    virtual_balance.execute(
        &VirtualBalanceExecuteMsg::Approve(ExecuteApprove {
            amount: voucher_amount,
            token_id: asset_in.token.to_string(),
            spender: CrossChainUser::new(
                euclid::chain::ChainUid::vsl_chain_uid().expect("VSL chain UID should be valid"),
                vlp_address.clone(),
            ),
            owner: sender.clone(),
        }),
        &[],
    )?;

    let before_out = virtual_balance
        .get_balance(BalanceKey {
            cross_chain_user: sender.clone(),
            token_id: asset_out.to_string(),
        })?
        .amount;

    vlp.set_sender(&router.address().expect("router should have address"));
    vlp.execute(
        &euclid::msgs::vlp::concentrated::msg::ExecuteMsg::Swap(VlpSwapMsg {
            sender: sender.clone(),
            tx_id: "fuzz_swap".to_string(),
            asset_in: asset_in.token,
            amount_in: voucher_amount,
            min_token_out: Uint256::from(1u128),
            next_swaps: vec![],
            test_fail: None,
        }),
        &[],
    )?;

    let after_out = virtual_balance
        .get_balance(BalanceKey {
            cross_chain_user: sender,
            token_id: asset_out.to_string(),
        })?
        .amount;
    Ok(after_out - before_out)
}

/// List all position token IDs by paginating until exhausted.
pub fn list_position_ids(factory: &FactoryContract<MockBase>) -> Result<Vec<String>, CwOrchError> {
    let contract = get_position_token_for_factory(factory)?;
    if contract.address().is_err() {
        return Ok(vec![]);
    }
    let mut all_tokens = Vec::new();
    let mut skip = 0u64;
    let page_size = 100u64;
    loop {
        let page: euclid::msgs::position_token::TokensResponse =
            contract.query(&euclid::msgs::position_token::QueryMsg::AllTokens {
                pagination: euclid::utils::pagination::Pagination::new(
                    None,
                    None,
                    Some(skip),
                    Some(page_size),
                ),
            })?;
        let count = page.tokens.len() as u64;
        all_tokens.extend(page.tokens);
        if count < page_size {
            break;
        }
        skip += count;
    }
    Ok(all_tokens)
}

/// Get the position token contract wrapper for a factory.
fn get_position_token_for_factory(
    factory: &FactoryContract<MockBase>,
) -> Result<PositionTokenContract<MockBase>, CwOrchError> {
    let response = factory.get_position_token_contract()?;
    let contract = PositionTokenContract::new(factory.environment().clone());
    if let Some(address) = response.position_token_contract {
        contract.set_address(&address);
        return Ok(contract);
    }
    Ok(contract)
}
