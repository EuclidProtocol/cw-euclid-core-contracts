use cosmwasm_std::{
    ensure, from_json, to_json_binary, CosmosMsg, DepsMut, Env, IbcMsg, IbcTimeout, MessageInfo,
    Response, Uint128, WasmMsg,
};
use cw20::Cw20ReceiveMsg;
use euclid::{
    error::ContractError,
    msgs::pool::Cw20HookMsg,
    pool::LiquidityResponse,
    swap::{extract_sender, LiquidityTxInfo, SwapInfo, SwapResponse},
    token::TokenInfo,
};
use euclid_ibc::msg::{self, IbcExecuteMsg};

use crate::state::{get_liquidity_info, get_swap_info, PENDING_LIQUIDITY, PENDING_SWAPS, STATE};

use euclid::msgs::factory::ExecuteMsg as FactoryExecuteMsg;

pub fn execute_swap_request(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    asset: TokenInfo,
    asset_amount: Uint128,
    min_amount_out: Uint128,
    channel: String,
    msg_sender: Option<String>,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;

    // if `msg_sender` is not None, then the sender is the one who initiated the swap
    let sender = match msg_sender {
        Some(sender) => sender,
        None => info.sender.clone().to_string(),
    };

    // Verify that the asset exists in the pool
    if asset != state.pair_info.token_1 && asset != state.pair_info.token_2 {
        return Err(ContractError::AssetDoesNotExist {});
    }

    // Verify that the asset amount is greater than 0
    if asset_amount.is_zero() {
        return Err(ContractError::ZeroAssetAmount {});
    }

    // Verify that the min amount out is greater than 0
    if min_amount_out.is_zero() {
        return Err(ContractError::ZeroAssetAmount {});
    }

    // Verify if the token is native
    if asset.is_native() {
        // Get the denom of native token
        let denom = asset.get_denom();

        // Verify thatthe amount of funds passed is greater than the asset amount
        if info.funds.iter().find(|x| x.denom == denom).unwrap().amount < asset_amount {
            return Err(ContractError::Unauthorized {});
        }
    } else {
        // Verify that the contract address is the same as the asset contract address
        if info.sender != asset.get_contract_address() {
            return Err(ContractError::Unauthorized {});
        }
    }

    // Get token from tokenInfo
    let token = asset.get_token();

    // Generate a unique identifier for this swap
    let swap_id = format!(
        "{}-{}-{}",
        sender,
        env.block.height,
        env.transaction.unwrap().index
    );

    let msg = FactoryExecuteMsg::ExecuteSwap {
        asset: token,
        asset_amount,
        min_amount_out,
        channel,
        swap_id: swap_id.clone(),
    };

    let msg = WasmMsg::Execute {
        contract_addr: state.factory_contract,
        msg: to_json_binary(&msg)?,
        funds: vec![],
    };

    // Get alternative token
    let asset_out: TokenInfo = state.pair_info.get_other_token(asset.clone());

    // Add the deposit to Pending Swaps
    let swap_info = SwapInfo {
        asset: asset.clone(),
        asset_out: asset_out.clone(),
        asset_amount,
        timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(60)),
        swap_id,
    };

    // Load previous pending swaps for user
    let mut pending_swaps = PENDING_SWAPS
        .may_load(deps.storage, sender.to_string())?
        .unwrap_or_default();

    // Append the new swap to the list
    pending_swaps.push(swap_info);

    // Save the new list of pending swaps
    PENDING_SWAPS.save(deps.storage, sender.clone().to_string(), &pending_swaps)?;

    Ok(Response::new()
        .add_attribute("method", "execute_swap_request")
        .add_message(msg))
}

// Function execute_swap that routes the swap request to the appropriate function
pub fn execute_complete_swap(
    deps: DepsMut,
    swap_response: SwapResponse,
) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;
    // Verify that assets exist in the state.
    ensure!(
        swap_response.asset.exists(state.clone().pair),
        ContractError::AssetDoesNotExist {}
    );

    ensure!(
        swap_response.asset_out.exists(state.clone().pair),
        ContractError::AssetDoesNotExist {}
    );

    // Fetch the sender from swap_id
    let sender = extract_sender(&swap_response.swap_id);

    // Validate that the pending swap exists for the sender
    let pending_swaps = PENDING_SWAPS.load(deps.storage, sender.clone())?;

    // Get swap id info
    let swap_info = get_swap_info(&swap_response.swap_id, pending_swaps.clone());

    // Pop the swap from the pending swaps
    let mut new_pending_swaps = pending_swaps.clone();
    new_pending_swaps.retain(|x| x.swap_id != swap_response.swap_id);

    // Update the pending swaps
    PENDING_SWAPS.save(deps.storage, sender.clone(), &new_pending_swaps)?;

    // Check if asset is token_1 or token_2 and calculate accordingly
    if swap_response.asset == state.clone().pair.token_1 {
        state.reserve_1 += swap_response.asset_amount;
        state.reserve_2 -= swap_response.amount_out;
    } else {
        state.reserve_2 += swap_response.asset_amount;
        state.reserve_1 -= swap_response.amount_out;
    };

    // Save the updated state
    STATE.save(deps.storage, &state)?;

    // Prepare messages to send tokens to user
    let msg = swap_info
        .asset_out
        .create_transfer_msg(swap_response.amount_out, sender);

    // Look through pending swaps for one with the same swap_id
    Ok(Response::new().add_message(msg))
}

