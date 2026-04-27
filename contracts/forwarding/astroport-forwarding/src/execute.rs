use cosmwasm_std::{
    coin, ensure, from_json, to_json_binary, DepsMut, Env, MessageInfo, Response, SubMsg, Uint128,
    Uint256, WasmMsg,
};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use forwarding::msgs::{
    astroport::SwapMsg,
    common_old::{EuclidReceive, TokenType},
    cw20::Cw20HookMsg,
    errors_old::ContractError,
    euclid_receive::AstroportEuclidReceiveHook,
};

use astroport::router::ExecuteMsg as AstroportExecuteMsg;

use crate::{
    reply::ASTRO_SWAP_REPLY_ID,
    state::{ForwardingState, FORWARDING_STATE, STATE},
};

pub fn execute_cw20_receive(
    deps: &mut DepsMut,
    env: &Env,
    info: &MessageInfo,
    receive_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
<<<<<<< HEAD
    let amount: Uint256 = receive_msg.amount.into();
=======
    ensure!(
        info.funds.is_empty(),
        ContractError::new("No funds allowed")
    );
    let amount = receive_msg.amount;
>>>>>>> origin/development
    let from_token = TokenType::Smart {
        contract_address: info.sender.to_string(),
    };

    let msg: Cw20HookMsg = from_json(receive_msg.msg)?;
    match msg {
        Cw20HookMsg::EuclidReceive(euclid_receive) => receive_euclid_cw20(
            deps,
            env,
            info,
            receive_msg.sender.to_string(),
            euclid_receive,
            amount,
        ),
        Cw20HookMsg::Swap(swap_msg) => swap(deps, env, swap_msg, from_token, amount),
    }
}

pub fn receive_euclid_native(
    deps: &mut DepsMut,
    env: &Env,
    info: &MessageInfo,
    euclid_receive: EuclidReceive,
) -> Result<Response, ContractError> {
    match from_json::<AstroportEuclidReceiveHook>(euclid_receive.data.clone())? {
        AstroportEuclidReceiveHook::Swap(swap_msg) => {
            let response = crate::contract::execute(
                deps.branch(),
                env.clone(),
                info.clone(),
                forwarding::msgs::astroport::ExecuteMsg::Swap(swap_msg),
            )?;
            Ok(response)
        }
    }
}

pub fn receive_euclid_cw20(
    deps: &mut DepsMut,
    env: &Env,
    _info: &MessageInfo,
    sender: String,
    euclid_receive: EuclidReceive,
    amount: Uint256,
) -> Result<Response, ContractError> {
    match from_json::<AstroportEuclidReceiveHook>(euclid_receive.data.clone())? {
        AstroportEuclidReceiveHook::Swap(swap_msg) => {
            let from_token = TokenType::Smart {
                contract_address: sender.to_string(),
            };
            let response = swap(deps, env, swap_msg, from_token, amount)?;
            Ok(response)
        }
    }
}

pub fn swap(
    deps: &mut DepsMut,
    env: &Env,
    swap_msg: SwapMsg,
    from_token: TokenType,
    from_amount: Uint256,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;

    let operations = swap_msg.operations.clone();
    ensure!(
        !operations.is_empty(),
        ContractError::new("min 1 operation is required")
    );

    let from_amount_u128 = Uint128::try_from(from_amount).unwrap();
    let astro_execute_msg = AstroportExecuteMsg::ExecuteSwapOperations {
        operations,
        minimum_receive: Some(Uint128::try_from(swap_msg.minimum_receive).unwrap()),
        // Contract will receive the tokens
        to: Some(env.contract.address.to_string()),
        max_spread: swap_msg.max_spread,
    };

    let previous_balance = swap_msg
        .to_token
        .get_balance(deps.as_ref(), env.contract.address.to_string())?;

    FORWARDING_STATE.save(
        deps.storage,
        &ForwardingState {
            from_token: from_token.clone(),
            from_amount,
            previous_balance,
            swap_msg,
        },
    )?;

    let msg = match &from_token {
        TokenType::Native { denom } => WasmMsg::Execute {
            contract_addr: state.astro_router_address.to_string(),
            msg: to_json_binary(&astro_execute_msg)?,
            funds: vec![coin(from_amount_u128.u128(), denom)],
        },
        TokenType::Smart { contract_address } => {
            let send_msg = Cw20ExecuteMsg::Send {
                contract: state.astro_router_address.to_string(),
                amount: from_amount_u128,
                msg: to_json_binary(&astro_execute_msg)?,
            };
            WasmMsg::Execute {
                contract_addr: contract_address.to_string(),
                msg: to_json_binary(&send_msg)?,
                funds: vec![],
            }
        }
        _ => return Err(ContractError::new("unsupported token type")),
    };

    Ok(Response::new()
        .add_attribute("dex", "astroport")
        .add_attribute("start_swap_amount", from_amount)
        .add_attribute("start_swap_token", from_token.get_key())
        .add_submessage(SubMsg::reply_on_success(msg, ASTRO_SWAP_REPLY_ID)))
}
