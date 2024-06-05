use cosmwasm_std::{
    ensure, from_json, to_json_binary, CosmosMsg, DepsMut, Env, IbcMsg, IbcTimeout, MessageInfo,
    Response, Uint128,
};
use cw20::Cw20ReceiveMsg;
use euclid::{
    error::ContractError,
    liquidity,
    msgs::pool::Cw20HookMsg,
    pool::LiquidityResponse,
    swap::{self, SwapResponse},
    timeout::get_timeout,
    token::{PairInfo, Token, TokenInfo},
};
use euclid_ibc::msg::ChainIbcExecuteMsg;

use crate::state::{
    generate_liquidity_req, generate_pool_req, generate_swap_req, PENDING_LIQUIDITY, PENDING_SWAPS,
    POOL_STATE, STATE,
};

// Function to send IBC request to Router in VLS to create a new pool
pub fn execute_request_pool_creation(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    pair_info: PairInfo,
    timeout: Option<u64>,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(
        state.hub_channel.is_some(),
        ContractError::Generic {
            err: "Hub Channel doesn't exist".to_string()
        }
    );
    let channel = state.hub_channel.unwrap();

    // Create a Request in state
    let pool_request = generate_pool_req(deps, &info.sender, env.block.chain_id, channel.clone())?;

    let timeout = get_timeout(timeout)?;

    // Create IBC packet to send to Router
    let ibc_packet = IbcMsg::SendPacket {
        channel_id: channel.clone(),
        data: to_json_binary(&ChainIbcExecuteMsg::RequestPoolCreation {
            pool_rq_id: pool_request.pool_rq_id,
            pair_info,
        })?,

        timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(timeout)),
    };

    Ok(Response::new()
        .add_attribute("method", "request_pool_creation")
        .add_message(ibc_packet))
}

// Function to send IBC request to Router in VLS to perform a swap
fn execute_swap(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    asset: Token,
    asset_amount: Uint128,
    min_amount_out: Uint128,
    swap_id: String,
    timeout: Option<u64>,
    vlp_address: String,
) -> Result<Response, ContractError> {
    // Load the state
    let state = STATE.load(deps.storage)?;

    ensure!(
        state.hub_channel.is_some(),
        ContractError::Generic {
            err: "Hub Channel doesn't exist".to_string()
        }
    );
    let channel = state.hub_channel.unwrap();

    let pool_address = info.sender;

    let timeout = get_timeout(timeout)?;

    // Create IBC packet to send to Router
    let ibc_packet = IbcMsg::SendPacket {
        channel_id: channel.clone(),
        data: to_json_binary(&ChainIbcExecuteMsg::Swap {
            chain_id: state.chain_id,
            asset,
            asset_amount,
            min_amount_out,
            channel,
            swap_id,
            pool_address,
            vlp_address,
        })?,
        timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(timeout)),
    };

    let msg = CosmosMsg::Ibc(ibc_packet);

    Ok(Response::new()
        .add_attribute("method", "request_pool_creation")
        .add_message(msg))
}

// Function to send IBC request to Router in VLS to add liquidity to a pool
fn execute_add_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    token_1_liquidity: Uint128,
    token_2_liquidity: Uint128,
    slippage_tolerance: u64,
    liquidity_id: String,
    timeout: Option<u64>,
    vlp_address: String,
) -> Result<Response, ContractError> {
    // Load the state
    let state = STATE.load(deps.storage)?;

    ensure!(
        state.hub_channel.is_some(),
        ContractError::Generic {
            err: "Hub Channel doesn't exist".to_string()
        }
    );
    let channel = state.hub_channel.unwrap();

    let pool_address = info.sender.clone();

    let timeout = get_timeout(timeout)?;

    // Create IBC packet to send to Router
    let ibc_packet = IbcMsg::SendPacket {
        channel_id: channel.clone(),
        data: to_json_binary(&ChainIbcExecuteMsg::AddLiquidity {
            token_1_liquidity,
            token_2_liquidity,
            slippage_tolerance,
            liquidity_id,
            pool_address: pool_address.clone().to_string(),
            vlp_address,
        })?,
        timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(timeout)),
    };

    let msg = CosmosMsg::Ibc(ibc_packet);

    Ok(Response::new()
        .add_attribute("method", "add_liquidity_request")
        .add_message(msg))
}

