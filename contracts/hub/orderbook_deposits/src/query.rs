use cosmwasm_std::{to_json_binary, Addr, Binary, Deps, StdResult, Uint128};

use crate::msg::{
    AssetDepositResponse, QueryMsg, StateResponse, UserDepositResponse, WhitelistResponse,
};
use crate::state::{
    OrderbookDepositsStatus, ASSET_DEPOSITS, STATE, USER_DEPOSITS, WHITELISTED_ASSETS,
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
    let user_addr: Addr = deps.api.addr_validate(&user)?;
    let amount = USER_DEPOSITS
        .may_load(deps.storage, (user_addr.clone(), token_id.clone()))?
        .unwrap_or_else(Uint128::zero);

    Ok(UserDepositResponse {
        user: user_addr.to_string(),
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
