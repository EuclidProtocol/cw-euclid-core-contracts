use cosmwasm_std::{from_json, DepsMut, Reply, Response, SubMsgResult};
use cw_utils::parse_instantiate_response_data;
use euclid::error::ContractError;

use crate::state::VLP_TO_LP_TOKEN;

/// Pool factory owns its own reply ID namespace, disjoint from main Factory's,
/// so reply collisions across the two contracts are structurally impossible.
pub const LP_INSTANTIATE_REPLY_ID: u64 = 1001;
pub const VLP_INSTANTIATE_REPLY_ID: u64 = 1002;
pub const POSITION_TOKEN_INSTANTIATE_REPLY_ID: u64 = 1003;

pub fn on_lp_instantiate_reply(deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result {
        SubMsgResult::Err(err) => Err(ContractError::Reply {
            action: "lp_instantiate_reply".to_string(),
            err,
        }),
        SubMsgResult::Ok(res) => {
            let data = res
                .msg_responses
                .into_iter()
                .next()
                .map(|r| r.value)
                .ok_or(ContractError::new(
                    "lp instantiate reply missing msg_responses",
                ))?;
            let instantiate_data: cw_utils::MsgInstantiateContractResponse =
                parse_instantiate_response_data(&data)
                    .map_err(|e| ContractError::Generic { err: e.to_string() })?;
            let cw20_address = deps.api.addr_validate(&instantiate_data.contract_address)?;
            let vlp: String = from_json(instantiate_data.data.unwrap_or_default())?;
            VLP_TO_LP_TOKEN.save(deps.storage, vlp.clone(), &cw20_address)?;
            Ok(Response::new()
                .add_attribute("method", "lp_instantiate_reply")
                .add_attribute("vlp", vlp)
                .add_attribute("lp_token", cw20_address))
        }
    }
}
