use cosmwasm_std::{
    ensure, to_json_binary, CosmosMsg, Decimal256, DepsMut, Env, OverflowError, OverflowOperation,
    Response, SubMsg, Uint128, WasmMsg,
};
use euclid::{
    error::ContractError,
    msgs::{
        cw20::ExecuteMsg as Cw20ExecuteMsg, vcoin::ExecuteMsg as VcoinExecuteMsg,
        vcoin::ExecuteTransfer,
    },
    pool::{LiquidityResponse, Pool, PoolCreationResponse, RemoveLiquidityResponse},
    swap::{NextSwap, SwapResponse},
    token::{PairInfo, Token},
};

use crate::{
    query::{assert_slippage_tolerance, calculate_lp_allocation, calculate_swap},
    reply::{NEXT_SWAP_REPLY_ID, VCOIN_TRANSFER_REPLY_ID},
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
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;

    // Verify that chain pool does not already exist
    ensure!(
        POOLS.may_load(deps.storage, &chain_id)?.is_none(),
        ContractError::PoolAlreadyExists {}
    );

    // Check for token id
    ensure!(
        state.pair.token_1 == pair_info.token_1.get_token(),
        ContractError::AssetDoesNotExist {}
    );

    ensure!(
        state.pair.token_2 == pair_info.token_2.get_token(),
        ContractError::AssetDoesNotExist {}
    );

    let pool = Pool::new(&chain_id, pair_info, Uint128::zero(), Uint128::zero());

    // Store the pool in the map
    POOLS.save(deps.storage, &chain_id, &pool)?;

    STATE.save(deps.storage, &state)?;

    let ack = PoolCreationResponse {
        vlp_contract: env.contract.address.to_string(),
    };

    Ok(Response::new()
        .add_attribute("action", "register_pool")
        .add_attribute("pool_chain", pool.chain)
        .set_data(to_json_binary(&ack)?))
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
    outpost_sender: String,
) -> Result<Response, ContractError> {
    // TODO: Check for pool liquidity balance and balance in vcoin
    // Router mints new tokens for this vlp so token_liquidity = vcoin_balance - pool_current_liquidity

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
    let acknowledgement = to_json_binary(&liquidity_response)?;

    // Mint LP tokens with CW20 contract

    // Get cw20 contract address
    let contract_addr = state.cw20;

    // Get LP token recipient address
    let recipient_string = format!("{}:{}", chain_id, outpost_sender);

    let mint_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr,
        msg: to_json_binary(&Cw20ExecuteMsg::Mint {
            recipient: recipient_string,
            amount: lp_allocation,
        })?,
        funds: vec![],
    });

    Ok(Response::new()
        .add_message(mint_msg)
        .add_attribute("action", "add_liquidity")
        .add_attribute("chain_id", chain_id)
        .add_attribute("lp_allocation", lp_allocation)
        .add_attribute("liquidity_1_added", token_1_liquidity)
        .add_attribute("liquidity_2_added", token_2_liquidity)
        .set_data(acknowledgement))
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
    env: Env,
    chain_id: String,
    lp_allocation: Uint128,
    outpost_sender: String,
) -> Result<Response, ContractError> {
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

    // Prepare Liquidity Response
    let liquidity_response = RemoveLiquidityResponse {
        token_1_liquidity,
        token_2_liquidity,
        burn_lp_tokens: lp_allocation,
        chain_id: chain_id.clone(),
        // TODO token 2 or 1?
        token_id: pool.pair.token_2.get_token().id,
        to_address: outpost_sender.clone(),
    };
    // Prepare acknowledgement
    let acknowledgement = to_json_binary(&liquidity_response)?;

    // Burn LP tokens with CW20 contract

    // Get cw20 contract address
    let contract_addr = state.cw20;

    // Create cw20 burn message
    let burn_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr,
        msg: to_json_binary(&Cw20ExecuteMsg::Burn {
            amount: lp_allocation,
        })?,
        funds: vec![],
    });

    // Send vcoin transfer message for both token_1 and token_2 for the to_address and to_chain_id
    let from_address = env.contract.address.to_string();
    let from_chain_id = &env.block.chain_id;
    let to_chain_id = &chain_id;

    let vcoin_transfer_msg_1 = VcoinExecuteMsg::Transfer(ExecuteTransfer {
        amount: token_1_liquidity,
        token_id: pool.pair.token_1.get_token().id,
        from_address: from_address.to_string(),
        from_chain_id: from_chain_id.to_string(),
        to_address: outpost_sender.to_string(),
        to_chain_id: to_chain_id.to_string(),
    });

    let vcoin_transfer_msg_2 = VcoinExecuteMsg::Transfer(ExecuteTransfer {
        amount: token_2_liquidity,
        token_id: pool.pair.token_2.get_token().id,
        from_address,
        from_chain_id: from_chain_id.to_string(),
        to_address: outpost_sender,
        to_chain_id: to_chain_id.to_string(),
    });

    let transfer_msgs: Vec<CosmosMsg> = vec![
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: state.vcoin.clone(),
            msg: to_json_binary(&vcoin_transfer_msg_1)?,
            funds: vec![],
        }),
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: state.vcoin,
            msg: to_json_binary(&vcoin_transfer_msg_2)?,
            funds: vec![],
        }),
    ];

    Ok(Response::new()
        .add_messages(transfer_msgs)
        .add_message(burn_msg)
        .add_attribute("action", "remove_liquidity")
        .add_attribute("chain_id", chain_id)
        .add_attribute("token_1_removed_liquidity", token_1_liquidity)
        .add_attribute("token_2_removed_liquidity", token_2_liquidity)
        .add_attribute("burn_lp", lp_allocation)
        .set_data(acknowledgement))
}

