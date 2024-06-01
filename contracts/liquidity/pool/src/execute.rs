use cosmwasm_std::{
    ensure, from_json, to_json_binary, CosmosMsg, DepsMut, Env, IbcTimeout, MessageInfo, Response,
    Uint128, WasmMsg,
};
use cw20::Cw20ReceiveMsg;
use euclid::{
    error::ContractError,
    liquidity,
    msgs::{factory, pool::Cw20HookMsg},
    pool::LiquidityResponse,
    swap::{self, SwapResponse},
    timeout::get_timeout,
    token::TokenInfo,
};

use crate::state::{
    generate_liquidity_req, generate_swap_req, PENDING_LIQUIDITY, PENDING_SWAPS, STATE,
};

use euclid::msgs::factory::ExecuteMsg as FactoryExecuteMsg;

pub fn execute_swap_request(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    asset: TokenInfo,
    asset_amount: Uint128,
    min_amount_out: Uint128,
    msg_sender: Option<String>,
    timeout: Option<u64>,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;

    // if `msg_sender` is not None, then the sender is the one who initiated the swap
    let sender = match msg_sender {
        Some(sender) => sender,
        None => info.sender.clone().to_string(),
    };

    // Verify that the asset exists in the pool
    ensure!(
        asset == state.pair_info.token_1 || asset == state.pair_info.token_2,
        ContractError::AssetDoesNotExist {}
    );

    // Verify that the asset amount is greater than 0
    ensure!(!asset_amount.is_zero(), ContractError::ZeroAssetAmount {});

    // Verify that the min amount out is greater than 0
    ensure!(!min_amount_out.is_zero(), ContractError::ZeroAssetAmount {});

    // Verify if the token is native
    if asset.is_native() {
        // Get the denom of native token
        let denom = asset.get_denom();

        // Verify thatthe amount of funds passed is greater than the asset amount
        if info
            .funds
            .iter()
            .find(|x| x.denom == denom)
            .ok_or(ContractError::Generic {
                err: "Denom not found".to_string(),
            })?
            .amount
            < asset_amount
        {
            return Err(ContractError::Unauthorized {});
        }
    } else {
        // Verify that the contract address is the same as the asset contract address
        ensure!(
            info.sender == asset.get_contract_address(),
            ContractError::Unauthorized {}
        );
    }

    // Get token from tokenInfo
    let token = asset.get_token();
    // Get alternative token
    let asset_out: TokenInfo = state.pair_info.get_other_token(asset.clone());

    let timeout_duration = get_timeout(timeout)?;
    let timeout = IbcTimeout::with_timestamp(env.block.time.plus_seconds(timeout_duration));
    let swap_info = generate_swap_req(deps, sender, asset, asset_out, asset_amount, timeout)?;

    let msg = FactoryExecuteMsg::ExecuteSwap {
        asset: token,
        asset_amount,
        min_amount_out,
        swap_id: swap_info.swap_id,
        timeout: Some(timeout_duration),
        vlp_address: state.vlp_contract,
    };

    let msg = WasmMsg::Execute {
        contract_addr: state.factory_contract,
        msg: to_json_binary(&msg)?,
        funds: vec![],
    };

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
        swap_response.asset.exists(state.pair_info.get_pair()),
        ContractError::AssetDoesNotExist {}
    );

    ensure!(
        swap_response.asset_out.exists(state.pair_info.get_pair()),
        ContractError::AssetDoesNotExist {}
    );

    // Fetch the sender from swap_id
    let extracted_swap_id = swap::parse_swap_id(&swap_response.swap_id)?;

    // Validate that the pending swap exists for the sender
    let swap_info = PENDING_SWAPS.load(
        deps.storage,
        (extracted_swap_id.sender.clone(), extracted_swap_id.index),
    )?;

    // Remove this from pending swaps
    PENDING_SWAPS.remove(
        deps.storage,
        (extracted_swap_id.sender.clone(), extracted_swap_id.index),
    );

    // Check if asset is token_1 or token_2 and calculate accordingly
    if swap_response.asset == state.pair_info.token_1.get_token() {
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
        .create_transfer_msg(swap_response.amount_out, extracted_swap_id.sender)?;

    // Look through pending swaps for one with the same swap_id
    Ok(Response::new().add_message(msg))
}

