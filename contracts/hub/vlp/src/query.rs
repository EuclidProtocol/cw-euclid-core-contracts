use cosmwasm_std::{
    ensure, to_json_binary, Binary, Decimal, Decimal256, Deps, Env, Isqrt, Uint128,
};
use euclid::chain::ChainUid;
use euclid::error::ContractError;
use euclid::pool::MINIMUM_LIQUIDITY;
use euclid::swap::NextSwapVlp;
use euclid::token::Token;

use euclid::msgs::vlp::{
    AllPoolsResponse, FeeResponse, GetLiquidityResponse, GetStateResponse, GetSwapResponse,
    PoolInfo, PoolResponse,
};

use crate::state::{State, BALANCES, CHAIN_LP_TOKENS, STATE};

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

    // Get Fee from the state
    let fee = state.clone().fee;

    let lp_fee = amount_in.checked_mul_floor(Decimal::bps(fee.lp_fee_bps))?;
    let euclid_fee = amount_in.checked_mul_floor(Decimal::bps(fee.euclid_fee_bps))?;

    // Calcuate the sum of fees
    let total_fee = lp_fee.checked_add(euclid_fee)?;

    // Calculate the amount of asset to be swapped
    let swap_amount = amount_in.checked_sub(total_fee)?;

    let asset_out = state.pair.get_other_token(asset_in.clone());

    let token_in_reserve = BALANCES.load(deps.storage, asset_in)?;
    let token_out_reserve = BALANCES.load(deps.storage, asset_out.clone())?;

    let receive_amount = calculate_swap(swap_amount, token_in_reserve, token_out_reserve)?;
    let response = match next_swaps.split_first() {
        Some((next_swap, forward_swaps)) => {
            let next_swap_response: GetSwapResponse = deps.querier.query_wasm_smart(
                next_swap.vlp_address.clone(),
                &euclid::msgs::vlp::QueryMsg::SimulateSwap {
                    asset: asset_out,
                    asset_amount: receive_amount,
                    swaps: forward_swaps.to_vec(),
                },
            )?;
            Ok(to_json_binary(&next_swap_response)?)
        }
        None => Ok(to_json_binary(&GetSwapResponse {
            amount_out: receive_amount,
            asset_out,
        })?),
    };
    response
}

// Function to query the total liquidity
pub fn query_liquidity(deps: Deps, _env: Env) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    let pair = state.pair.clone();
    Ok(to_json_binary(&GetLiquidityResponse {
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

pub fn query_state(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&GetStateResponse {
        pair: state.pair,
        router: state.router,
        vcoin: state.vcoin,
        fee: state.fee,
        last_updated: state.last_updated,
        total_lp_tokens: state.total_lp_tokens,
        admin: state.admin,
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
        reserve_1: reserve_1.checked_multiply_ratio(chain_lp_tokens, state.total_lp_tokens)?,
        reserve_2: reserve_2.checked_multiply_ratio(chain_lp_tokens, state.total_lp_tokens)?,
        lp_shares: chain_lp_tokens,
    })
}
// Function to calculate the asset to be recieved after a swap
pub fn calculate_swap(
    swap_amount: Uint128,
    reserve_in: Uint128,
    reserve_out: Uint128,
) -> Result<Uint128, ContractError> {
    // Calculate the k constant product
    let k = reserve_in.checked_mul(reserve_out)?;
    // Calculate the new reserve of token 1
    let new_reserve_in = reserve_in.checked_add(swap_amount)?;
    // Calculate the new reserve of token 2
    let new_reserve_out = k.checked_div(new_reserve_in)?;
    // Calculate the amount of token 2 to be recieved
    let token_2_recieved = reserve_out.checked_sub(new_reserve_out)?;

    Ok(token_2_recieved)
}

pub fn calculate_lp_allocation(
    token_1_amount: Uint128,
    token_2_amount: Uint128,
    total_liquidity_1: Uint128,
    total_liquidity_2: Uint128,
    total_lp_supply: Uint128,
) -> Result<Uint128, ContractError> {
    // IF LP supply is 0 use original function
    if total_lp_supply.is_zero() {
        let sq_root = Isqrt::isqrt(token_1_amount.checked_mul(token_2_amount)?);
        return Ok(sq_root.checked_sub(Uint128::new(MINIMUM_LIQUIDITY))?);
    }

    let lp_allocation = token_1_amount
        .checked_multiply_ratio(total_lp_supply, total_liquidity_1)?
        .min(token_2_amount.checked_multiply_ratio(total_lp_supply, total_liquidity_2)?);

    Ok(lp_allocation)
}

// Function to assert slippage is tolerated during transaction
pub fn assert_slippage_tolerance(
    ratio: Decimal256,
    pool_ratio: Decimal256,
    slippage_tolerance: u64,
) -> Result<bool, ContractError> {
    let slippage = pool_ratio.abs_diff(ratio);
    let slippage_tolerance =
        Decimal256::from_ratio(Uint128::from(slippage_tolerance), Uint128::from(100u128));
    ensure!(
        slippage.le(&slippage_tolerance),
        ContractError::LiquiditySlippageExceeded {}
    );
    Ok(true)
}