// Function execute_swap that routes the swap request to the appropriate function
pub fn execute_reject_swap(
    deps: DepsMut,
    swap_id: String,
    error: Option<String>,
) -> Result<Response, ContractError> {
    let sender = extract_sender(&swap_id);
    // Fetch the pending swaps for the sender
    let pending_swaps = PENDING_SWAPS.load(deps.storage, sender.clone())?;
    // Get the current pending swap
    let swap_info = get_swap_info(&swap_id, pending_swaps.clone());
    // Pop this swap from the vector
    let mut new_pending_swaps = pending_swaps.clone();
    new_pending_swaps.retain(|x| x.swap_id != swap_id);
    // Update the pending swaps
    PENDING_SWAPS.save(deps.storage, sender.clone(), &new_pending_swaps)?;

    // Prepare messages to refund tokens back to user
    let msg = swap_info
        .asset
        .create_transfer_msg(swap_info.asset_amount, sender.clone());

    Ok(Response::new()
        .add_attribute("method", "process_failed_swap")
        .add_attribute("refund_to", "sender")
        .add_attribute("refund_amount", swap_info.asset_amount.clone())
        .add_attribute("error", error.unwrap_or_default())
        .add_message(msg))
}

/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
///
/// * **cw20_msg** is the CW20 message that has to be processed.
pub fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    match from_json(&cw20_msg.msg)? {
        // Allow to swap using a CW20 hook message
        Cw20HookMsg::Swap {
            asset,
            min_amount_out,
            channel,
        } => {
            let contract_adr = info.sender.clone();

            // ensure that contract address is same as asset being swapped
            ensure!(
                contract_adr == asset.get_contract_address(),
                ContractError::AssetDoesNotExist {}
            );
            // Add sender as the option

            // ensure that the contract address is the same as the asset contract address
            execute_swap_request(
                deps,
                info,
                env,
                asset,
                cw20_msg.amount,
                min_amount_out,
                channel,
                Some(cw20_msg.sender),
            )
        }
    }
}

// Add liquidity to the pool
pub fn add_liquidity_request(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    token_1_liquidity: Uint128,
    token_2_liquidity: Uint128,
    slippage_tolerance: u64,
    channel: String,
    msg_sender: Option<String>,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;

    // Check that slippage tolerance is between 1 and 100
    if !(1..=100).contains(&slippage_tolerance) {
        return Err(ContractError::InvalidSlippageTolerance {});
    }

    // if `msg_sender` is not None, then the sender is the one who initiated the swap
    let sender = match msg_sender {
        Some(sender) => sender,
        None => info.sender.clone().to_string(),
    };

    // Check that the token_1 liquidity is greater than 0
    if token_1_liquidity.is_zero() || token_2_liquidity.is_zero() {
        return Err(ContractError::ZeroAssetAmount {});
    }

    // Get the token 1 and token 2 from the pair info
    let token_1 = state.pair_info.token_1.clone();
    let token_2 = state.pair_info.token_2.clone();

    // Prepare msg vector
    let mut msgs: Vec<CosmosMsg> = Vec::new();

    // IF TOKEN IS A SMART CONTRACT IT REQUIRES APPROVAL FOR TRANSFER
    if token_1.is_smart() {
        let msg = token_1
            .create_transfer_msg(token_1_liquidity, env.contract.address.clone().to_string());
        msgs.push(msg);
    } else {
        // If funds empty return error
        if info.funds.is_empty() {
            return Err(ContractError::InsufficientDeposit {});
        }
        // Check for funds sent with the message
        let amt = info
            .funds
            .iter()
            .find(|x| x.denom == token_1.get_denom())
            .unwrap();
        if amt.amount < token_1_liquidity {
            return Err(ContractError::InsufficientDeposit {});
        }
    }

    // Same for token 2
    if token_2.is_smart() {
        let msg = token_2
            .create_transfer_msg(token_2_liquidity, env.contract.address.clone().to_string());
        msgs.push(msg);
    } else {
        if info.funds.is_empty() {
            return Err(ContractError::InsufficientDeposit {});
        }
        let amt = info
            .funds
            .iter()
            .find(|x| x.denom == token_2.get_denom())
            .unwrap();
        if amt.amount < token_2_liquidity {
            return Err(ContractError::InsufficientDeposit {});
        }
    }

    // Save the pending liquidity transaction
    let liquidity_id = format!(
        "{}-{}-{}",
        info.sender,
        env.block.height,
        env.transaction.unwrap().index
    );
    // Create new Liquidity Info
    let liquidity_info: LiquidityTxInfo = LiquidityTxInfo {
        sender: sender.clone(),
        token_1_liquidity,
        token_2_liquidity,
        liquidity_id: liquidity_id.clone(),
    };

    // Store Liquidity Info
    let mut pending_liquidity = PENDING_LIQUIDITY
        .may_load(deps.storage, sender.clone())?
        .unwrap_or_default();
    pending_liquidity.push(liquidity_info);
    PENDING_LIQUIDITY.save(deps.storage, sender.clone(), &pending_liquidity)?;

    let ibc_packet = IbcMsg::SendPacket {
        channel_id: channel.clone(),
        data: to_json_binary(&IbcExecuteMsg::AddLiquidity {
            chain_id: state.chain_id.clone(),
            token_1_liquidity,
            token_2_liquidity,
            slippage_tolerance,
            liquidity_id: liquidity_id.clone(),
        })
        .unwrap(),
        timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(60)),
    };

    msgs.push(CosmosMsg::Ibc(ibc_packet));

    Ok(Response::new()
        .add_attribute("method", "add_liquidity_request")
        .add_attribute("token_1_liquidity", token_1_liquidity)
        .add_attribute("token_2_liquidity", token_2_liquidity)
        .add_messages(msgs))
}

