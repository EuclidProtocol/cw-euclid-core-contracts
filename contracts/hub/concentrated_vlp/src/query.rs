use cosmwasm_std::{ensure, to_json_binary, Binary, Deps, Env, Uint128, Uint256};
use euclid::chain::ChainUid;
use euclid::error::ContractError;
use euclid::msgs::vlp::base::{
    GetLiquidityQueryResponse, GetSwapQueryResponse, PoolConfig, PoolKey, State, VlpSimulateSwapMsg,
};
use euclid::msgs::vlp::concentrated::msg::{
    AllConcentratedPoolsResponse, ConcentratedPoolInfo, ConcentratedPoolResponse, FeeResponse,
    GetStateResponse, TotalFeesPerDenomResponse, TotalFeesResponse,
};
use euclid::swap::NextSwapVlp;
use euclid::token::{Pair, PairWithAmount, Token};
use euclid_pool::common::calculate_amount_from_shares;
use euclid_pool::cp::simulate_swap;

use crate::state::{BALANCES, CHAIN_LP_TOKENS, POOL_KEY, STATE};

pub fn query_simulate_swap(
    deps: Deps,
    asset_in: Token,
    amount_in: Uint256,
    next_swaps: Vec<NextSwapVlp>,
) -> Result<Binary, ContractError> {
    ensure!(!amount_in.is_zero(), ContractError::ZeroAssetAmount {});

    let state = STATE.load(deps.storage)?;
    let pair = state.pair.clone();

    ensure!(asset_in.exists(pair), ContractError::AssetDoesNotExist {});

    let swap_response = simulate_swap(deps, &STATE, &BALANCES, asset_in, amount_in)?;

    match next_swaps.split_first() {
        Some((next_swap, forward_swaps)) => {
            let next_swap_response: GetSwapQueryResponse = deps.querier.query_wasm_smart(
                next_swap.vlp_address.clone(),
                &euclid::msgs::vlp::concentrated::msg::QueryMsg::SimulateSwap(VlpSimulateSwapMsg {
                    asset: swap_response.asset_out,
                    asset_amount: swap_response.amount_out,
                    swaps: forward_swaps.to_vec(),
                }),
            )?;
            Ok(to_json_binary(&next_swap_response)?)
        }
        None => Ok(to_json_binary(&swap_response)?),
    }
}

pub fn query_liquidity(deps: Deps, _env: Env) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    let pair = state.pair.clone();
    Ok(to_json_binary(&GetLiquidityQueryResponse {
        pair,
        token_1_reserve: BALANCES
            .may_load(deps.storage, state.pair.token_1)?
            .unwrap_or_default(),
        token_2_reserve: BALANCES
            .may_load(deps.storage, state.pair.token_2)?
            .unwrap_or_default(),
        total_lp_tokens: state.total_lp_tokens,
    })?)
}

pub fn query_fee(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&FeeResponse { fee: state.fee })?)
}

pub fn query_total_fees_collected(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&TotalFeesResponse {
        total_fees: state.total_fees_collected,
    })?)
}

pub fn query_total_fees_per_denom(deps: Deps, denom: String) -> Result<Binary, ContractError> {
    let total_fees_collected = STATE.load(deps.storage)?.total_fees_collected;

    let lp_fees = total_fees_collected.lp_fees.get_fee(denom.as_str());
    let euclid_fees = total_fees_collected.euclid_fees.get_fee(denom.as_str());

    Ok(to_json_binary(&TotalFeesPerDenomResponse {
        lp_fees: Uint128::try_from(lp_fees).map_err(|_| ContractError::new("lp_fees overflow"))?,
        euclid_fees: Uint128::try_from(euclid_fees)
            .map_err(|_| ContractError::new("euclid_fees overflow"))?,
    })?)
}

