use cosmwasm_std::{ensure, from_json, Binary, DepsMut, Env, MessageInfo, Response};
use euclid::error::ContractError;
use euclid_ibc::{ack::AcknowledgementMsg, router_ibc::RouterCrossChainExecuteMsg};

use crate::state::{MAIN_FACTORY_ADDRESS, PAIR_TO_VLP, PENDING_POOL_REQUESTS};

/// Dispatcher for ack callbacks forwarded from main factory's IBC ack path.
/// Caller MUST be main factory. The dispatcher does not own any of the
/// per-variant business logic — it only routes to the matching handler.
pub fn on_pool_ack(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    original_msg: Binary,
    ack: Binary,
    is_native: bool,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;
    let main_factory = MAIN_FACTORY_ADDRESS.load(deps.storage)?;
    ensure!(info.sender == main_factory, ContractError::Unauthorized {});

    let msg: RouterCrossChainExecuteMsg = from_json(&original_msg)?;
    match msg {
        RouterCrossChainExecuteMsg::RequestPoolCreation { tx_id, sender, .. } => {
            ack_pool_creation(deps, sender.address, tx_id, ack, is_native)
        }
        // Future slices wire the other pool variants here.
        other => Err(ContractError::new(&format!(
            "OnPoolAck: variant not yet handled by pool_factory: {}",
            std::any::type_name_of_val(&other)
        ))),
    }
}

/// Slice 1 ack path for `RequestPoolCreation`. Currently records the pool
/// registration in `PAIR_TO_VLP` and removes the pending entry. Follow-up
/// slices add escrow/LP instantiation through main factory's proxy entries.
fn ack_pool_creation(
    deps: DepsMut,
    sender: String,
    tx_id: String,
    ack: Binary,
    is_native: bool,
) -> Result<Response, ContractError> {
    use euclid::liquidity::AddLiquidityResponse;

    let sender = deps.api.addr_validate(&sender)?;
    let req_key = (sender.clone(), tx_id.clone());
    let existing = PENDING_POOL_REQUESTS
        .may_load(deps.storage, req_key.clone())?
        .ok_or(ContractError::PoolRequestDoesNotExists { req: tx_id.clone() })?;
    PENDING_POOL_REQUESTS.remove(deps.storage, req_key);

    let res: AcknowledgementMsg<AddLiquidityResponse> = from_json(&ack)?;
    match res {
        AcknowledgementMsg::Ok(data) => {
            PAIR_TO_VLP.save(
                deps.storage,
                existing.pair_info.get_pair()?.get_tupple(),
                &data.vlp_address.clone(),
            )?;
            Ok(Response::new()
                .add_attribute("method", "pool_factory_ack_pool_creation")
                .add_attribute("tx_id", tx_id)
                .add_attribute("vlp", data.vlp_address))
        }
        AcknowledgementMsg::Error(err) => {
            if is_native {
                return Err(ContractError::new(&err));
            }
            Ok(Response::new()
                .add_attribute("method", "pool_factory_reject_pool_request")
                .add_attribute("tx_id", tx_id)
                .add_attribute("error", err))
        }
    }
}
