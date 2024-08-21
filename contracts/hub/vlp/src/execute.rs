use cosmwasm_std::{
    ensure, to_json_binary, Decimal, Decimal256, DepsMut, Env, MessageInfo, Response, SubMsg,
    Uint128, WasmMsg,
};
use euclid::{
    chain::{ChainUid, CrossChainUser},
    error::ContractError,
    events::{liquidity_event, simple_event, tx_event, TxType},
    fee::MAX_FEE_BPS,
    liquidity::AddLiquidityResponse,
    msgs::{
        virtual_balance::ExecuteTransfer,
        vlp::{VlpRemoveLiquidityResponse, VlpSwapResponse},
    },
    pool::{Pool, PoolCreationResponse},
    swap::NextSwapVlp,
    token::{Pair, Token},
    virtual_balance::BalanceKey,
};

use crate::{
    query::{assert_slippage_tolerance, calculate_lp_allocation, calculate_swap},
    reply::{VIRTUAL_BALANCE_TRANSFER_REPLY_ID, NEXT_SWAP_REPLY_ID},
    state::{self, BALANCES, CHAIN_LP_TOKENS, STATE},
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
    info: MessageInfo,
    sender: CrossChainUser,
    pair: Pair,
    tx_id: String,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;

    ensure!(info.sender == state.router, ContractError::Unauthorized {});

    // Verify that chain pool does not already exist
    ensure!(
        !CHAIN_LP_TOKENS.has(deps.storage, sender.chain_uid.clone()),
        ContractError::PoolAlreadyExists {}
    );

    // Check for token id
    ensure!(
        state.pair.get_tupple() == pair.get_tupple(),
        ContractError::AssetDoesNotExist {}
    );

    // Store the pool in the map
    CHAIN_LP_TOKENS.save(deps.storage, sender.chain_uid.clone(), &Uint128::zero())?;

    let ack = PoolCreationResponse {
        vlp_contract: env.contract.address.to_string(),
    };

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            &sender.to_sender_string(),
            TxType::PoolCreation,
        ))
        .add_attribute("action", "register_pool")
        .add_attribute("pool_chain", sender.chain_uid.to_string())
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
    env: Env,
    info: MessageInfo,
    sender: CrossChainUser,
    token_1_liquidity: Uint128,
    token_2_liquidity: Uint128,
    slippage_tolerance: u64,
    tx_id: String,
) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.router, ContractError::Unauthorized {});

    let mut chain_lp_tokens = CHAIN_LP_TOKENS.load(deps.storage, sender.chain_uid.clone())?;

    let pair = state.pair.clone();

    // Verify that ratio of assets provided is equal to the ratio of assets in the pool
    let ratio =
        Decimal256::checked_from_ratio(token_1_liquidity, token_2_liquidity).map_err(|err| {
            ContractError::Generic {
                err: err.to_string(),
            }
        })?;

    let mut total_reserve_1 = BALANCES.load(deps.storage, pair.token_1.clone())?;
    let mut total_reserve_2 = BALANCES.load(deps.storage, pair.token_2.clone())?;

    // Lets get lq ratio, it will be the current ratio of token reserves or if its first time then it will be ratio of tokens provided
    let lq_ratio =
        Decimal256::checked_from_ratio(total_reserve_1, total_reserve_2).unwrap_or(ratio);

    // Verify slippage tolerance is between 0 and 100
    ensure!(
        slippage_tolerance.le(&100),
        ContractError::InvalidSlippageTolerance {}
    );

    assert_slippage_tolerance(ratio, lq_ratio, slippage_tolerance)?;

    // Calculate liquidity added share for LP provider from total liquidity
    let lp_allocation = calculate_lp_allocation(
        token_1_liquidity,
        token_2_liquidity,
        total_reserve_1,
        total_reserve_2,
        state.total_lp_tokens,
    )?;

    ensure!(
        !lp_allocation.is_zero(),
        ContractError::Generic {
            err: "LP Allocation cannot be zero".to_string()
        }
    );

    chain_lp_tokens = chain_lp_tokens.checked_add(lp_allocation)?;
    CHAIN_LP_TOKENS.save(deps.storage, sender.chain_uid.clone(), &chain_lp_tokens)?;

    // Add to total liquidity and total lp allocation
    total_reserve_1 = total_reserve_1.checked_add(token_1_liquidity)?;
    total_reserve_2 = total_reserve_2.checked_add(token_2_liquidity)?;

    state.total_lp_tokens = state.total_lp_tokens.checked_add(lp_allocation)?;
    STATE.save(deps.storage, &state)?;

    BALANCES.save(deps.storage, pair.token_1.clone(), &total_reserve_1)?;

    BALANCES.save(deps.storage, pair.token_2.clone(), &total_reserve_2)?;

    // Add current balance to SNAPSHOT MAP

    // Prepare Liquidity Response
    let liquidity_response = AddLiquidityResponse {
        mint_lp_tokens: lp_allocation,
        vlp_address: env.contract.address.to_string(),
    };

    // Prepare acknowledgement
    let acknowledgement = to_json_binary(&liquidity_response)?;

    let pool = Pool {
        pair,
        reserve_1: total_reserve_1,
        reserve_2: total_reserve_2,
    };

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            &sender.to_sender_string(),
            TxType::AddLiquidity,
        ))
        .add_event(liquidity_event(&pool, &tx_id))
        .add_attribute("action", "add_liquidity")
        .add_attribute("sender", sender.to_sender_string())
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
    info: MessageInfo,
    sender: CrossChainUser,
    lp_allocation: Uint128,
    tx_id: String,
) -> Result<Response, ContractError> {
    // Get the pool for the chain_id provided
    let mut state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.router, ContractError::Unauthorized {});
    let pair = state.pair.clone();

    let mut total_reserve_1 = BALANCES.load(deps.storage, pair.token_1.clone())?;
    let mut total_reserve_2 = BALANCES.load(deps.storage, pair.token_2.clone())?;

    // Remove chain lp tokens from the sender, remove liquidity only works for a single chain remove liquidity
    let mut chain_lp_tokens = CHAIN_LP_TOKENS.load(deps.storage, sender.chain_uid.clone())?;
    chain_lp_tokens = chain_lp_tokens.checked_sub(lp_allocation)?;
    CHAIN_LP_TOKENS.save(deps.storage, sender.chain_uid.clone(), &chain_lp_tokens)?;

    // Fetch allocated liquidity to LP tokens
    let lp_tokens = state.total_lp_tokens;
    let lp_share = Decimal::checked_from_ratio(lp_allocation, lp_tokens)
        .map_err(|err| ContractError::new(&err.to_string()))?;

    // Calculate tokens_1 to send
    let token_1_liquidity = total_reserve_1.checked_mul_ceil(lp_share)?;
    // Calculate tokens_2 to send
    let token_2_liquidity = total_reserve_2.checked_mul_ceil(lp_share)?;

    total_reserve_1 = total_reserve_1.checked_sub(token_1_liquidity)?;
    total_reserve_2 = total_reserve_2.checked_sub(token_2_liquidity)?;

    BALANCES.save(deps.storage, pair.token_1.clone(), &total_reserve_1)?;

    BALANCES.save(deps.storage, pair.token_2.clone(), &total_reserve_2)?;

    state.total_lp_tokens = state.total_lp_tokens.checked_sub(lp_allocation)?;
    STATE.save(deps.storage, &state)?;

    // Prepare Liquidity Response
    let liquidity_response = VlpRemoveLiquidityResponse {
        token_1_liquidity_released: token_1_liquidity,
        token_2_liquidity_released: token_2_liquidity,
        burn_lp_tokens: lp_allocation,
        tx_id: tx_id.clone(),
        sender: sender.clone(),
        vlp_address: env.contract.address.to_string(),
    };

    // Prepare acknowledgement
    let acknowledgement = to_json_binary(&liquidity_response)?;

    let pool = Pool {
        pair: pair.clone(),
        reserve_1: total_reserve_1,
        reserve_2: total_reserve_2,
    };

    let vlp_cross_chain_struct = CrossChainUser {
        address: env.contract.address.to_string(),
        chain_uid: ChainUid::vsl_chain_uid()?,
    };

    let token_1_transfer_msg = pair.token_1.create_virtual_balance_transfer_msg(
        state.virtual_balance.clone(),
        token_1_liquidity,
        vlp_cross_chain_struct.clone(),
        sender.clone(),
    )?;

    let token_2_transfer_msg = pair.token_2.create_virtual_balance_transfer_msg(
        state.virtual_balance,
        token_2_liquidity,
        vlp_cross_chain_struct,
        sender.clone(),
    )?;

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            &sender.to_sender_string(),
            TxType::RemoveLiquidity,
        ))
        .add_submessage(SubMsg::reply_always(
            token_1_transfer_msg,
            VIRTUAL_BALANCE_TRANSFER_REPLY_ID,
        ))
        .add_submessage(SubMsg::reply_always(
            token_2_transfer_msg,
            VIRTUAL_BALANCE_TRANSFER_REPLY_ID,
        ))
        .add_event(liquidity_event(&pool, &tx_id))
        .add_attribute("action", "remove_liquidity")
        .add_attribute("sender", sender.to_sender_string())
        .add_attribute("token_1_removed_liquidity", token_1_liquidity)
        .add_attribute("token_2_removed_liquidity", token_2_liquidity)
        .add_attribute("burn_lp", lp_allocation)
        .set_data(acknowledgement))
}