// Function to update the pool code ID
pub fn execute_update_pool_code_id(
    deps: DepsMut,
    info: MessageInfo,
    new_pool_code_id: u64,
) -> Result<Response, ContractError> {
    // Load the state
    STATE.update(deps.storage, |mut state| -> Result<_, ContractError> {
        // Ensure that only the admin can update the pool code ID
        ensure!(info.sender == state.admin, ContractError::Unauthorized {});

        // Update the pool code ID
        state.pool_code_id = new_pool_code_id;
        Ok(state)
    })?;

    Ok(Response::new()
        .add_attribute("method", "update_pool_code_id")
        .add_attribute("new_pool_code_id", new_pool_code_id.to_string()))
}

// Pool Functions //

// TODO make execute_swap an internal function OR merge execute_swap_request and execute_swap into one function

pub fn execute_swap_request(
    deps: &mut DepsMut,
    info: MessageInfo,
    env: Env,
    asset: TokenInfo,
    asset_amount: Uint128,
    min_amount_out: Uint128,
    msg_sender: Option<String>,
    timeout: Option<u64>,
) -> Result<Response, ContractError> {
    let state = POOL_STATE.load(deps.storage)?;

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
    let swap_info = generate_swap_req(
        deps.branch(),
        sender,
        asset,
        asset_out,
        asset_amount,
        timeout,
    )?;

    execute_swap(
        deps.branch(),
        env,
        info,
        token,
        asset_amount,
        min_amount_out,
        swap_info.swap_id,
        Some(timeout_duration),
        state.vlp_contract,
    )

    // let msg = FactoryExecuteMsg::ExecuteSwap {
    //     asset: token,
    //     asset_amount,
    //     min_amount_out,
    //     swap_id: swap_info.swap_id,
    //     timeout: Some(timeout_duration),
    //     vlp_address: state.vlp_contract,
    // };

    // let msg = WasmMsg::Execute {
    //     contract_addr: state.factory_contract,
    //     msg: to_json_binary(&msg)?,
    //     funds: vec![],
    // };

    // Ok(Response::new()
    //     .add_attribute("method", "execute_swap_request")
    //     .add_message(msg))
}

pub fn execute_complete_swap(
    deps: DepsMut,
    swap_response: SwapResponse,
) -> Result<Response, ContractError> {
    let mut state = POOL_STATE.load(deps.storage)?;
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
    POOL_STATE.save(deps.storage, &state)?;

    // Prepare messages to send tokens to user
    let msg = swap_info
        .asset_out
        .create_transfer_msg(swap_response.amount_out, extracted_swap_id.sender)?;

    // Look through pending swaps for one with the same swap_id
    Ok(Response::new().add_message(msg))
}

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
    mut deps: DepsMut,
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
                &mut deps,
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
// TODO look into alternatives of using .branch(), maybe unifying the functions would help
pub fn add_liquidity_request(
    mut deps: DepsMut,
    info: MessageInfo,
    env: Env,
    token_1_liquidity: Uint128,
    token_2_liquidity: Uint128,
    slippage_tolerance: u64,
    msg_sender: Option<String>,
    timeout: Option<u64>,
) -> Result<Response, ContractError> {
    let state = POOL_STATE.load(deps.storage)?;

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
        generate_liquidity_req(deps.branch(), sender, token_1_liquidity, token_2_liquidity)?;

    execute_add_liquidity(
        deps.branch(),
        env,
        info,
        token_1_liquidity,
        token_2_liquidity,
        slippage_tolerance,
        liquidity_info.liquidity_id,
        timeout,
        state.vlp_contract,
    )

    // let factory_msg = factory::ExecuteMsg::AddLiquidity {
    //     token_1_liquidity,
    //     token_2_liquidity,
    //     slippage_tolerance,
    //     liquidity_id: liquidity_info.liquidity_id,
    //     timeout,
    //     vlp_address: state.vlp_contract,
    // };

    // let msg = WasmMsg::Execute {
    //     contract_addr: state.factory_contract,
    //     msg: to_json_binary(&factory_msg)?,
    //     funds: vec![],
    // };

    // msgs.push(CosmosMsg::Wasm(msg));

    // Ok(Response::new()
    //     .add_attribute("method", "add_liquidity_request")
    //     .add_attribute("token_1_liquidity", token_1_liquidity)
    //     .add_attribute("token_2_liquidity", token_2_liquidity)
    //     .add_messages(msgs))
}

// Function to execute after LiquidityResponse acknowledgment
pub fn execute_complete_add_liquidity(
    deps: DepsMut,
    liquidity_response: LiquidityResponse,
    liquidity_id: String,
) -> Result<Response, ContractError> {
    let mut state = POOL_STATE.load(deps.storage)?;

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
    POOL_STATE.save(deps.storage, &state)?;

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
    let state = POOL_STATE.load(deps.storage)?;

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