pub fn query_state(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    let pool_key = POOL_KEY.load(deps.storage)?;
    let fee_tier_bps = match pool_key.pool_type {
        euclid::msgs::vlp::base::PoolType::Concentrated { fee_tier_bps, .. } => fee_tier_bps,
        _ => 0,
    };
    let tick_spacing = match pool_key.pool_type {
        euclid::msgs::vlp::base::PoolType::Concentrated { tick_spacing, .. } => tick_spacing,
        _ => 1,
    };

    Ok(to_json_binary(&GetStateResponse {
        pair: state.pair,
        router: state.router,
        virtual_balance_contract: state.virtual_balance_contract,
        fee: state.fee,
        total_fees_collected: state.total_fees_collected,
        last_updated: state.last_updated,
        total_lp_tokens: Uint128::try_from(state.total_lp_tokens)
            .map_err(|_| ContractError::new("total_lp_tokens overflow"))?,
        pool_config: PoolConfig::Concentrated {
            fee_tier_bps,
            tick_spacing,
        },
    })?)
}

pub fn query_pool(
    deps: Deps,
    chain_uid: ChainUid,
    pool_key: PoolKey,
) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    let stored_pool_key = POOL_KEY.load(deps.storage)?;
    ensure!(
        pool_key == stored_pool_key,
        ContractError::new("pool key mismatch")
    );

    let chain_lp_tokens = CHAIN_LP_TOKENS.load(deps.storage, chain_uid)?;

    let reserve_1 = BALANCES.load(deps.storage, state.pair.token_1.clone())?;

    let reserve_2 = BALANCES.load(deps.storage, state.pair.token_2.clone())?;

    let pool = get_pool(&state, pool_key, chain_lp_tokens, reserve_1, reserve_2)?;

    Ok(to_json_binary(&pool)?)
}

pub fn query_all_pools(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    let pool_key = POOL_KEY.load(deps.storage)?;

    let reserve_1 = BALANCES.load(deps.storage, state.pair.token_1.clone())?;

    let reserve_2 = BALANCES.load(deps.storage, state.pair.token_2.clone())?;

    let pools: Result<_, ContractError> = CHAIN_LP_TOKENS
        .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .map(|item| {
            let (chain_uid, chain_lp_tokens) = item?;
            let pool = get_pool(
                &state,
                pool_key.clone(),
                chain_lp_tokens,
                reserve_1,
                reserve_2,
            )?;

            Ok::<ConcentratedPoolInfo, ContractError>(ConcentratedPoolInfo { chain_uid, pool })
        })
        .collect();

    Ok(to_json_binary(&AllConcentratedPoolsResponse {
        pools: pools?,
    })?)
}

fn get_pool(
    state: &State,
    pool_key: PoolKey,
    chain_lp_tokens: Uint256,
    reserve_1: Uint256,
    reserve_2: Uint256,
) -> Result<ConcentratedPoolResponse, ContractError> {
    let r1 = calculate_amount_from_shares(reserve_1, chain_lp_tokens, state.total_lp_tokens)
        .unwrap_or(Uint256::zero());
    let r2 = calculate_amount_from_shares(reserve_2, chain_lp_tokens, state.total_lp_tokens)
        .unwrap_or(Uint256::zero());
    Ok(ConcentratedPoolResponse {
        pool_key,
        reserve_1: Uint128::try_from(r1).map_err(|_| ContractError::new("reserve_1 overflow"))?,
        reserve_2: Uint128::try_from(r2).map_err(|_| ContractError::new("reserve_2 overflow"))?,
        lp_shares: Uint128::try_from(chain_lp_tokens)
            .map_err(|_| ContractError::new("lp_shares overflow"))?,
    })
}

pub fn extract_token_amount(
    liquidity: &PairWithAmount,
    pair: &Pair,
) -> Result<(Uint128, Uint128), ContractError> {
    let token_1_liquidity = if liquidity.token_1.token == pair.token_1 {
        liquidity.token_1.amount
    } else {
        liquidity.token_2.amount
    };

    let token_2_liquidity = if liquidity.token_2.token == pair.token_2 {
        liquidity.token_2.amount
    } else {
        liquidity.token_1.amount
    };

    Ok((
        Uint128::try_from(token_1_liquidity)
            .map_err(|_| ContractError::new("token_1 amount overflow"))?,
        Uint128::try_from(token_2_liquidity)
            .map_err(|_| ContractError::new("token_2 amount overflow"))?,
    ))
}
