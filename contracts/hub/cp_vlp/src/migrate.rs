use cosmwasm_schema::cw_serde;
use cosmwasm_std::{entry_point, Addr, DepsMut, Env, Response, Uint128};
use cw2::set_contract_version;
use cw_storage_plus::Item;
use euclid::{
    admin::EuclidAdmin,
    error::ContractError,
    fee::{Fee, TotalFees},
    msgs::vlp::{base::State, cp::msg::MigrateMsg},
    token::Pair,
};

use crate::state::STATE;

const CONTRACT_NAME: &str = "crates.io:vlp";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cw_serde]
struct LegacyState {
    pub pair: Pair,
    pub router: Addr,
    pub virtual_balance_contract: Addr,
    pub fee: Fee,
    pub total_fees_collected: TotalFees,
    pub last_updated: u64,
    pub total_lp_tokens: Uint128,
    pub admin: String,
}

/// This is the migrate entry point for the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    let migrated = if STATE.load(deps.storage).is_ok() {
        false
    } else {
        let legacy_state = Item::<LegacyState>::new("state").load(deps.storage)?;
        let admin = deps.api.addr_validate(&legacy_state.admin)?;
        let state = State {
            pair: legacy_state.pair,
            router: legacy_state.router,
            virtual_balance_contract: legacy_state.virtual_balance_contract,
            fee: legacy_state.fee,
            total_fees_collected: legacy_state.total_fees_collected,
            last_updated: legacy_state.last_updated,
            total_lp_tokens: legacy_state.total_lp_tokens,
            admin: EuclidAdmin::default(admin),
        };
        STATE.save(deps.storage, &state)?;
        true
    };

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::new()
        .add_attribute("method", "migrate")
        .add_attribute("admins_migrated", migrated.to_string()))
}
