use cosmwasm_std::{ensure, to_json_binary, Binary, Deps, Env, Uint128};
use euclid::chain::ChainUid;
use euclid::error::ContractError;
use euclid::msgs::vlp::base::{
    GetLiquidityQueryResponse, GetSwapQueryResponse, PoolConfig, State, VlpSimulateSwapMsg,
};
use euclid::swap::NextSwapVlp;
use euclid::token::{Pair, PairWithAmount, Token};
use euclid_pool::{calculate_amount_from_shares, simulate_swap, SwapCalculationMethod};

use crate::state::{BALANCES, CHAIN_LP_TOKENS, STATE};
use euclid::msgs::vlp::cp::msg::{
    AllPoolsResponse, FeeResponse, GetStateResponse, PoolInfo, PoolResponse,
    TotalFeesPerDenomResponse, TotalFeesResponse,
};

// Function to simulate swap in a query
pub fn query_simulate_swap(
    deps: Deps,
    asset_in: Token,
    amount_in: Uint128,
    next_swaps: Vec<NextSwapVlp>,
) -> Result<Binary, ContractError> {
    // Verify that the asset amount is non-zero
    ensure!(!amount_in.is_zero(), ContractError::ZeroAssetAmount {});

    let state = STATE.load(deps.storage)?;

    let pair = state.pair.clone();

    // asset should match either token
    ensure!(asset_in.exists(pair), ContractError::AssetDoesNotExist {});

    let swap_response = simulate_swap(
        deps,
        &STATE,
        &BALANCES,
        asset_in,
        amount_in,
        SwapCalculationMethod::Regular,
    )?;

    let response = match next_swaps.split_first() {
        Some((next_swap, forward_swaps)) => {
            let next_swap_response: GetSwapQueryResponse = deps.querier.query_wasm_smart(
                next_swap.vlp_address.clone(),
                &euclid::msgs::vlp::cp::msg::QueryMsg::SimulateSwap(VlpSimulateSwapMsg {
                    asset: swap_response.asset_out,
                    asset_amount: swap_response.amount_out,
                    swaps: forward_swaps.to_vec(),
                }),
            )?;
            Ok(to_json_binary(&next_swap_response)?)
        }
        None => Ok(to_json_binary(&swap_response)?),
    };
    response
}

// Function to query the total liquidity
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

// Function to query fee of the contract
pub fn query_fee(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&FeeResponse { fee: state.fee })?)
}

// Function to query total fees collected of the contract
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
        lp_fees,
        euclid_fees,
    })?)
}

pub fn query_state(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&GetStateResponse {
        pair: state.pair,
        router: state.router,
        virtual_balance_contract: state.virtual_balance_contract,
        fee: state.fee,
        total_fees_collected: state.total_fees_collected,
        last_updated: state.last_updated,
        total_lp_tokens: state.total_lp_tokens,
        admin: state.admin,
        pool_config: PoolConfig::ConstantProduct {},
    })?)
}

// Function to query a Euclid Pool Information for this pair
pub fn query_pool(deps: Deps, chain_uid: ChainUid) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;

    let chain_lp_tokens = CHAIN_LP_TOKENS.load(deps.storage, chain_uid)?;

    let reserve_1 = BALANCES.load(deps.storage, state.pair.token_1.clone())?;

    let reserve_2 = BALANCES.load(deps.storage, state.pair.token_2.clone())?;

    let pool = get_pool(&state, chain_lp_tokens, reserve_1, reserve_2)?;

    Ok(to_json_binary(&pool)?)
}
// Function to query all Euclid Pool Information
pub fn query_all_pools(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;

    let reserve_1 = BALANCES.load(deps.storage, state.pair.token_1.clone())?;

    let reserve_2 = BALANCES.load(deps.storage, state.pair.token_2.clone())?;

    let pools: Result<_, ContractError> = CHAIN_LP_TOKENS
        .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .map(|item| {
            let (chain_uid, chain_lp_tokens) = item?;
            let pool = get_pool(&state, chain_lp_tokens, reserve_1, reserve_2)?;

            Ok::<PoolInfo, ContractError>(PoolInfo { chain_uid, pool })
        })
        .collect();

    Ok(to_json_binary(&AllPoolsResponse { pools: pools? })?)
}

fn get_pool(
    state: &State,
    chain_lp_tokens: Uint128,
    reserve_1: Uint128,
    reserve_2: Uint128,
) -> Result<PoolResponse, ContractError> {
    Ok(PoolResponse {
        reserve_1: calculate_amount_from_shares(reserve_1, chain_lp_tokens, state.total_lp_tokens)
            .unwrap_or(Uint128::zero()),
        reserve_2: calculate_amount_from_shares(reserve_2, chain_lp_tokens, state.total_lp_tokens)
            .unwrap_or(Uint128::zero()),
        lp_shares: chain_lp_tokens,
    })
}

/// Extracts the token amount for a given token from a pair with amounts
/// by matching it against a reference token
pub fn extract_token_amount(liquidity: &PairWithAmount, pair: &Pair) -> (Uint128, Uint128) {
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

    (token_1_liquidity, token_2_liquidity)
}
