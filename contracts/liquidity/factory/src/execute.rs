use cosmwasm_std::{
    ensure, to_json_binary, CosmosMsg, DepsMut, Env, IbcBasicResponse, IbcMsg, IbcTimeout,
    MessageInfo, Response, SubMsg, Uint128, WasmMsg,
};
use cw20::Cw20ReceiveMsg;
use euclid::{
    error::ContractError,
    swap::NextSwap,
    timeout::get_timeout,
    token::{PairInfo, Token, TokenInfo},
};
use euclid_ibc::msg::ChainIbcExecuteMsg;

use crate::state::{
    generate_liquidity_req, generate_pool_req, generate_swap_req, STATE, TOKEN_TO_ESCROW,
    VLP_TO_POOL,
};

// Function to send IBC request to Router in VLS to create a new pool
pub fn execute_request_pool_creation(
    mut deps: DepsMut,
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
    let pool_request = generate_pool_req(
        &mut deps,
        &info.sender,
        env.block.chain_id,
        channel.clone(),
        pair_info.clone(),
    )?;

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

// TODO make execute_swap an internal function OR merge execute_swap_request and execute_swap into one function

pub fn execute_swap_request(
    deps: &mut DepsMut,
    info: MessageInfo,
    env: Env,
    asset_in: TokenInfo,
    asset_out: TokenInfo,
    amount_in: Uint128,
    min_amount_out: Uint128,
    swaps: Vec<NextSwap>,
    msg_sender: Option<String>,
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

    let first_swap = swaps.first().ok_or(ContractError::Generic {
        err: "Empty Swap not allowed".to_string(),
    })?;

    // Verify that this asset is allowed
    let escrow = TOKEN_TO_ESCROW.load(deps.storage, asset_in.get_token())?;

    let token_allowed: euclid::msgs::escrow::AllowedTokenResponse = deps.querier.query_wasm_smart(
        escrow,
        &euclid::msgs::escrow::QueryMsg::TokenAllowed {
            token: asset_in.clone(),
        },
    )?;

    ensure!(
        token_allowed.allowed,
        ContractError::UnsupportedDenomination {}
    );

    let pair = VLP_TO_POOL.load(deps.storage, first_swap.vlp_address.clone());

    ensure!(
        pair.is_ok(),
        ContractError::Generic {
            err: "vlp not registered with this chain".to_string()
        }
    );

    let pair = pair?;

    // if `msg_sender` is not None, then the sender is the one who initiated the swap
    let sender = match msg_sender {
        Some(sender) => sender,
        None => info.sender.clone().to_string(),
    };

    // Verify that the asset exists in the pool
    ensure!(
        asset_in == pair.token_1 || asset_in == pair.token_2,
        ContractError::AssetDoesNotExist {}
    );

    // Verify that the asset amount is greater than 0
    ensure!(!amount_in.is_zero(), ContractError::ZeroAssetAmount {});

    // Verify that the min amount out is greater than 0
    ensure!(!min_amount_out.is_zero(), ContractError::ZeroAssetAmount {});

    // Verify if the token is native
    if asset_in.is_native() {
        // Get the denom of native token
        let denom = asset_in.get_denom();

        // Verify thatthe amount of funds passed is greater than the asset amount
        if info
            .funds
            .iter()
            .find(|x| x.denom == denom)
            .ok_or(ContractError::Generic {
                err: "Denom not found".to_string(),
            })?
            .amount
            < amount_in
        {
            return Err(ContractError::Unauthorized {});
        }
    } else {
        // Verify that the contract address is the same as the asset contract address
        ensure!(
            info.sender == asset_in.get_denom(),
            ContractError::Unauthorized {}
        );
    }

    let timeout_duration = get_timeout(timeout)?;
    let timeout = IbcTimeout::with_timestamp(env.block.time.plus_seconds(timeout_duration));
    let swap_info = generate_swap_req(
        deps.branch(),
        sender.clone(),
        asset_in.clone(),
        asset_out,
        amount_in,
        min_amount_out,
        swaps.clone(),
        timeout.clone(),
    )?;

    // Create IBC packet to send to Router
    let ibc_packet = IbcMsg::SendPacket {
        channel_id: channel.clone(),
        data: to_json_binary(&ChainIbcExecuteMsg::Swap {
            to_address: sender,
            to_chain_id: state.chain_id,
            asset_in: asset_in.get_token(),
            amount_in,
            min_amount_out,
            swap_id: swap_info.swap_id,
            swaps,
        })?,
        timeout,
    };

    let msg = CosmosMsg::Ibc(ibc_packet);

    Ok(Response::new()
        .add_attribute("method", "execute_request_swap")
        .add_message(msg))
}

/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
///
/// * **cw20_msg** is the CW20 message that has to be processed.
pub fn receive_cw20(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    // match from_json(&cw20_msg.msg)? {
    //     // Allow to swap using a CW20 hook message
    //     Cw20HookMsg::Swap {
    //         asset,
    //         min_amount_out,
    //         timeout,
    //     } => {
    //         let contract_adr = info.sender.clone();

    //         // ensure that contract address is same as asset being swapped
    //         ensure!(
    //             contract_adr == asset.get_contract_address(),
    //             ContractError::AssetDoesNotExist {}
    //         );
    //         // Add sender as the option

    //         // ensure that the contract address is the same as the asset contract address
    //         execute_swap_request(
    //             &mut deps,
    //             info,
    //             env,
    //             asset,
    //             cw20_msg.amount,
    //             min_amount_out,
    //             Some(cw20_msg.sender),
    //             timeout,
    //         )
    //     }
    //     Cw20HookMsg::Deposit {} => {}
    // }
    Err(ContractError::NotImplemented {})
}

// Add liquidity to the pool
// TODO look into alternatives of using .branch(), maybe unifying the functions would help
pub fn add_liquidity_request(
    mut deps: DepsMut,
    info: MessageInfo,
    env: Env,
    vlp_address: String,
    token_1_liquidity: Uint128,
    token_2_liquidity: Uint128,
    slippage_tolerance: u64,
    msg_sender: Option<String>,
    timeout: Option<u64>,
) -> Result<Response, ContractError> {
    // Load the state
    let state = STATE.load(deps.storage)?;
    dbg!(&state);

    ensure!(
        state.hub_channel.is_some(),
        ContractError::Generic {
            err: "Hub Channel doesn't exist".to_string()
        }
    );
    let channel = state.hub_channel.unwrap();
    dbg!(&channel);

    let pool_address = info.sender.clone();

    let pair_info = VLP_TO_POOL.load(deps.storage, vlp_address.clone())?;
    dbg!(&pair_info);

    // Check that slippage tolerance is between 1 and 100
    ensure!(
        (1..=100).contains(&slippage_tolerance),
        ContractError::InvalidSlippageTolerance {}
    );
    dbg!(slippage_tolerance);

    // if `msg_sender` is not None, then the sender is the one who initiated the swap
    let sender = match msg_sender {
        Some(sender) => sender,
        None => info.sender.clone().to_string(),
    };
    dbg!(&sender);

    // Check that the liquidity is greater than 0
    ensure!(
        !token_1_liquidity.is_zero() && !token_2_liquidity.is_zero(),
        ContractError::ZeroAssetAmount {}
    );
    dbg!(token_1_liquidity, token_2_liquidity);

    // Get the token 1 and token 2 from the pair info
    let token_1 = pair_info.token_1.clone();
    let token_2 = pair_info.token_2.clone();
    dbg!(&token_1, &token_2);

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
        dbg!(&info.funds);

        // Check for funds sent with the message
        let amt = info
            .funds
            .iter()
            .find(|x| x.denom == token_1.get_denom())
            .ok_or(ContractError::Generic {
                err: "Denom not found".to_string(),
            })?;
        dbg!(&amt);

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
        dbg!(&amt);

        ensure!(
            amt.amount.ge(&token_2_liquidity),
            ContractError::InsufficientDeposit {}
        );
    }

    // Save the pending liquidity transaction
    let liquidity_info = generate_liquidity_req(
        deps.branch(),
        sender,
        token_1_liquidity,
        token_2_liquidity,
        vlp_address.clone(),
        pair_info,
    )?;
    dbg!(&liquidity_info);

    let timeout_duration = get_timeout(timeout)?;
    let timeout = IbcTimeout::with_timestamp(env.block.time.plus_seconds(timeout_duration));
    dbg!(&timeout);

    // Create IBC packet to send to Router
    let ibc_packet = IbcMsg::SendPacket {
        channel_id: channel.clone(),
        data: to_json_binary(&ChainIbcExecuteMsg::AddLiquidity {
            token_1_liquidity,
            token_2_liquidity,
            slippage_tolerance,
            liquidity_id: liquidity_info.liquidity_id,
            pool_address: pool_address.clone().to_string(),
            vlp_address,
        })?,
        timeout,
    };
    dbg!(&ibc_packet);

    let msg = CosmosMsg::Ibc(ibc_packet);

    Ok(Response::new()
        .add_attribute("method", "add_liquidity_request")
        .add_message(msg))
}

