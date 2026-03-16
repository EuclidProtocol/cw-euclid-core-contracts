#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response};

use cw2::set_contract_version;
use euclid::error::ContractError;
use euclid::msgs::claimer::msg::{ExecuteMsg, InstantiateMsg, QueryMsg, State};

use crate::execute::{execute_claim_voucher, execute_virtual_balance_receive};
use crate::query::{
    get_admin, get_claim, get_claim_by_pseudo_claim_id, get_claims_by_claimer_pubkey,
    get_claims_by_group_id, get_claims_by_sender, get_state,
};
use crate::{
    execute::execute_update_admin,
    state::{ADMIN, STATE},
};

// version info for migration info
pub(crate) const CONTRACT_NAME: &str = "crates.io:euclid-claimer";
pub(crate) const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let state = State {
        vcoin_address: msg.vcoin_address.clone(),
        router_contract: msg.router_contract.clone(),
    };
    STATE.save(deps.storage, &state)?;
    ADMIN.save(deps.storage, &info.sender)?;
    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("vcoin_address", msg.vcoin_address.to_string()))
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
        ExecuteMsg::VoucherReceive(msg) => {
            execute_virtual_balance_receive(&mut deps, &env, &info, msg)
        }
        ExecuteMsg::UpdateAdmin(msg) => execute_update_admin(&mut deps, &info, msg),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::GetState {} => Ok(to_json_binary(&get_state(&deps)?)?),
        QueryMsg::GetAdmin {} => Ok(to_json_binary(&get_admin(&deps)?)?),
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
