use cosmwasm_std::{to_json_binary, Binary, Decimal256, Deps, Isqrt, Uint128};
use euclid::error::ContractError;
use euclid::pool::MINIMUM_LIQUIDITY;
use euclid::token::Token;

use euclid::msgs::vlp::{
    GetLiquidityResponse, GetSwapResponse, LiquidityInfoResponse, TotalLPTokensResponse,
    TotalReservesResponse,
};

use crate::state::{POOLS, STATE};

// Function to simulate swap in a query
pub fn query_simulate_swap(
    deps: Deps,
    asset: Token,
    asset_amount: Uint128,
) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;

    // Verify that the asset exists for the VLP
    let asset_info = asset.id;
    if asset_info != state.pair.token_1.id && asset_info != state.pair.token_2.id {
        return Err(ContractError::AssetDoesNotExist {});
    }

    // Verify that the asset amount is non-zero
    if asset_amount.is_zero() {
        return Err(ContractError::ZeroAssetAmount {});
    }

    // Get Fee from the state
    let fee = state.fee;

    // Calcuate the sum of fees
    let total_fee = fee.lp_fee + fee.staker_fee + fee.treasury_fee;

    // Remove the fee from the asset amount
    let fee_amount = asset_amount.multiply_ratio(Uint128::from(total_fee), Uint128::from(100u128));

    // Calculate the amount of asset to be swapped
    let swap_amount = asset_amount.checked_sub(fee_amount).unwrap();

    // verify if asset is token 1 or token 2
    let swap_info = if asset_info == state.pair.token_1.id {
        (swap_amount, state.total_reserve_1, state.total_reserve_2)
    } else {
        (swap_amount, state.total_reserve_2, state.total_reserve_1)
    };

    let receive_amount = calculate_swap(swap_info.0, swap_info.1, swap_info.2);

    // Return the amount of token to be recieved
    Ok(to_json_binary(&GetSwapResponse {
        token_out: receive_amount,
    })
    .unwrap())
}

// Function to query the total liquidity
pub fn query_liquidity(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&GetLiquidityResponse {
        token_1_reserve: state.total_reserve_1,
        token_2_reserve: state.total_reserve_2,
    })
    .unwrap())
}

// Function to query the total liquidity with pair information
pub fn query_liquidity_info(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&LiquidityInfoResponse {
        pair: state.pair,
        token_1_reserve: state.total_reserve_1,
        token_2_reserve: state.total_reserve_2,
    })
    .unwrap())
}

// Function to query fee of the contract
pub fn query_fee(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&state.fee).unwrap())
}

// Function to query a Euclid Pool Information for this pair
pub fn query_pool(deps: Deps, chain_id: String) -> Result<Binary, ContractError> {
    let pool = POOLS.load(deps.storage, &chain_id)?;
    Ok(to_json_binary(&pool).unwrap())
}

// Function to calculate the asset to be recieved after a swap
pub fn calculate_swap(swap_amount: Uint128, reserve_in: Uint128, reserve_out: Uint128) -> Uint128 {
    // Calculate the k constant product
    let k = reserve_in.checked_mul(reserve_out).unwrap();
    // Calculate the new reserve of token 1
    let new_reserve_in = reserve_in.checked_add(swap_amount).unwrap();
    // Calculate the new reserve of token 2
    let new_reserve_out = k.checked_div(new_reserve_in).unwrap();
    // Calculate the amount of token 2 to be recieved
    let token_2_recieved = reserve_out.checked_sub(new_reserve_out).unwrap();

    token_2_recieved
}

pub fn calculate_lp_allocation(
    token_1_amount: Uint128,
    token_2_amount: Uint128,
    total_liquidity_1: Uint128,
    total_liquidity_2: Uint128,
    total_lp_supply: Uint128,
) -> Uint128 {
    // IF LP supply is 0 use original function
    if total_lp_supply.is_zero() {
        let sq_root = Isqrt::isqrt(token_1_amount.checked_mul(token_2_amount).unwrap());
        return sq_root
            .checked_sub(Uint128::new(MINIMUM_LIQUIDITY))
            .unwrap();
    }
    let share_1 = token_1_amount.checked_div(total_liquidity_1).unwrap();
    let share_2 = token_2_amount.checked_div(total_liquidity_2).unwrap();

    // LP allocation is minimum of the two shares multiplied by the total_lp_supply
    let lp_allocation = share_1.min(share_2).checked_mul(total_lp_supply).unwrap();
    lp_allocation
}

// Function to assert slippage is tolerated during transaction
pub fn assert_slippage_tolerance(
    ratio: Decimal256,
    pool_ratio: Decimal256,
    slippage_tolerance: u64,
) -> bool {
    let slippage = pool_ratio.checked_sub(ratio).unwrap();
    let slippage_tolerance =
        Decimal256::from_ratio(Uint128::from(slippage_tolerance), Uint128::from(100u128));
    if slippage > slippage_tolerance {
        return false;
    }
    true
}

pub fn query_total_lp_tokens(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&TotalLPTokensResponse {
        total_lp_tokens: state.total_lp_tokens,
    })?)
}

pub fn query_total_reserves(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&TotalReservesResponse {
        token_1_reserve: state.total_reserve_1,
        token_2_reserve: state.total_reserve_2,
    })?)
}