// Function execute_swap that routes the swap request to the appropriate function
pub fn execute_reject_swap(
    deps: DepsMut,
    swap_id: String,
    error: Option<String>,
) -> Result<Response, ContractError> {
    let extracted_swap_id = swap::parse_swap_id(&swap_id)?;

    // Validate that the pending swap exists for the sender
    let swap_info = PENDING_SWAPS.load(
        deps.storage,
        (extracted_swap_id.sender.clone(), extracted_swap_id.index),
    )?;
    // Remove this from pending swaps
    PENDING_SWAPS.remove(
        deps.storage,
        (extracted_swap_id.sender.clone(), extracted_swap_id.index),
    );

    // Prepare messages to refund tokens back to user
    let msg = swap_info
        .asset
        .create_transfer_msg(swap_info.asset_amount, extracted_swap_id.sender)?;

    Ok(Response::new()
        .add_attribute("method", "process_failed_swap")
        .add_attribute("refund_to", "sender")
        .add_attribute("refund_amount", swap_info.asset_amount)
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
            timeout,
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
                Some(cw20_msg.sender),
                timeout,
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
    msg_sender: Option<String>,
    timeout: Option<u64>,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;

    // Check that slippage tolerance is between 1 and 100
    ensure!(
        (1..=100).contains(&slippage_tolerance),
        ContractError::InvalidSlippageTolerance {}
    );

    // if `msg_sender` is not None, then the sender is the one who initiated the swap
    let sender = match msg_sender {
        Some(sender) => sender,
        None => info.sender.clone().to_string(),
    };

    // Check that the liquidity is greater than 0
    ensure!(
        !token_1_liquidity.is_zero() && !token_2_liquidity.is_zero(),
        ContractError::ZeroAssetAmount {}
    );

    // Get the token 1 and token 2 from the pair info
    let token_1 = state.pair_info.token_1.clone();
    let token_2 = state.pair_info.token_2.clone();

    // Prepare msg vector
    let mut msgs: Vec<CosmosMsg> = Vec::new();

    // IF TOKEN IS A SMART CONTRACT IT REQUIRES APPROVAL FOR TRANSFER
    if token_1.is_smart() {
        let msg = token_1
            .create_transfer_msg(token_1_liquidity, env.contract.address.clone().to_string())?;
        msgs.push(msg);
    } else {
        // If funds empty return error
        ensure!(
            !info.funds.is_empty(),
            ContractError::InsufficientDeposit {}
        );

        // Check for funds sent with the message
        let amt = info
            .funds
            .iter()
            .find(|x| x.denom == token_1.get_denom())
            .ok_or(ContractError::Generic {
                err: "Denom not found".to_string(),
            })?;

        ensure!(
            amt.amount.ge(&token_1_liquidity),
            ContractError::InsufficientDeposit {}
        );
    }

    // Same for token 2
    if token_2.is_smart() {
        let msg = token_2
            .create_transfer_msg(token_2_liquidity, env.contract.address.clone().to_string())?;
        msgs.push(msg);
    } else {
        // If funds empty return error
        ensure!(
            !info.funds.is_empty(),
            ContractError::InsufficientDeposit {}
        );

        let amt = info
            .funds
            .iter()
            .find(|x| x.denom == token_2.get_denom())
            .ok_or(ContractError::Generic {
                err: "Denom not found".to_string(),
            })?;

        ensure!(
            amt.amount.ge(&token_2_liquidity),
            ContractError::InsufficientDeposit {}
        );
    }

    // Save the pending liquidity transaction
    let liquidity_info =
        generate_liquidity_req(deps, sender, token_1_liquidity, token_2_liquidity)?;

    let factory_msg = factory::ExecuteMsg::AddLiquidity {
        token_1_liquidity,
        token_2_liquidity,
        slippage_tolerance,
        liquidity_id: liquidity_info.liquidity_id,
        timeout,
        vlp_address: state.vlp_contract,
    };

    let msg = WasmMsg::Execute {
        contract_addr: state.factory_contract,
        msg: to_json_binary(&factory_msg)?,
        funds: vec![],
    };

    msgs.push(CosmosMsg::Wasm(msg));

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
    let parsed_liquidity_id = liquidity::parse_liquidity_id(&liquidity_id)?;
    // Fetch the pending liquidity transactions for the sender
    let pending_liquidity = PENDING_LIQUIDITY.load(
        deps.storage,
        (
            parsed_liquidity_id.sender.clone(),
            parsed_liquidity_id.index,
        ),
    )?;

    // Remove the liquidity
    PENDING_LIQUIDITY.remove(
        deps.storage,
        (
            parsed_liquidity_id.sender.clone(),
            parsed_liquidity_id.index,
        ),
    );

    // Update the state with the new reserves
    state.reserve_1 += liquidity_response.token_1_liquidity;
    state.reserve_2 += liquidity_response.token_2_liquidity;

    // Save the updated state
    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("method", "process_add_liquidity")
        .add_attribute("sender", parsed_liquidity_id.sender)
        .add_attribute("liquidity_id", liquidity_id.clone())
        .add_attribute("pending_liquidity", format!("{pending_liquidity:?}")))
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

    // Unpack response
    // Fetch the sender from liquidity_id
    let parsed_liquidity_id = liquidity::parse_liquidity_id(&liquidity_id)?;
    // Fetch the pending liquidity transactions for the sender
    let pending_liquidity = PENDING_LIQUIDITY.load(
        deps.storage,
        (
            parsed_liquidity_id.sender.clone(),
            parsed_liquidity_id.index,
        ),
    )?;

    // Prepare messages to refund tokens back to user
    let mut msgs: Vec<CosmosMsg> = Vec::new();
    let msg = token_1.clone().create_transfer_msg(
        pending_liquidity.token_1_liquidity,
        parsed_liquidity_id.sender.clone(),
    )?;
    msgs.push(msg);
    let msg = token_2.clone().create_transfer_msg(
        pending_liquidity.token_2_liquidity,
        parsed_liquidity_id.sender.clone(),
    )?;
    msgs.push(msg);

    // Remove the liquidity
    PENDING_LIQUIDITY.remove(
        deps.storage,
        (
            parsed_liquidity_id.sender.clone(),
            parsed_liquidity_id.index,
        ),
    );

    Ok(Response::new()
        .add_attribute("method", "liquidity_tx_err_refund")
        .add_attribute("sender", parsed_liquidity_id.sender)
        .add_attribute("liquidity_id", liquidity_id.clone())
        .add_attribute("error", error.unwrap_or_default())
        .add_messages(msgs))
}
