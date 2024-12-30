use cosmwasm_std::{
    coin, ensure, from_json, to_json_binary, DepsMut, Env, MessageInfo, Response, SubMsg, Uint128,
    WasmMsg,
};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use euclid::{
    error::ContractError,
    events::simple_event,
    msgs::hook::{EuclidForwardSwap, EuclidReceive},
    token::TokenType,
};
use forwarding::msgs::astroport::{Cw20HookMsg, SwapMsg};

use astroport::router::ExecuteMsg as AstroportExecuteMsg;

use crate::{
    reply::ASTRO_SWAP_REPLY_ID,
    state::{ForwardingState, FORWARDING_STATE, STATE},
};

pub fn execute_cw20_receive(
    deps: &mut DepsMut,
    env: &Env,
    info: &MessageInfo,
    msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let amount = msg.amount;
    let from_token = TokenType::Smart {
        contract_address: info.sender.to_string(),
    };

    let msg: Cw20HookMsg = from_json(msg.msg)?;
    match msg {
        Cw20HookMsg::EuclidReceive(euclid_receive) => match euclid_receive {
            EuclidReceive::ForwardSwap(swap_msg) => {
                let event =
                    simple_event().add_attribute("meta", swap_msg.meta.clone().unwrap_or_default());
                let response = swap(deps, env, info, swap_msg, from_token, amount)?;
                Ok(response.add_event(event))
            }
        },
    }
}

pub fn execute_forward(
    deps: &mut DepsMut,
    env: &Env,
    info: &MessageInfo,
    euclid_receive: EuclidReceive,
) -> Result<Response, ContractError> {
    match euclid_receive {
        EuclidReceive::ForwardSwap(swap_msg) => {
            ensure!(
                info.funds.len() == 1,
                ContractError::new("only one token is supported")
            );
            let from_token = TokenType::Native {
                denom: info.funds[0].denom.to_string(),
            };
            let from_amount = info.funds[0].amount;

            let event =
                simple_event().add_attribute("meta", swap_msg.meta.clone().unwrap_or_default());

            let response = swap(deps, env, info, swap_msg, from_token, from_amount)?;
            Ok(response.add_event(event))
        }
    }
}

pub fn swap(
    deps: &mut DepsMut,
    env: &Env,
    _info: &MessageInfo,
    msg: EuclidForwardSwap,
    from_token: TokenType,
    from_amount: Uint128,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    let swap_msg: SwapMsg = from_json(msg.data.clone())?;

    let operations = swap_msg.operations.clone();
    ensure!(
        !operations.is_empty(),
        ContractError::new("min 1 operation is required")
    );

    let astro_execute_msg = AstroportExecuteMsg::ExecuteSwapOperations {
        operations,
        minimum_receive: Some(swap_msg.minimum_receive),
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
            funds: vec![coin(from_amount.u128(), denom)],
        },
        TokenType::Smart { contract_address } => {
            let send_msg = Cw20ExecuteMsg::Send {
                contract: state.astro_router_address.to_string(),
                amount: from_amount,
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
        .add_submessage(SubMsg::reply_always(msg, ASTRO_SWAP_REPLY_ID)))
}
