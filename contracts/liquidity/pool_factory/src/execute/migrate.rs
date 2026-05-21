use cosmwasm_std::{ensure, Addr, DepsMut, Env, MessageInfo, Response};
use euclid::{error::ContractError, token::Pair};

use crate::state::{MAIN_FACTORY_ADDRESS, MIGRATION_ACCEPTED, PAIR_TO_VLP, VLP_TO_LP_TOKEN};

/// One-shot migration accept. Only callable by main factory, only once.
/// In Slice 1 the payload is intentionally narrow (just CP/Stable pool maps);
/// later slices extend it with the concentrated and pending-queue items.
pub fn migrate_accept_pool_state(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    pair_to_vlp: Vec<(Pair, String)>,
    vlp_to_lp_token: Vec<(String, Addr)>,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;
    let main_factory = MAIN_FACTORY_ADDRESS.load(deps.storage)?;
    ensure!(info.sender == main_factory, ContractError::Unauthorized {});

    let already = MIGRATION_ACCEPTED.may_load(deps.storage)?.unwrap_or(false);
    ensure!(!already, ContractError::new("Migration already accepted"));

    for (pair, vlp) in pair_to_vlp {
        PAIR_TO_VLP.save(deps.storage, pair.get_tupple(), &vlp)?;
    }
    for (vlp, lp_token) in vlp_to_lp_token {
        VLP_TO_LP_TOKEN.save(deps.storage, vlp, &lp_token)?;
    }
    MIGRATION_ACCEPTED.save(deps.storage, &true)?;

    Ok(Response::new()
        .add_attribute("method", "migrate_accept_pool_state")
        .add_attribute("main_factory", main_factory))
}
