use cosmwasm_std::{ensure, to_json_binary, DepsMut, Env, MessageInfo, Response, SubMsg, WasmMsg};
use euclid::{error::ContractError, fee::Fee, msgs, token::PairInfo};

use crate::{
    reply::{VLP_INSTANTIATE_REPLY_ID, VLP_POOL_REGISTER_REPLY_ID},
    state::{STATE, VLPS},
};

// Function to send IBC request to Router in VLS to create a new pool
pub fn execute_request_pool_creation(
    deps: DepsMut,
    env: Env,
    chain_id: String,
    factory: String,
    pair_info: PairInfo,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;

    let pair = (pair_info.token_1.get_token(), pair_info.token_2.get_token());

    let vlp = VLPS.may_load(deps.storage, pair)?;

    if vlp.is_none() {
        let pair = (pair_info.token_2.get_token(), pair_info.token_1.get_token());
        ensure!(
            VLPS.load(deps.storage, pair).is_err(),
            ContractError::Generic {
                err: "pair order is reversed".to_string()
            }
        );
    }

    let register_msg = msgs::vlp::ExecuteMsg::RegisterPool {
        chain_id,
        factory,
        pair_info: pair_info.clone(),
    };

    if vlp.is_some() {
        let msg = WasmMsg::Execute {
            contract_addr: vlp.unwrap(),
            msg: to_json_binary(&register_msg)?,
            funds: vec![],
        };
        Ok(Response::new().add_submessage(SubMsg::reply_always(msg, VLP_POOL_REGISTER_REPLY_ID)))
    } else {
        let instantiate_msg = msgs::vlp::InstantiateMsg {
            router: env.contract.address.to_string(),
            pair: pair_info,
            fee: Fee {
                lp_fee: 0,
                treasury_fee: 0,
                staker_fee: 0,
            },
            execute: Some(register_msg),
        };
        let msg = WasmMsg::Instantiate {
            admin: Some(env.contract.address.to_string()),
            code_id: state.vlp_code_id,
            msg: to_json_binary(&instantiate_msg)?,
            funds: vec![],
            label: "VLP".to_string(),
        };
        Ok(Response::new().add_submessage(SubMsg::reply_always(msg, VLP_INSTANTIATE_REPLY_ID)))
    }
}

// Function to update the pool code ID
pub fn execute_update_vlp_code_id(
    deps: DepsMut,
    info: MessageInfo,
    new_vlp_code_id: u64,
) -> Result<Response, ContractError> {
    // Load the state
    STATE.update(deps.storage, |mut state| -> Result<_, ContractError> {
        // Ensure that only the admin can update the pool code ID
        if info.sender != state.admin {
            return Err(ContractError::Unauthorized {});
        }

        // Update the pool code ID
        state.vlp_code_id = new_vlp_code_id;
        Ok(state)
    })?;

    Ok(Response::new()
        .add_attribute("method", "update_pool_code_id")
        .add_attribute("new_vlp_code_id", new_vlp_code_id.to_string()))
}