// Function to execute after LiquidityResponse acknowledgment
pub fn execute_complete_add_liquidity(
    deps: DepsMut,
    liquidity_response: LiquidityResponse,
    liquidity_id: String,
) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;

    // Unpack response
    // Fetch the sender from liquidity_id
    let sender = extract_sender(&liquidity_id);
    // Fetch the pending liquidity transactions for the sender
    let pending_liquidity = PENDING_LIQUIDITY.load(deps.storage, sender.clone())?;
    // Get the current pending liquidity transaction
    let _liquidity_info = get_liquidity_info(&liquidity_id, pending_liquidity.clone());
    // Pop this liquidity transaction from the vector
    let mut new_pending_liquidity = pending_liquidity.clone();
    new_pending_liquidity.retain(|x| x.liquidity_id != liquidity_id);
    // Update the pending liquidity transactions
    PENDING_LIQUIDITY.save(deps.storage, sender.clone(), &new_pending_liquidity)?;

    // Update the state with the new reserves
    state.reserve_1 += liquidity_response.token_1_liquidity;
    state.reserve_2 += liquidity_response.token_2_liquidity;

    // Save the updated state
    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("method", "process_add_liquidity")
        .add_attribute("sender", sender.clone())
        .add_attribute("liquidity_id", liquidity_id.clone()))
}

// Function to execute after LiquidityResponse acknowledgment
pub fn execute_reject_add_liquidity(
    deps: DepsMut,
    liquidity_id: String,
    error: Option<String>,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;

    // Fetch the 2 tokens
    let token_1 = state.pair_info.token_1;
    let token_2 = state.pair_info.token_2;
    let sender = extract_sender(&liquidity_id);
    // Fetch the pending liquidity transactions for the sender
    let pending_liquidity = PENDING_LIQUIDITY.load(deps.storage, sender.clone())?;
    // Get the current pending liquidity transaction
    let liquidity_info = get_liquidity_info(&liquidity_id, pending_liquidity.clone());
    // Pop this liquidity transaction from the vector
    let mut new_pending_liquidity = pending_liquidity.clone();
    new_pending_liquidity.retain(|x| x.liquidity_id != liquidity_id);
    // Update the pending liquidity transactions
    PENDING_LIQUIDITY.save(deps.storage, sender.clone(), &new_pending_liquidity)?;

    // Prepare messages to refund tokens back to user
    let mut msgs: Vec<CosmosMsg> = Vec::new();
    let msg = token_1
        .clone()
        .create_transfer_msg(liquidity_info.token_1_liquidity, sender.clone());
    msgs.push(msg);
    let msg = token_2
        .clone()
        .create_transfer_msg(liquidity_info.token_2_liquidity, sender.clone());
    msgs.push(msg);

    Ok(Response::new()
        .add_attribute("method", "liquidity_tx_err_refund")
        .add_attribute("sender", sender.clone())
        .add_attribute("liquidity_id", liquidity_id.clone())
        .add_attribute("error", error.unwrap_or_default())
        .add_messages(msgs))
}
