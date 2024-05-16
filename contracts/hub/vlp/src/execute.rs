use cosmwasm_std::{to_json_binary, Decimal256, DepsMut, Env, IbcReceiveResponse, Uint128};
use euclid::{
    error::ContractError,
    pool::{LiquidityResponse, Pool, PoolCreationResponse},
    swap::SwapResponse,
    token::{Pair, PairInfo, Token},
};
use euclid_ibc::msg::AcknowledgementMsg;

use crate::{
    ack::make_ack_success,
    query::{assert_slippage_tolerance, calculate_lp_allocation, calculate_swap},
    state::{self, POOLS, STATE},
};

/// Registers a new pool in the contract. Function called by Router Contract
///
/// # Arguments
///
/// * `deps` - The mutable dependencies for the contract execution.
/// * `info` - The message info containing the sender and other information.
/// * `pool` - The pool to be registered.
///
/// # Errors
///
/// Returns an error if the pool already exists.
///
/// # Returns
///
/// Returns a response with the action and pool chain attributes if successful.
pub fn register_pool(
    deps: DepsMut,
    env: Env,
    chain_id: String,
    pair_info: PairInfo,
) -> Result<IbcReceiveResponse, ContractError> {
    let state = STATE.load(deps.storage)?;

    // Verify that chain pool does not already exist
    if POOLS.may_load(deps.storage, &chain_id)?.is_some() {
        return Err(ContractError::PoolAlreadyExists {});
    }
    let pool = Pool {
        chain: chain_id.clone(),
        pair: pair_info,
        reserve_1: Uint128::zero(),
        reserve_2: Uint128::zero(),
    };
    // Store the pool in the map
    POOLS.save(deps.storage, &chain_id, &pool)?;

    STATE.save(deps.storage, &state)?;

    let ack = AcknowledgementMsg::Ok(PoolCreationResponse {
        vlp_contract: env.contract.address.to_string(),
        token_pair: Pair {
            token_1: pool.pair.token_1.get_token(),
            token_2: pool.pair.token_2.get_token(),
        },
    });

    Ok(IbcReceiveResponse::new()
        .add_attribute("action", "register_pool")
        .add_attribute("pool_chain", pool.chain)
        .set_ack(to_json_binary(&ack)?))
}

/// Adds liquidity to the VLP
///
/// # Arguments
///
/// * `deps` - The mutable dependencies for the contract execution.
/// * `chain_id` - The chain id of the pool to add liquidity to.
/// * `token_1_liquidity` - The amount of token 1 to add to the pool.
/// * `token_2_liquidity` - The amount of token 2 to add to the pool.
///
/// # Errors
///
/// Returns an error if the pool does not exist.
///
/// # Returns
///
/// Returns a response with the action and chain id attributes if successful.
pub fn add_liquidity(
    deps: DepsMut,
    chain_id: String,
    token_1_liquidity: Uint128,
    token_2_liquidity: Uint128,
    slippage_tolerance: u64,
) -> Result<IbcReceiveResponse, ContractError> {
    // Get the pool for the chain_id provided
    let mut pool = POOLS.load(deps.storage, &chain_id)?;
    let mut state = STATE.load(deps.storage)?;
    // Verify that ratio of assets provided is equal to the ratio of assets in the pool
    let ratio: Decimal256 = Decimal256::from_ratio(token_1_liquidity, token_2_liquidity);
    let pool_ratio: Decimal256 = Decimal256::from_ratio(pool.reserve_1, pool.reserve_2);

    // Verify slippage tolerance is between 0 and 100
    if slippage_tolerance > 100 {
        return Err(ContractError::InvalidSlippageTolerance {});
    }

    if !assert_slippage_tolerance(ratio, pool_ratio, slippage_tolerance) {
        return Err(ContractError::LiquiditySlippageExceeded {});
    }

    // Add liquidity to the pool
    pool.reserve_1 = pool.reserve_1.checked_add(token_1_liquidity).unwrap();
    pool.reserve_2 = pool.reserve_2.checked_add(token_2_liquidity).unwrap();
    POOLS.save(deps.storage, &chain_id, &pool)?;

    // Calculate liquidity added share for LP provider from total liquidity
    let lp_allocation = calculate_lp_allocation(
        token_1_liquidity,
        token_2_liquidity,
        state.total_reserve_1,
        state.total_reserve_2,
        state.total_lp_tokens,
    );

    // Add to total liquidity and total lp allocation
    state.total_reserve_1 = state
        .total_reserve_1
        .checked_add(token_1_liquidity)
        .unwrap();
    state.total_reserve_2 = state
        .total_reserve_2
        .checked_add(token_2_liquidity)
        .unwrap();
    state.total_lp_tokens = state.total_lp_tokens.checked_add(lp_allocation).unwrap();
    STATE.save(deps.storage, &state)?;

    // Add current balance to SNAPSHOT MAP
    // [TODO] BALANCES snapshotMap Token variable does not inherit all needed values

    // Prepare Liquidity Response
    let liquidity_response = LiquidityResponse {
        token_1_liquidity,
        token_2_liquidity,
        mint_lp_tokens: lp_allocation,
    };

    // Prepare acknowledgement
    let acknowledgement = to_json_binary(&AcknowledgementMsg::Ok(liquidity_response))?;

    Ok(IbcReceiveResponse::new()
        .add_attribute("action", "add_liquidity")
        .add_attribute("chain_id", chain_id)
        .add_attribute("lp_allocation", lp_allocation)
        .add_attribute("liquidity_1_added", token_1_liquidity)
        .add_attribute("liquidity_2_added", token_2_liquidity)
        .set_ack(acknowledgement))
}

