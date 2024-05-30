use cosmwasm_std::{
    ensure, to_json_binary, Decimal256, DepsMut, Env, IbcReceiveResponse, OverflowError,
    OverflowOperation, Uint128,
};
use euclid::{
    error::ContractError,
    pool::{LiquidityResponse, Pool, PoolCreationResponse},
    swap::SwapResponse,
    token::{PairInfo, Token},
};
use euclid_ibc::{ack::make_ack_success, msg::AcknowledgementMsg};

use crate::{
    query::{assert_slippage_tolerance, calculate_lp_allocation, calculate_swap},
    state::{self, FACTORIES, POOLS, STATE},
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
    factory: String,
    pair_info: PairInfo,
) -> Result<IbcReceiveResponse, ContractError> {
    let state = STATE.load(deps.storage)?;

    // Verify that chain pool does not already exist
    ensure!(
        POOLS.may_load(deps.storage, &chain_id)?.is_none(),
        ContractError::PoolAlreadyExists {}
    );

    // Check for token id
    ensure!(
        state.pair.token_1.get_token() == pair_info.token_1.get_token(),
        ContractError::AssetDoesNotExist {}
    );

    ensure!(
        state.pair.token_2.get_token() == pair_info.token_2.get_token(),
        ContractError::AssetDoesNotExist {}
    );

    let pool = Pool::new(&chain_id, pair_info, Uint128::zero(), Uint128::zero());

    // Store the pool in the map
    POOLS.save(deps.storage, &chain_id, &pool)?;
    FACTORIES.save(deps.storage, &chain_id, &factory)?;

    STATE.save(deps.storage, &state)?;

    let ack = AcknowledgementMsg::Ok(PoolCreationResponse {
        vlp_contract: env.contract.address.to_string(),
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
    let ratio =
        Decimal256::checked_from_ratio(token_1_liquidity, token_2_liquidity).map_err(|err| {
            ContractError::Generic {
                err: err.to_string(),
            }
        })?;

    // Lets get lq ratio, it will be the current ratio of token reserves or if its first time then it will be ratio of tokens provided
    let lq_ratio = Decimal256::checked_from_ratio(state.total_reserve_1, state.total_reserve_2)
        .unwrap_or(ratio);

    // Verify slippage tolerance is between 0 and 100
    ensure!(
        slippage_tolerance.le(&100),
        ContractError::InvalidSlippageTolerance {}
    );

    assert_slippage_tolerance(ratio, lq_ratio, slippage_tolerance)?;

    // Add liquidity to the pool
    pool.reserve_1 = pool.reserve_1.checked_add(token_1_liquidity)?;
    pool.reserve_2 = pool.reserve_2.checked_add(token_2_liquidity)?;
    POOLS.save(deps.storage, &chain_id, &pool)?;

    // Calculate liquidity added share for LP provider from total liquidity
    let lp_allocation = calculate_lp_allocation(
        token_1_liquidity,
        token_2_liquidity,
        state.total_reserve_1,
        state.total_reserve_2,
        state.total_lp_tokens,
    )?;

    // Add to total liquidity and total lp allocation
    state.total_reserve_1 = state.total_reserve_1.checked_add(token_1_liquidity)?;
    state.total_reserve_2 = state.total_reserve_2.checked_add(token_2_liquidity)?;
    state.total_lp_tokens = state.total_lp_tokens.checked_add(lp_allocation)?;
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
    pool.reserve_1 = pool.reserve_1.checked_sub(token_1_liquidity)?;
    pool.reserve_2 = pool.reserve_2.checked_sub(token_2_liquidity)?;
    POOLS.save(deps.storage, &chain_id, &pool)?;

    // Remove from total VLP liquidity

    state.total_reserve_1 = state.total_reserve_1.checked_sub(token_1_liquidity)?;
    state.total_reserve_2 = state.total_reserve_2.checked_sub(token_2_liquidity)?;
    state.total_lp_tokens = state.total_lp_tokens.checked_sub(lp_allocation)?;
    STATE.save(deps.storage, &state)?;

    Ok(IbcReceiveResponse::new()
        .add_attribute("action", "remove_liquidity")
        .add_attribute("chain_id", chain_id)
        .add_attribute("token_1_removed_liquidity", token_1_liquidity)
        .add_attribute("token_2_removed_liquidity", token_2_liquidity)
        .add_attribute("burn_lp", lp_allocation)
        .set_ack(make_ack_success()?))
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
    ensure!(
        asset_info == state.clone().pair.token_1.get_token().id
            || asset_info == state.clone().pair.token_2.get_token().id,
        ContractError::AssetDoesNotExist {}
    );

    // Verify that the asset amount is non-zero
    ensure!(!asset_amount.is_zero(), ContractError::ZeroAssetAmount {});

    // Get Fee from the state
    let fee = state.clone().fee;

    // Calcuate the sum of fees
    let total_fee = fee
        .lp_fee
        .checked_add(fee.staker_fee)
        .and_then(|x| x.checked_add(fee.treasury_fee));

    ensure!(
        total_fee.is_some(),
        ContractError::Overflow(OverflowError::new(
            OverflowOperation::Add,
            fee.lp_fee,
            fee.staker_fee
        ))
    );

    // Remove the fee from the asset amount
    let fee_amount =
        asset_amount.multiply_ratio(Uint128::from(total_fee.unwrap()), Uint128::from(100u128));

    // Calculate the amount of asset to be swapped
    let swap_amount = asset_amount.checked_sub(fee_amount)?;

    // verify if asset is token 1 or token 2
    let swap_info = if asset_info == state.pair.token_1.get_token().id {
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

    let receive_amount = calculate_swap(swap_info.0, swap_info.1, swap_info.2)?;

    // Verify that the receive amount is greater than the minimum token out
    ensure!(
        receive_amount > min_token_out,
        ContractError::SlippageExceeded {
            amount: receive_amount,
            min_amount_out: min_token_out,
        }
    );

    // Verify that the pool has enough liquidity to swap to user
    // Should activate ELP algorithm to get liquidity from other available pool

    if asset_info == state.clone().pair.token_1.get_token().id {
        ensure!(
            pool.reserve_1.ge(&swap_amount),
            ContractError::SlippageExceeded {
                amount: swap_amount,
                min_amount_out: min_token_out,
            }
        );
    } else {
        ensure!(
            pool.reserve_2.ge(&swap_amount),
            ContractError::SlippageExceeded {
                amount: swap_amount,
                min_amount_out: min_token_out,
            }
        );
    }

    // Move liquidity from the pool
    if asset_info == state.pair.token_1.get_token().id {
        pool.reserve_1 = pool.reserve_1.checked_add(swap_amount)?;
        pool.reserve_2 = pool.reserve_2.checked_sub(receive_amount)?;
    } else {
        pool.reserve_2 = pool.reserve_2.checked_add(swap_amount)?;
        pool.reserve_1 = pool.reserve_1.checked_sub(receive_amount)?;
    }

    // Save the state of the pool
    POOLS.save(deps.storage, &chain_id, &pool)?;

    // Move liquidity for the state
    if asset_info == state.pair.token_1.get_token().id {
        state.total_reserve_1 = state.clone().total_reserve_1.checked_add(swap_amount)?;
        state.total_reserve_2 = state.clone().total_reserve_2.checked_sub(receive_amount)?;
    } else {
        state.total_reserve_2 = state.clone().total_reserve_2.checked_add(swap_amount)?;
        state.total_reserve_1 = state.clone().total_reserve_1.checked_sub(receive_amount)?;
    }

    // Get asset to be recieved by user
    let asset_out = if asset_info == state.pair.token_1.get_token().id {
        state.clone().pair.token_2.get_token()
    } else {
        state.clone().pair.token_1.get_token()
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