// New factory functions //
pub fn execute_request_add_allowed_denom(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    token: Token,
    denom: String,
) -> Result<Response, ContractError> {
    let escrow_address = TOKEN_TO_ESCROW.may_load(deps.storage, token.clone())?;
    match escrow_address {
        Some(escrow_address) => {
            let msg = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: escrow_address.into_string(),
                msg: to_json_binary(&euclid::msgs::escrow::ExecuteMsg::AddAllowedDenom {
                    denom: denom.clone(),
                })?,
                funds: vec![],
            });
            Ok(Response::new()
                .add_submessage(SubMsg::new(msg))
                .add_attribute("method", "request_add_allowed_denom")
                .add_attribute("token", token.id)
                .add_attribute("denom", denom))
        }
        None => Err(ContractError::EscrowDoesNotExist {}),
    }
}

pub fn execute_request_deregister_denom(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    token: Token,
    denom: String,
) -> Result<Response, ContractError> {
    let escrow_address = TOKEN_TO_ESCROW.may_load(deps.storage, token.clone())?;
    match escrow_address {
        Some(escrow_address) => {
            let msg = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: escrow_address.into_string(),
                msg: to_json_binary(&euclid::msgs::escrow::ExecuteMsg::DisallowDenom {
                    denom: denom.clone(),
                })?,
                funds: vec![],
            });
            Ok(Response::new()
                .add_submessage(SubMsg::new(msg))
                .add_attribute("method", "request_disallow_denom")
                .add_attribute("token", token.id)
                .add_attribute("denom", denom))
        }
        None => Err(ContractError::EscrowDoesNotExist {}),
    }
}

pub fn execute_request_disallow_denom(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    token: Token,
    denom: String,
) -> Result<IbcBasicResponse, ContractError> {
    let escrow_address = TOKEN_TO_ESCROW.may_load(deps.storage, token.clone())?;
    match escrow_address {
        Some(escrow_address) => {
            let msg = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: escrow_address.into_string(),
                msg: to_json_binary(&euclid::msgs::escrow::ExecuteMsg::DisallowDenom {
                    denom: denom.clone(),
                })?,
                funds: vec![],
            });
            Ok(IbcBasicResponse::new()
                .add_submessage(SubMsg::new(msg))
                .add_attribute("method", "request_disallow_denom")
                .add_attribute("token", token.id)
                .add_attribute("denom", denom))
        }
        None => Err(ContractError::EscrowDoesNotExist {}),
    }
}
