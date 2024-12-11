use crate::state::{REFUND_ADDRESS, REFUND_ASSETS};
use cosmwasm_std::{to_json_binary, CosmosMsg, DepsMut, Reply, Response, SubMsgResult, WasmMsg};
use cw20::Cw20ExecuteMsg;
use euclid::error::ContractError;

pub const FORWARDING_MESSAGE_REPLY_ID: u64 = 1;

pub fn handle_refund(deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => {
            let refund_address = REFUND_ADDRESS.load(deps.storage).unwrap();
            let refund_assets = REFUND_ASSETS.load(deps.storage).unwrap_or_default();

            let mut refund_msgs = Vec::new();
            for refund_asset in refund_assets {
                let refund_msg = match deps.api.addr_validate(&refund_asset.denom) {
                    Ok(cw20_address) => CosmosMsg::Wasm(WasmMsg::Execute {
                        contract_addr: cw20_address.into_string(),
                        msg: to_json_binary(&Cw20ExecuteMsg::Transfer {
                            recipient: refund_address.clone(),
                            amount: refund_asset.amount,
                        })?,
                        funds: vec![],
                    }),
                    Err(_) => CosmosMsg::Bank(cosmwasm_std::BankMsg::Send {
                        to_address: refund_address.clone(),
                        amount: vec![refund_asset],
                    }),
                };
                refund_msgs.push(refund_msg);
            }
            REFUND_ADDRESS.remove(deps.storage);
            REFUND_ASSETS.remove(deps.storage);

            Ok(Response::new()
                .add_messages(refund_msgs)
                .add_attribute("action", "forwarding_message")
                .add_attribute("error", err))
        }
        SubMsgResult::Ok(_res) => {
            REFUND_ADDRESS.remove(deps.storage);
            REFUND_ASSETS.remove(deps.storage);
            Ok(Response::new().add_attribute("action", "forwarding_message"))
        }
    }
}