/// Removes liquidity from the VLP
///
/// # Arguments
///
/// * `deps` - The mutable dependencies for the contract execution.
/// * `chain_id` - The chain id of the pool to remove liquidity from.
/// * `token_1_liquidity` - The amount of token 1 to remove from the pool.
/// * `token_2_liquidity` - The amount of token 2 to remove from the pool.
///
/// # Errors
///
/// Returns an error if the pool does not exist.
///
/// # Returns
///
/// Returns a response with the action and chain id attributes if successful.
pub fn remove_liquidity(
    deps: DepsMut,
    chain_id: String,
    lp_allocation: Uint128,
) -> Result<IbcReceiveResponse, ContractError> {
    // Get the pool for the chain_id provided
    let mut pool = POOLS.load(deps.storage, &chain_id)?;
    let mut state = STATE.load(deps.storage)?;

    // Fetch allocated liquidity to LP tokens
    let lp_tokens = state.total_lp_tokens;
    let lp_share = lp_allocation.multiply_ratio(Uint128::from(100u128), lp_tokens);

    // Calculate tokens_1 to send
    let token_1_liquidity = pool
        .reserve_1
        .multiply_ratio(lp_share, Uint128::from(100u128));
    // Calculate tokens_2 to send
    let token_2_liquidity = pool
        .reserve_2
        .multiply_ratio(lp_share, Uint128::from(100u128));

    // Remove liquidity from the pool
    pool.reserve_1 = pool.reserve_1.checked_sub(token_1_liquidity).unwrap();
    pool.reserve_2 = pool.reserve_2.checked_sub(token_2_liquidity).unwrap();
    POOLS.save(deps.storage, &chain_id, &pool)?;

    // Remove from total VLP liquidity

    state.total_reserve_1 = state
        .total_reserve_1
        .checked_sub(token_1_liquidity)
        .unwrap();
    state.total_reserve_2 = state
        .total_reserve_2
        .checked_sub(token_2_liquidity)
        .unwrap();
    state.total_lp_tokens = state.total_lp_tokens.checked_sub(lp_allocation).unwrap();
    STATE.save(deps.storage, &state)?;

    Ok(IbcReceiveResponse::new()
        .add_attribute("action", "remove_liquidity")
        .add_attribute("chain_id", chain_id)
        .add_attribute("token_1_removed_liquidity", token_1_liquidity)
        .add_attribute("token_2_removed_liquidity", token_2_liquidity)
        .add_attribute("burn_lp", lp_allocation)
        .set_ack(make_ack_success()))
}

