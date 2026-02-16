use cosmwasm_std::{to_json_binary, Binary, Deps, StdResult, Uint128};
use cw_storage_plus::Bound;

use crate::msg::{
    AssetDepositResponse, QueryMsg, RootResponse, StateResponse, UserDepositResponse,
    WhitelistListResponse, WhitelistResponse,
};
use crate::state::{
    OrderbookDepositsStatus, ASSET_DEPOSITS, CURRENT_ROOT, STATE, USER_DEPOSITS, WHITELISTED_ASSETS,
};

pub fn query(deps: Deps, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::State {} => to_json_binary(&query_state(deps)?),
        QueryMsg::AssetDeposit { token_id } => {
            to_json_binary(&query_asset_deposit(deps, token_id)?)
        }
        QueryMsg::UserDeposit { user, token_id } => {
            to_json_binary(&query_user_deposit(deps, user, token_id)?)
        }
        QueryMsg::Whitelist { token_id } => to_json_binary(&query_whitelist(deps, token_id)?),
        QueryMsg::WhitelistedAssets { start_after, limit } => {
            to_json_binary(&query_whitelisted_assets(deps, start_after, limit)?)
        }
        QueryMsg::CurrentRoot {} => to_json_binary(&query_current_root(deps)?),
    }
}

fn query_state(deps: Deps) -> StdResult<StateResponse> {
    let state = STATE.load(deps.storage)?;
    Ok(StateResponse {
        admin: state.admin.to_string(),
        status: match state.status {
            OrderbookDepositsStatus::Active => "active".to_string(),
            OrderbookDepositsStatus::Paused => "paused".to_string(),
            OrderbookDepositsStatus::Stopped => "stopped".to_string(),
        },
        virtual_balance: state.virtual_balance.to_string(),
    })
}

fn query_asset_deposit(deps: Deps, token_id: String) -> StdResult<AssetDepositResponse> {
    let amount = ASSET_DEPOSITS
        .may_load(deps.storage, token_id.clone())?
        .unwrap_or_else(Uint128::zero);
    Ok(AssetDepositResponse { token_id, amount })
}

fn query_user_deposit(
    deps: Deps,
    user: String,
    token_id: String,
) -> StdResult<UserDepositResponse> {
    let amount = USER_DEPOSITS
        .may_load(deps.storage, (user.clone(), token_id.clone()))?
        .unwrap_or_else(Uint128::zero);

    Ok(UserDepositResponse {
        user: user.to_string(),
        token_id,
        amount,
    })
}

fn query_whitelist(deps: Deps, token_id: String) -> StdResult<WhitelistResponse> {
    let whitelisted = WHITELISTED_ASSETS
        .may_load(deps.storage, token_id.clone())?
        .unwrap_or(false);
    Ok(WhitelistResponse {
        token_id,
        whitelisted,
    })
}

fn query_whitelisted_assets(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<WhitelistListResponse> {
    let limit = limit.unwrap_or(50).min(200) as usize;
    let start = start_after.map(Bound::exclusive);

    let assets: Vec<WhitelistResponse> = WHITELISTED_ASSETS
        .range(deps.storage, start, None, cosmwasm_std::Order::Ascending)
        .filter_map(|item| item.ok())
        .filter(|(_, whitelisted)| *whitelisted)
        .take(limit)
        .map(|(token_id, _)| WhitelistResponse {
            token_id,
            whitelisted: true,
        })
        .collect();

    Ok(WhitelistListResponse { assets })
}

fn query_current_root(deps: Deps) -> StdResult<RootResponse> {
    let root = CURRENT_ROOT.load(deps.storage)?;
    Ok(RootResponse {
        root_id: root.root_id,
        root_hash: root.root_hash,
        per_asset_totals: root.per_asset_totals,
        da_hash: root.da_hash,
        da_url: root.da_url,
        proposed_at: root.proposed_at,
    })
}
