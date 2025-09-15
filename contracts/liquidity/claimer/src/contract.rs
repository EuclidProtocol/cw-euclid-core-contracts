#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{ensure, to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response};

use cw2::set_contract_version;
use euclid::error::ContractError;
use euclid::msgs::claimer::{ExecuteMsg, InstantiateMsg, QueryMsg, State};
use euclid::msgs::factory;

use crate::execute::{execute_claim_voucher, execute_virtual_balance_receive};
use crate::query::{
    get_claim, get_claim_by_pseudo_claim_id, get_claims_by_claimer_pubkey, get_claims_by_group_id,
    get_claims_by_sender,
};
use crate::{execute::execute_update_admin, query::get_state, state::STATE};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:euclid-claimer";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    let factory_state_query = factory::QueryMsg::GetState {};
    let factory_state: factory::msg::StateResponse = deps
        .querier
        .query_wasm_smart(msg.factory_address.clone(), &factory_state_query)?;

    // Only support native factory as of now
    ensure!(
        factory_state.is_native,
        ContractError::Generic {
            err: "Factory is not native".to_string(),
        }
    );
    let state = State {
        factory_address: msg.factory_address.clone(),
        voucher_address: msg.voucher_address.clone(),
        chain_uid: factory_state.chain_uid,
        admin: info.sender,
    };
    STATE.save(deps.storage, &state)?;
    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("chain_uid", state.chain_uid.to_string())
        .add_attribute("voucher_address", msg.voucher_address.to_string())
        .add_attribute("factory_address", msg.factory_address.to_string()))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::ClaimVoucher(msg) => execute_claim_voucher(&mut deps, &info, msg),
        ExecuteMsg::VirtualBalanceReceive(msg) => {
            execute_virtual_balance_receive(&mut deps, &env, &info, msg)
        }
        ExecuteMsg::UpdateAdmin(msg) => execute_update_admin(&mut deps, &info, msg),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::GetState {} => Ok(to_json_binary(&get_state(&deps)?)?),
        QueryMsg::GetSenderClaims {
            sender,
            limit,
            offset,
        } => Ok(to_json_binary(&get_claims_by_sender(
            &deps, sender, limit, offset,
        )?)?),
        QueryMsg::GetUserClaims {
            pub_key,
            limit,
            offset,
        } => Ok(to_json_binary(&get_claims_by_claimer_pubkey(
            &deps, pub_key, limit, offset,
        )?)?),
        QueryMsg::GetClaim { claim_id } => Ok(to_json_binary(&get_claim(&deps, claim_id)?)?),
        QueryMsg::GetClaimsByClaimerPubkey {
            pub_key,
            limit,
            offset,
        } => Ok(to_json_binary(&get_claims_by_claimer_pubkey(
            &deps, pub_key, limit, offset,
        )?)?),
        QueryMsg::GetClaimsByGroupId {
            group_id,
            limit,
            offset,
        } => Ok(to_json_binary(&get_claims_by_group_id(
            &deps, group_id, limit, offset,
        )?)?),
        QueryMsg::GetClaimByPseudoClaimId { pseudo_claim_id } => Ok(to_json_binary(
            &get_claim_by_pseudo_claim_id(&deps, pseudo_claim_id)?,
        )?),
    }
}
