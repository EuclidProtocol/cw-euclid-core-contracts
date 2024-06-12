use crate::state::TOKEN_TO_ESCROW;
use cosmwasm_std::{from_json, DepsMut, Reply, Response, SubMsgResult};
use cw0::parse_reply_instantiate_data;
use euclid::error::ContractError;

pub const ESCROW_INSTANTIATE_REPLY_ID: u64 = 1u64;

pub fn on_escrow_instantiate_reply(deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::PoolInstantiateFailed { err }),
        SubMsgResult::Ok(..) => {
            let instantiate_data: cw0::MsgInstantiateContractResponse =
                parse_reply_instantiate_data(msg).map_err(|res| ContractError::Generic {
                    err: res.to_string(),
                })?;

            let escrow_address = deps.api.addr_validate(&instantiate_data.contract_address)?;
            let escrow_data: euclid::msgs::escrow::EscrowInstantiateResponse =
                from_json(instantiate_data.data.unwrap_or_default())?;

            TOKEN_TO_ESCROW.save(deps.storage, escrow_data.token, &escrow_address)?;
            Ok(Response::new()
                .add_attribute("action", "reply_pool_instantiate")
                .add_attribute("escrow", escrow_address)
                .add_attribute("token_id", escrow_data.token.id))
        }
    }
}