pub fn execute_swap(
    deps: DepsMut,
    env: Env,
    to_chain_id: String,
    to_address: String,
    asset_in: Token,
    amount_in: Uint128,
    min_token_out: Uint128,
    swap_id: String,
    next_swaps: Vec<NextSwap>,
) -> Result<Response, ContractError> {
    // TODO: Check for pool liquidity balance and balance in vcoin
    // Router mints new tokens for this vlp so amount_in = vcoin_balance - pool_current_liquidity

    // Get the pool for the chain_id provided
    let mut pool = POOLS.load(deps.storage, &to_chain_id)?;
    let mut state = state::STATE.load(deps.storage)?;

    // Verify that the asset exists for the VLP
    let asset_info = asset_in.clone().id;
    ensure!(
        asset_info == state.clone().pair.token_1.id || asset_info == state.clone().pair.token_2.id,
        ContractError::AssetDoesNotExist {}
    );

    // Verify that the asset amount is non-zero
    ensure!(!amount_in.is_zero(), ContractError::ZeroAssetAmount {});

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
        amount_in.multiply_ratio(Uint128::from(total_fee.unwrap()), Uint128::from(100u128));

    // Send fee to its recipient
    let vcoin_execute_msg = VcoinExecuteMsg::Transfer(ExecuteTransfer {
        amount: fee_amount,
        // Asset in or out?
        token_id: asset_in.id,
        from_address: env.contract.address.into_string(),
        from_chain_id: env.block.chain_id,
        // Who will be the fee's recipient?
        to_address: todo!(),
        // I assume that the fees will be collected on the hub
        to_chain_id: env.block.chain_id,
    });

    // Calculate the amount of asset to be swapped
    let swap_amount = amount_in.checked_sub(fee_amount)?;
    let vcoin_transfer_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: state.vcoin,
        msg: to_json_binary(&vcoin_execute_msg)?,
        funds: vec![],
    });

    // verify if asset is token 1 or token 2
    let swap_info = if asset_info == state.pair.token_1.id {
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
        !receive_amount.is_zero(),
        ContractError::SlippageExceeded {
            amount: receive_amount,
            min_amount_out: min_token_out,
        }
    );

    // Verify that the pool has enough liquidity to swap to user
    // Should activate ELP algorithm to get liquidity from other available pool

    if asset_info == state.clone().pair.token_1.id {
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
    if asset_info == state.pair.token_1.id {
        pool.reserve_1 = pool.reserve_1.checked_add(swap_amount)?;
        pool.reserve_2 = pool.reserve_2.checked_sub(receive_amount)?;
    } else {
        pool.reserve_2 = pool.reserve_2.checked_add(swap_amount)?;
        pool.reserve_1 = pool.reserve_1.checked_sub(receive_amount)?;
    }

    // Save the state of the pool
    POOLS.save(deps.storage, &to_chain_id, &pool)?;

    // Move liquidity for the state
    if asset_info == state.pair.token_1.id {
        state.total_reserve_1 = state.clone().total_reserve_1.checked_add(swap_amount)?;
        state.total_reserve_2 = state.clone().total_reserve_2.checked_sub(receive_amount)?;
    } else {
        state.total_reserve_2 = state.clone().total_reserve_2.checked_add(swap_amount)?;
        state.total_reserve_1 = state.clone().total_reserve_1.checked_sub(receive_amount)?;
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
        asset_in,
        asset_out,
        amount_in,
        amount_out: receive_amount,
        to_address: to_address.clone(),
        to_chain_id: to_chain_id.clone(),
        swap_id: swap_id.clone(),
    };

    // Prepare acknowledgement
    let acknowledgement = to_json_binary(&swap_response)?;

    let response = match next_swaps.split_first() {
        Some((next_swap, forward_swaps)) => {
            // There are more swaps
            let vcoin_transfer_msg = VcoinExecuteMsg::Transfer(ExecuteTransfer {
                amount: swap_response.amount_out,
                token_id: swap_response.asset_out.id.clone(),

                // Source Address
                from_address: env.contract.address.to_string(),
                from_chain_id: env.block.chain_id.clone(),

                // Destination Address
                to_address: next_swap.vlp_address.clone(),
                to_chain_id: env.block.chain_id,
            });

            let vcoin_transfer_msg = WasmMsg::Execute {
                contract_addr: state.vcoin,
                msg: to_json_binary(&vcoin_transfer_msg)?,
                funds: vec![],
            };

            let vcoin_transfer_msg =
                SubMsg::reply_on_error(vcoin_transfer_msg, VCOIN_TRANSFER_REPLY_ID);

            let next_swap_msg = euclid::msgs::vlp::ExecuteMsg::Swap {
                // Final user address and chain id
                to_address,
                to_chain_id,

                // Carry forward amount to next swap
                asset_in: swap_response.asset_out,
                amount_in: swap_response.amount_out,
                min_token_out,
                swap_id,
                next_swaps: forward_swaps.to_vec(),
            };
            let next_swap_msg = WasmMsg::Execute {
                contract_addr: next_swap.vlp_address.clone(),
                msg: to_json_binary(&next_swap_msg)?,
                funds: vec![],
            };

            let next_swap_msg = SubMsg::reply_always(next_swap_msg, NEXT_SWAP_REPLY_ID);

            Response::new()
                .add_attribute("swap_type", "forward_swap")
                .add_attribute("forward_to", next_swap.vlp_address.clone())
                .add_submessage(vcoin_transfer_msg)
                .add_submessage(next_swap_msg)
        }
        None => {
            //Its the last swap

            // Verify that the receive amount is greater than the minimum token out
            ensure!(
                receive_amount > min_token_out,
                ContractError::SlippageExceeded {
                    amount: receive_amount,
                    min_amount_out: min_token_out,
                }
            );
            let vcoin_transfer_msg = VcoinExecuteMsg::Transfer(ExecuteTransfer {
                amount: swap_response.amount_out,
                token_id: swap_response.asset_out.id,

                // Source Address
                from_address: env.contract.address.to_string(),
                from_chain_id: env.block.chain_id,

                // Destination Address
                to_address: to_address.clone(),
                to_chain_id: to_chain_id.clone(),
            });

            let vcoin_transfer_msg = WasmMsg::Execute {
                contract_addr: state.vcoin,
                msg: to_json_binary(&vcoin_transfer_msg)?,
                funds: vec![],
            };

            let vcoin_transfer_msg =
                SubMsg::reply_on_error(vcoin_transfer_msg, VCOIN_TRANSFER_REPLY_ID);

            Response::new()
                .add_attribute("swap_type", "final_swap")
                .add_attribute("receiver_address", to_address)
                .add_attribute("receiver_chain_id", to_chain_id)
                .add_submessage(vcoin_transfer_msg)
        }
    };

    Ok(response
        .add_message(vcoin_transfer_msg)
        .add_attribute("action", "swap")
        .add_attribute("amount_in", amount_in)
        .add_attribute("total_fee", fee_amount)
        .add_attribute("receive_amount", receive_amount)
        .set_data(acknowledgement))
}
