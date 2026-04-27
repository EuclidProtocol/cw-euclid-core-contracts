use cosmwasm_std::{
    coin, ensure, from_json, to_json_binary, Coin, DepsMut, Env, MessageInfo, Response, SubMsg,
    Uint128, WasmMsg,
};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use forwarding::msgs::{
    common_old::{EuclidReceive, TokenType},
    cw20::OsmosisCw20HookMsg,
    errors_old::ContractError,
    euclid_receive::OsmosisEuclidReceiveHook,
    osmosis::SwapMsg,
};
use swaprouter::msg::ExecuteMsg as OsmosisExecuteMsg;

// use osmosis::ExecuteMsg as OsmosisExecuteMsg;

use crate::{
    reply::OSMO_SWAP_REPLY_ID,
    state::{ForwardingState, FORWARDING_STATE, STATE},
};

pub fn execute_cw20_receive(
    deps: &mut DepsMut,
    env: &Env,
    info: &MessageInfo,
    receive_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    ensure!(
        info.funds.is_empty(),
        ContractError::new("No funds allowed")
    );
    let amount = receive_msg.amount;
    let from_token = TokenType::Smart {
        contract_address: info.sender.to_string(),
    };

    let msg: OsmosisCw20HookMsg = from_json(receive_msg.msg)?;
    match msg {
        OsmosisCw20HookMsg::EuclidReceive(euclid_receive) => receive_euclid_cw20(
            deps,
            env,
            info,
            receive_msg.sender.to_string(),
            euclid_receive,
            amount,
        ),
        OsmosisCw20HookMsg::Swap(swap_msg) => swap(deps, env, swap_msg, from_token, amount),
    }
}

pub fn receive_euclid_native(
    deps: &mut DepsMut,
    env: &Env,
    info: &MessageInfo,
    euclid_receive: EuclidReceive,
) -> Result<Response, ContractError> {
    match from_json::<OsmosisEuclidReceiveHook>(euclid_receive.data.clone())? {
        OsmosisEuclidReceiveHook::Swap(swap_msg) => {
            let response = crate::contract::execute(
                deps.branch(),
                env.clone(),
                info.clone(),
                forwarding::msgs::osmosis::ExecuteMsg::Swap(swap_msg),
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
    amount: Uint128,
) -> Result<Response, ContractError> {
    match from_json::<OsmosisEuclidReceiveHook>(euclid_receive.data.clone())? {
        OsmosisEuclidReceiveHook::Swap(swap_msg) => {
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
    from_amount: Uint128,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;

    let input_coin = match from_token {
        TokenType::Native { ref denom } => Coin {
            denom: denom.to_string(),
            amount: from_amount,
        },
        TokenType::Smart {
            ref contract_address,
        } => Coin {
            denom: contract_address.to_string(),
            amount: from_amount,
        },
        _ => return Err(ContractError::new("from token can't be a voucher")),
    };

    let output_denom = &swap_msg.to_token.get_denom()?;

    let osmo_route = swap_msg
        .route
        .iter()
        .map(|route| route.clone().into())
        .collect();

    let osmo_execute_msg = OsmosisExecuteMsg::Swap {
        input_coin,
        output_denom: output_denom.clone(),
        slippage: swap_msg.slippage.clone(),
        route: Some(osmo_route),
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
            swap_msg: swap_msg.clone(),
        },
    )?;

    let msg = match &from_token {
        TokenType::Native { denom } => WasmMsg::Execute {
            contract_addr: state.osmo_router_address.to_string(),
            msg: to_json_binary(&osmo_execute_msg)?,
            funds: vec![coin(from_amount.u128(), denom)],
        },
        TokenType::Smart { contract_address } => {
            let send_msg = Cw20ExecuteMsg::Send {
                contract: state.osmo_router_address.to_string(),
                amount: from_amount,
                msg: to_json_binary(&osmo_execute_msg)?,
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
        .add_attribute("dex", "osmosis")
        .add_attribute("start_swap_amount", from_amount)
        .add_attribute("start_swap_token", from_token.get_key())
        .add_submessage(SubMsg::reply_on_success(msg, OSMO_SWAP_REPLY_ID)))
}
