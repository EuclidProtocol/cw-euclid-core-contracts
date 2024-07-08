use cosmwasm_std::{to_json_binary, Addr, Binary, Deps, Order};
use euclid::{
    error::ContractError,
    msgs::factory::{
        AllPoolsResponse, AllTokensResponse, GetEscrowResponse, GetPendingLiquidityResponse,
        GetPendingRemoveLiquidityResponse, GetPendingSwapsResponse, GetVlpResponse,
        PoolVlpResponse, StateResponse,
    },
    token::{Pair, Token},
};

use crate::state::{
    HUB_CHANNEL, PAIR_TO_VLP, PENDING_ADD_LIQUIDITY, PENDING_REMOVE_LIQUIDITY, PENDING_SWAPS,
    STATE, TOKEN_TO_ESCROW,
};

// Returns the Pair Info of the Pair in the pool
pub fn get_vlp(deps: Deps, pair: Pair) -> Result<Binary, ContractError> {
    let vlp_address = PAIR_TO_VLP.load(deps.storage, pair.get_tupple())?;
    Ok(to_json_binary(&GetVlpResponse { vlp_address })?)
}

// Returns the Pair Info of the Pair in the pool
pub fn get_escrow(deps: Deps, token_id: String) -> Result<Binary, ContractError> {
    let escrow_address = TOKEN_TO_ESCROW.may_load(deps.storage, Token::new(token_id)?)?;
    Ok(to_json_binary(&GetEscrowResponse { escrow_address })?)
}

pub fn query_state(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    let hub = HUB_CHANNEL.may_load(deps.storage)?;
    Ok(to_json_binary(&StateResponse {
        chain_uid: state.chain_uid,
        router_contract: state.router_contract,
        admin: state.admin,
        hub_channel: hub,
    })?)
}
pub fn query_all_pools(deps: Deps) -> Result<Binary, ContractError> {
    let pools = PAIR_TO_VLP
        .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .flat_map(|item| -> Result<_, ContractError> {
            let item = item.unwrap();
            Ok(PoolVlpResponse {
                pair: Pair::new(item.0 .0, item.0 .1)?,
                vlp: item.1,
            })
        })
        .collect();

    to_json_binary(&AllPoolsResponse { pools }).map_err(Into::into)
}

pub fn query_all_tokens(deps: Deps) -> Result<Binary, ContractError> {
    let tokens = TOKEN_TO_ESCROW
        .keys(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .flatten()
        .collect();

    to_json_binary(&AllTokensResponse { tokens }).map_err(Into::into)
}

// Returns the pending swaps for this pair with pagination
pub fn pending_swaps(
    deps: Deps,
    user: Addr,
    _lower_limit: Option<u128>,
    _upper_limit: Option<u128>,
) -> Result<Binary, ContractError> {
    // Fetch pending swaps for user
    let pending_swaps = PENDING_SWAPS
        .prefix(user)
        .range(deps.storage, None, None, Order::Ascending)
        .map(|k| k.unwrap().1)
        .collect();

    Ok(to_json_binary(&GetPendingSwapsResponse { pending_swaps })?)
}

// Returns the pending liquidity transactions for a user with pagination
pub fn pending_liquidity(
    deps: Deps,
    user: Addr,
    _lower_limit: Option<u128>,
    _upper_limit: Option<u128>,
) -> Result<Binary, ContractError> {
    let pending_add_liquidity = PENDING_ADD_LIQUIDITY
        .prefix(user)
        .range(deps.storage, None, None, Order::Ascending)
        .flat_map(|k| -> Result<_, ContractError> { Ok(k?.1) })
        .collect();

    Ok(to_json_binary(&GetPendingLiquidityResponse {
        pending_add_liquidity,
    })?)
}

// Returns the pending liquidity transactions for a user with pagination
pub fn pending_remove_liquidity(
    deps: Deps,
    user: Addr,
    _lower_limit: Option<u128>,
    _upper_limit: Option<u128>,
) -> Result<Binary, ContractError> {
    let pending_remove_liquidity = PENDING_REMOVE_LIQUIDITY
        .prefix(user)
        .range(deps.storage, None, None, Order::Ascending)
        .flat_map(|k| -> Result<_, ContractError> { Ok(k?.1) })
        .collect();

    Ok(to_json_binary(&GetPendingRemoveLiquidityResponse {
        pending_remove_liquidity,
    })?)
}
