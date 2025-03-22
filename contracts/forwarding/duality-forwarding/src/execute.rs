use std::str::FromStr;

use cosmwasm_std::{
    coin, ensure, from_json, to_json_binary, Coin, DepsMut, Env, MessageInfo, Response, SubMsg,
    Uint128, WasmMsg,
};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use euclid::{
    error::ContractError, events::simple_event, msgs::hook::EuclidReceive, token::TokenType,
};
use forwarding::msgs::{duality::SwapMsg, euclid_receive::DualityEuclidReceiveHook};
// use swaprouter::msg::ExecuteMsg as DualityExecuteMsg;
use neutron_std::types::neutron::dex::MsgMultiHopSwap as DualityExecuteMsg;
use neutron_std::types::neutron::dex::MultiHopRoute;
use neutron_std::types::neutron::util::precdec::PrecDec;

use crate::{
    reply::DUALITY_SWAP_REPLY_ID,
    state::{ForwardingState, FORWARDING_STATE, STATE},
};

pub fn execute_cw20_receive(
    _deps: &mut DepsMut,
    _env: &Env,
    _info: &MessageInfo,
    _receive_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    unimplemented!()
}

pub fn receive_euclid_native(
    deps: &mut DepsMut,
    env: &Env,
    info: &MessageInfo,
    euclid_receive: EuclidReceive,
) -> Result<Response, ContractError> {
    match from_json::<DualityEuclidReceiveHook>(euclid_receive.data.clone())? {
        DualityEuclidReceiveHook::Swap(swap_msg) => {
            let response = crate::contract::execute(
                deps.branch(),
                env.clone(),
                info.clone(),
                forwarding::msgs::duality::ExecuteMsg::Swap(swap_msg),
            )?;
            let event = simple_event().add_attribute(
                "meta",
                euclid_receive.meta.clone().unwrap_or("no_meta".to_string()),
            );
            Ok(response.add_event(event))
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
    unimplemented!()
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
        _ => return Err(ContractError::new("from token can't be a voucher")),
    };

    let output_denom = &swap_msg.to_token.get_denom()?;

    let hops = vec![input_coin.denom.clone(), output_denom.clone()];
    let route = MultiHopRoute { hops };

    // amount_in * limit_sell_price > 1
    let true_limit_price: PrecDec = PrecDec::one()
        .checked_div(PrecDec::from_str(&input_coin.amount.to_string()).unwrap())
        .unwrap()
        .checked_mul(PrecDec::from_str("1.0001").unwrap())
        .unwrap();

    let duality_execute_msg = DualityExecuteMsg {
        creator: env.contract.address.to_string(),
        receiver: env.contract.address.to_string(),
        routes: vec![route],
        amount_in: from_amount.to_string(),
        exit_limit_price: true_limit_price.to_prec_dec_string(),
        pick_best_route: true,
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
            contract_addr: state.duality_router_address.to_string(),
            msg: to_json_binary(&duality_execute_msg)?,
            funds: vec![coin(from_amount.u128(), denom)],
        },
        TokenType::Smart { contract_address } => {
            let send_msg = Cw20ExecuteMsg::Send {
                contract: state.duality_router_address.to_string(),
                amount: from_amount,
                msg: to_json_binary(&duality_execute_msg)?,
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
        .add_attribute("dex", "duality")
        .add_attribute("start_swap_amount", from_amount)
        .add_attribute("start_swap_token", from_token.get_key())
        .add_submessage(SubMsg::reply_on_success(msg, DUALITY_SWAP_REPLY_ID)))
}
