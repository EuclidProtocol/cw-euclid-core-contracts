use crate::state::VLP_TO_POOL;
use cosmwasm_std::{DepsMut, Reply, Response, SubMsgResult};
use cw0::parse_reply_instantiate_data;
use euclid::error::ContractError;
use euclid::msgs::pool::{GetVLPResponse, QueryMsg as PoolQueryMessage};

pub const INSTANTIATE_REPLY_ID: u64 = 1u64;

pub fn on_pool_instantiate_reply(deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::PoolInstantiateFailed { err }),
        SubMsgResult::Ok(..) => {
            let instantiate_data: cw0::MsgInstantiateContractResponse =
                parse_reply_instantiate_data(msg).unwrap();

            let pool_address = instantiate_data.contract_address;
            let vlp_address: GetVLPResponse = deps
                .querier
                .query_wasm_smart(pool_address.clone(), &PoolQueryMessage::GetVlp {})?;
            VLP_TO_POOL.save(deps.storage, vlp_address.vlp.clone(), &pool_address)?;
            Ok(Response::new()
                .add_attribute("action", "reply_pool_instantiate")
                .add_attribute("pool", pool_address)
                .add_attribute("vlp", vlp_address.vlp))
        }
    }
}