pub fn execute_swap(
    deps: DepsMut,
    env: Env,
    sender: CrossChainUser,
    asset_in: Token,
    amount_in: Uint128,
    min_token_out: Uint128,
    tx_id: String,
    next_swaps: Vec<NextSwapVlp>,
    test_fail: Option<bool>,
) -> Result<Response, ContractError> {
    ensure!(
        !test_fail.unwrap_or(false),
        ContractError::new("Force fail flag")
    );
    // Verify that the asset amount is non-zero
    ensure!(!amount_in.is_zero(), ContractError::ZeroAssetAmount {});

    let state = state::STATE.load(deps.storage)?;

    let pair = state.pair.clone();

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

    let mut token_in_reserve = BALANCES.load(deps.storage, asset_in.clone())?;
    let mut token_out_reserve = BALANCES.load(deps.storage, asset_out.clone())?;

    // Router mints new tokens or this vlp gets new balance from token transfer by previous, so virtual_balance_balance = amount_in + pool_current_liquidity
    let vlp_virtual_balance_balance: euclid::msgs::virtual_balance::GetBalanceResponse =
        deps.querier.query_wasm_smart(
            state.virtual_balance.clone(),
            &euclid::msgs::virtual_balance::QueryMsg::GetBalance {
                balance_key: BalanceKey {
                    cross_chain_user: CrossChainUser {
                        address: env.contract.address.to_string(),
                        chain_uid: ChainUid::vsl_chain_uid()?,
                    },
                    token_id: asset_in.to_string(),
                },
            },
        )?;

    ensure!(
        vlp_virtual_balance_balance.amount == token_in_reserve.checked_add(amount_in)?,
        ContractError::new("Swap didn't receive any funds!")
    );

    let receive_amount = calculate_swap(swap_amount, token_in_reserve, token_out_reserve)?;

    // Verify that the receive amount is greater than 0 to be eligible for any swap
    ensure!(
        !receive_amount.is_zero(),
        ContractError::SlippageExceeded {
            amount: receive_amount,
            min_amount_out: min_token_out,
        }
    );

    token_in_reserve = token_in_reserve
        .checked_add(swap_amount)?
        .checked_add(lp_fee)?;
    token_out_reserve = token_out_reserve.checked_sub(receive_amount)?;

    BALANCES.save(deps.storage, asset_in.clone(), &token_in_reserve)?;
    BALANCES.save(deps.storage, asset_out.clone(), &token_out_reserve)?;

    let pool = Pool {
        pair: state.pair.clone(),
        reserve_1: if state.pair.token_1 == asset_in {
            token_in_reserve
        } else {
            token_out_reserve
        },
        reserve_2: if state.pair.token_1 == asset_out {
            token_out_reserve
        } else {
            token_in_reserve
        },
    };

    // Finalize ack response to swap pool
    let swap_response = VlpSwapResponse {
        sender: sender.clone(),
        tx_id: tx_id.clone(),
        asset_out,
        amount_out: receive_amount,
    };

    // Prepare acknowledgement
    let acknowledgement = to_json_binary(&swap_response)?;

    let mut response = Response::new();

    if !euclid_fee.is_zero() {
        let euclid_fee_transfer_msg =
            euclid::msgs::virtual_balance::ExecuteMsg::Transfer(ExecuteTransfer {
                amount: euclid_fee,
                token_id: asset_in.to_string(),

                // Source Address
                from: CrossChainUser {
                    address: env.contract.address.to_string(),
                    chain_uid: ChainUid::vsl_chain_uid()?,
                },

                // Destination Address
                to: fee.recipient,
            });

        let euclid_fee_transfer_msg = WasmMsg::Execute {
            contract_addr: state.virtual_balance.clone(),
            msg: to_json_binary(&euclid_fee_transfer_msg)?,
            funds: vec![],
        };

        let euclid_fee_transfer_msg =
            SubMsg::reply_on_error(euclid_fee_transfer_msg, VIRTUAL_BALANCE_TRANSFER_REPLY_ID);

        response = response.add_submessage(euclid_fee_transfer_msg);
    }

    match next_swaps.split_first() {
        Some((next_swap, forward_swaps)) => {
            // There are more swaps
            let virtual_balance_transfer_msg =
                euclid::msgs::virtual_balance::ExecuteMsg::Transfer(ExecuteTransfer {
                    amount: swap_response.amount_out,
                    token_id: swap_response.asset_out.to_string(),

                    from: CrossChainUser {
                        address: env.contract.address.to_string(),
                        chain_uid: ChainUid::vsl_chain_uid()?,
                    },

                    to: CrossChainUser {
                        address: next_swap.vlp_address.clone(),
                        chain_uid: ChainUid::vsl_chain_uid()?,
                    },
                });

            let virtual_balance_transfer_msg = WasmMsg::Execute {
                contract_addr: state.virtual_balance.clone(),
                msg: to_json_binary(&virtual_balance_transfer_msg)?,
                funds: vec![],
            };

            let virtual_balance_transfer_msg = SubMsg::reply_on_error(
                virtual_balance_transfer_msg,
                VIRTUAL_BALANCE_TRANSFER_REPLY_ID,
            );

            let next_swap_msg = euclid::msgs::vlp::ExecuteMsg::Swap {
                sender: sender.clone(),
                // Final user address and chain id

                // Carry forward amount to next swap
                asset_in: swap_response.asset_out,
                amount_in: swap_response.amount_out,
                min_token_out,
                tx_id: tx_id.clone(),
                next_swaps: forward_swaps.to_vec(),
                test_fail: next_swap.test_fail,
            };
            let next_swap_msg = WasmMsg::Execute {
                contract_addr: next_swap.vlp_address.clone(),
                msg: to_json_binary(&next_swap_msg)?,
                funds: vec![],
            };

            let next_swap_msg = SubMsg::reply_always(next_swap_msg, NEXT_SWAP_REPLY_ID);

            response = response
                .add_attribute("swap_type", "forward_swap")
                .add_attribute("forward_to", next_swap.vlp_address.clone())
                .add_submessage(virtual_balance_transfer_msg)
                .add_submessage(next_swap_msg);
        }
        None => {
            //Its the last swap

            // Verify that the receive amount is >= min amount as its last swap
            ensure!(
                receive_amount.ge(&min_token_out),
                ContractError::SlippageExceeded {
                    amount: receive_amount,
                    min_amount_out: min_token_out,
                }
            );

            let virtual_balance_transfer_msg =
                euclid::msgs::virtual_balance::ExecuteMsg::Transfer(ExecuteTransfer {
                    amount: swap_response.amount_out,
                    token_id: swap_response.asset_out.to_string(),

                    // Source Address
                    from: CrossChainUser {
                        address: env.contract.address.to_string(),
                        chain_uid: ChainUid::vsl_chain_uid()?,
                    },

                    // Destination Address
                    to: sender.clone(),
                });

            let virtual_balance_transfer_msg = WasmMsg::Execute {
                contract_addr: state.virtual_balance.clone(),
                msg: to_json_binary(&virtual_balance_transfer_msg)?,
                funds: vec![],
            };

            let virtual_balance_transfer_msg = SubMsg::reply_on_error(
                virtual_balance_transfer_msg,
                VIRTUAL_BALANCE_TRANSFER_REPLY_ID,
            );

            response = response
                .add_attribute("swap_type", "final_swap")
                .add_attribute("receiver_address", sender.address.clone())
                .add_attribute("receiver_chain_id", sender.chain_uid.to_string())
                .add_submessage(virtual_balance_transfer_msg);
        }
    };

    Ok(response
        .add_event(tx_event(&tx_id, &sender.to_sender_string(), TxType::Swap))
        .add_event(liquidity_event(&pool, &tx_id))
        .add_attribute("action", "swap")
        .add_attribute("amount_in", amount_in)
        .add_attribute("total_fee", total_fee)
        .add_attribute("euclid_fee", euclid_fee)
        .add_attribute("lp_fee", lp_fee)
        .add_attribute("receive_amount", receive_amount)
        .set_data(acknowledgement))
}

pub fn update_fee(
    deps: DepsMut,
    info: MessageInfo,
    lp_fee_bps: Option<u64>,
    euclid_fee_bps: Option<u64>,
    recipient: Option<CrossChainUser>,
) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.admin, ContractError::Unauthorized {});

    state.fee.lp_fee_bps = lp_fee_bps.unwrap_or(state.fee.lp_fee_bps);
    ensure!(
        state.fee.lp_fee_bps.le(&MAX_FEE_BPS),
        ContractError::new("LP Fee cannot exceed maximum limit")
    );
    state.fee.euclid_fee_bps = euclid_fee_bps.unwrap_or(state.fee.euclid_fee_bps);
    ensure!(
        state.fee.euclid_fee_bps.le(&MAX_FEE_BPS),
        ContractError::new("Euclid Fee cannot exceed maximum limit")
    );
    state.fee.recipient = recipient.unwrap_or(state.fee.recipient);

    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_event(simple_event())
        .add_attribute("action", "update_fee"))
}
