use crate::contract::{CONTRACT_NAME, CONTRACT_VERSION};
use crate::state::{State, STATE};
use cosmwasm_schema::cw_serde;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{DepsMut, Env, Response};
use cw2::set_contract_version;
use cw_storage_plus::Item;
use euclid::{error::ContractError, msgs::factory::MigrateMsg};

#[cw_serde]
struct LegacyState {
    pub router_contract: String,
    pub relayer_contract: cosmwasm_std::Addr,
    pub escrow_code_id: u64,
    pub lp_code_id: u64,
    pub chain_uid: euclid::chain::ChainUid,
    pub is_native: bool,
}

/// This is the migrate entry point for the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, msg: MigrateMsg) -> Result<Response, ContractError> {
    let migrated = if STATE.load(deps.storage).is_ok() {
        false
    } else {
        let legacy_state = Item::<LegacyState>::new("state").load(deps.storage)?;
        let state = State {
            router_contract: legacy_state.router_contract,
            relayer_contract: legacy_state.relayer_contract,
            escrow_code_id: legacy_state.escrow_code_id,
            lp_code_id: legacy_state.lp_code_id,
            position_token_code_id: msg
                .position_token_code_id
                .ok_or(ContractError::new("Position token code ID not set"))?,
            chain_uid: legacy_state.chain_uid,
            is_native: legacy_state.is_native,
        };
        STATE.save(deps.storage, &state)?;
        true
    };

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    euclid::build_info::set_build_info(deps.storage)?;
    Ok(Response::new()
        .add_attribute("method", "migrate")
        .add_attribute("admins_migrated", migrated.to_string()))
}