pub fn execute_swap(
    deps: DepsMut,
    chain_id: String,
    asset: Token,
    asset_amount: Uint128,
    min_token_out: Uint128,
    swap_id: String,
) -> Result<IbcReceiveResponse, ContractError> {
    // Get the pool for the chain_id provided
    let mut pool = POOLS.load(deps.storage, &chain_id)?;
    let mut state = state::STATE.load(deps.storage)?;
    // Verify that the asset exists for the VLP

    let asset_info = asset.clone().id;
    if asset_info != state.clone().pair.token_1.id && asset_info != state.clone().pair.token_2.id {
        return Err(ContractError::AssetDoesNotExist {});
    }

    // Verify that the asset amount is non-zero
    if asset_amount.is_zero() {
        return Err(ContractError::ZeroAssetAmount {});
    }

    // Get Fee from the state
    let fee = state.clone().fee;

    // Calcuate the sum of fees
    let total_fee = fee.lp_fee + fee.staker_fee + fee.treasury_fee;

    // Remove the fee from the asset amount
    let fee_amount = asset_amount.multiply_ratio(Uint128::from(total_fee), Uint128::from(100u128));

    // Calculate the amount of asset to be swapped
    let swap_amount = asset_amount.checked_sub(fee_amount).unwrap();

    // verify if asset is token 1 or token 2
    let swap_info = if asset_info == state.clone().pair.token_1.id {
        (
            swap_amount,
            state.clone().total_reserve_1,
            state.clone().total_reserve_2,
        )
    } else {
        (
            swap_amount,
            state.clone().total_reserve_2,
            state.clone().total_reserve_1,
        )
    };

    let receive_amount = calculate_swap(swap_info.0, swap_info.1, swap_info.2);

    // Verify that the receive amount is greater than the minimum token out
    if receive_amount <= min_token_out {
        return Err(ContractError::SlippageExceeded {
            amount: receive_amount,
            min_amount_out: min_token_out,
        });
    }

    // Verify that the pool has enough liquidity to swap to user
    // Should activate ELP algorithm to get liquidity from other available pool
    if asset_info == state.clone().pair.token_1.id {
        if pool.reserve_1 < swap_amount {
            return Err(ContractError::SlippageExceeded {
                amount: swap_amount,
                min_amount_out: min_token_out,
            });
        }
    } else {
        if pool.reserve_2 < swap_amount {
            return Err(ContractError::SlippageExceeded {
                amount: swap_amount,
                min_amount_out: min_token_out,
            });
        }
    }

    // Move liquidity from the pool
    if asset_info == state.clone().pair.token_1.id {
        pool.reserve_1 = pool.reserve_1.checked_add(swap_amount).unwrap();
        pool.reserve_2 = pool.reserve_2.checked_sub(receive_amount).unwrap();
    } else {
        pool.reserve_2 = pool.reserve_2.checked_add(swap_amount).unwrap();
        pool.reserve_1 = pool.reserve_1.checked_sub(receive_amount).unwrap();
    }

    // Save the state of the pool
    POOLS.save(deps.storage, &chain_id, &pool)?;

    // Move liquidity for the state
    if asset_info == state.clone().pair.token_1.id {
        state.total_reserve_1 = state
            .clone()
            .total_reserve_1
            .checked_add(swap_amount)
            .unwrap();
        state.total_reserve_2 = state
            .clone()
            .total_reserve_2
            .checked_sub(receive_amount)
            .unwrap();
    } else {
        state.total_reserve_2 = state
            .clone()
            .total_reserve_2
            .checked_add(swap_amount)
            .unwrap();
        state.total_reserve_1 = state
            .clone()
            .total_reserve_1
            .checked_sub(receive_amount)
            .unwrap();
    }

    // Get asset to be recieved by user
    let asset_out = if asset_info == state.pair.token_1.id {
        state.clone().pair.token_2
    } else {
        state.clone().pair.token_1
    };

    STATE.save(deps.storage, &state)?;

    // Finalize ack response to swap pool
    let swap_response = SwapResponse {
        asset,
        asset_out,
        asset_amount,
        amount_out: receive_amount,
        swap_id,
    };

    // Prepare acknowledgement
    let acknowledgement = to_json_binary(&AcknowledgementMsg::Ok(swap_response))?;

    Ok(IbcReceiveResponse::new()
        .add_attribute("action", "swap")
        .add_attribute("chain_id", chain_id)
        .add_attribute("swap_amount", asset_amount)
        .add_attribute("total_fee", fee_amount)
        .add_attribute("receive_amount", receive_amount)
        .set_ack(acknowledgement))
}
