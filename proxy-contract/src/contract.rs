use cosmwasm_std::{
    entry_point, to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, StdError,
    StdResult, SubMsg, SubMsgResponse, SubMsgResult,
};
use euclid::token::{Token, TokenType};

use crate::error::ContractError;
use crate::state::{State, EXECUTE_INSTANTIATE_REPLY_ID, STATE};
use euclid::msgs::proxy::{ExecuteMsg, InstantiateMsg, QueryMsg};
use secret_toolkit::utils::InitCallback;

#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    let state = State {
        escrow_code_hash: msg.escrow_code_hash,
        escrow_code_id: msg.escrow_code_id,
        factory: info.sender.into_string(),
    };

    STATE.save(deps.storage, &state)?;

    Ok(Response::default())
}

#[entry_point]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::InitializeEscrow {
            token_id,
            allowed_denom,
        } => try_instantiate(deps, info, env, token_id, allowed_denom),
    }
}

pub fn try_instantiate(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    token_id: Token,
    allowed_denom: Option<TokenType>,
) -> StdResult<Response> {
    let state = STATE.load(deps.storage)?;
    if info.sender != state.factory {
        return Err(StdError::generic_err("Unauthorised"));
    }
    let msg = euclid::msgs::escrow::InstantiateMsg {
        token_id,
        allowed_denom,
    };

    let submsg = SubMsg::reply_always(
        msg.to_cosmos_msg(
            None,
            format!("escrow - {:?}", env.contract.address),
            state.escrow_code_id,
            state.escrow_code_hash,
            None,
        )?,
        EXECUTE_INSTANTIATE_REPLY_ID,
    );

    Ok(Response::new().add_submessage(submsg))
}

#[entry_point]
pub fn query(_deps: Deps, _env: Env, _msg: QueryMsg) -> StdResult<Binary> {
    unimplemented!()
}

#[entry_point]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        EXECUTE_INSTANTIATE_REPLY_ID => handle_instantiate_reply(deps, msg),
        id => Err(ContractError::UnexpectedReplyId { id }),
    }
}

fn handle_instantiate_reply(_deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result {
        SubMsgResult::Ok(res) => {
            let address = parse_reply_address_from_event(res);
            Ok(Response::new()
                .add_attribute("reply_on_proxy_escrow_init", "success")
                .set_data(to_binary(&address)?))
        }
        SubMsgResult::Err(e) => Ok(Response::new()
            .add_attribute("reply_on_proxy_escrow_init", "error")
            .add_attribute("error", e)),
    }
}

pub fn parse_reply_address_from_event(res: SubMsgResponse) -> String {
    let mut address = String::new();
    let mut found_address = false;

    for event in &res.events {
        if event.ty == "instantiate" {
            for attribute in &event.attributes {
                if attribute.key == "contract_address" || attribute.key == "_contract_addr" {
                    address.clone_from(&attribute.value);
                    found_address = true;
                    break;
                }
            }
        }
        if found_address {
            break;
        }
    }
    address
}
