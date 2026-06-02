use cosmwasm_std::{ensure, Addr, DepsMut, Env, MessageInfo, Response};
use euclid::{error::ContractError, msgs::vlp::base::PoolKey, token::Pair};

use crate::state::{
    CONCENTRATED_VLPS, MAIN_FACTORY_ADDRESS, MIGRATION_ACCEPTED, PAIR_TO_VLP,
    POSITION_TOKEN_CONTRACT, VLP_TO_LP_TOKEN,
};

/// One-shot migration accept. Only callable by main factory, only once.
/// In Slice 1 the payload was intentionally narrow (just CP/Stable pool maps);
/// Slice 4 extends it with the concentrated registry and the singleton
/// position-token contract address. Later slices extend further with the
/// pending-queue items.
pub fn migrate_accept_pool_state(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    pair_to_vlp: Vec<(Pair, String)>,
    vlp_to_lp_token: Vec<(String, Addr)>,
    concentrated_vlps: Option<Vec<(PoolKey, String)>>,
    position_token_contract: Option<Addr>,
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
    if let Some(entries) = concentrated_vlps {
        for (pool_key, vlp) in entries {
            CONCENTRATED_VLPS.save(deps.storage, pool_key.to_map_key(), &vlp)?;
        }
    }
    if let Some(position_token) = position_token_contract {
        POSITION_TOKEN_CONTRACT.save(deps.storage, &position_token)?;
    }
    MIGRATION_ACCEPTED.save(deps.storage, &true)?;

    Ok(Response::new()
        .add_attribute("method", "migrate_accept_pool_state")
        .add_attribute("main_factory", main_factory))
}
