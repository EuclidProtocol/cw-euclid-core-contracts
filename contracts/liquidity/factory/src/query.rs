use cosmwasm_std::{to_json_binary, Addr, Binary, Deps, Env, Order, Uint128};
use cw_storage_plus::Bound;
use euclid::{
    chain::{ChainType, CosmosChain},
    error::ContractError,
    msgs::factory::{
        AllPoolsResponse, AllTokensResponse, GetEscrowResponse, GetLPTokenResponse,
        GetPendingLiquidityResponse, GetPendingRemoveLiquidityResponse, GetPendingSwapsResponse,
        GetVlpResponse, PartnerFeesCollectedPerDenomResponse, PartnerFeesCollectedResponse,
        PoolVlpResponse, StateResponse,
    },
    token::{Pair, Token},
    utils::pagination::Pagination,
};

use crate::state::{
    FEE_STATE, PAIR_TO_VLP, PENDING_ADD_LIQUIDITY, PENDING_REMOVE_LIQUIDITY, PENDING_SWAPS, STATE,
    TOKEN_TO_ESCROW, VLP_TO_LP_TOKEN,
};

// Returns the VLP address
pub fn get_vlp(deps: Deps, pair: Pair) -> Result<Binary, ContractError> {
    let vlp_address = PAIR_TO_VLP.load(deps.storage, pair.get_tupple())?;
    Ok(to_json_binary(&GetVlpResponse { vlp_address })?)
}

// Returns the total partner fees collected
pub fn get_partner_fees_collected(deps: Deps) -> Result<Binary, ContractError> {
    let fee_state = FEE_STATE.load(deps.storage)?;
    Ok(to_json_binary(&PartnerFeesCollectedResponse {
        total: fee_state.partner_fees_collected,
    })?)
}

pub fn get_partner_fees_collected_per_denom(
    deps: Deps,
    denom: String,
) -> Result<Binary, ContractError> {
    let partner_fees_collected = FEE_STATE.load(deps.storage)?.partner_fees_collected;

    Ok(to_json_binary(&PartnerFeesCollectedPerDenomResponse {
        total: partner_fees_collected.get_fee(denom.as_str()),
    })?)
}

// Returns the LP token address
pub fn get_lp_token_address(deps: Deps, vlp: String) -> Result<Binary, ContractError> {
    let token_address = VLP_TO_LP_TOKEN.load(deps.storage, vlp)?;
    Ok(to_json_binary(&GetLPTokenResponse { token_address })?)
}

// Returns the Escrow address alongside allowed denoms if available
pub fn get_escrow(deps: Deps, token_id: String) -> Result<Binary, ContractError> {
    let escrow_address = TOKEN_TO_ESCROW.may_load(deps.storage, Token::create(token_id)?)?;
    let mut response = GetEscrowResponse {
        escrow_address: escrow_address.clone(),
        denoms: vec![],
    };
    if let Some(escrow_address) = escrow_address {
        let denoms: euclid::msgs::escrow::AllowedDenomsResponse = deps.querier.query_wasm_smart(
            escrow_address,
            &euclid::msgs::escrow::QueryMsg::AllowedDenoms {},
        )?;
        response.denoms = denoms.denoms;
    }
    Ok(to_json_binary(&response)?)
}

pub fn query_state(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&StateResponse {
        chain_uid: state.chain_uid,
        router_contract: state.router_contract,
        relayer_contract: state.relayer_contract,
        admin: state.admin,
        escrow_code_id: state.escrow_code_id,
        lp_code_id: state.lp_code_id,
        is_native: state.is_native,
    })?)
}
pub fn query_all_pools(deps: Deps) -> Result<Binary, ContractError> {
    let pools: Vec<PoolVlpResponse> = PAIR_TO_VLP
        .range(deps.storage, None, None, Order::Ascending)
        .map(|item| {
            let (pair_tokens, vlp) = item?;
            Ok(PoolVlpResponse {
                pair: Pair::new(Token::create(pair_tokens.0)?, Token::create(pair_tokens.1)?)?,
                vlp,
            })
        })
        .collect::<Result<_, ContractError>>()?;

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
    pagination: Pagination<Uint128>,
) -> Result<Binary, ContractError> {
    let min = pagination.min.map(Bound::inclusive);
    let max = pagination.max.map(Bound::inclusive);

    // Fetch pending swaps for user
    let pending_swaps = PENDING_SWAPS
        .prefix(user)
        .range(deps.storage, min, max, Order::Ascending)
        .skip(pagination.skip.unwrap_or(0) as usize)
        .take(pagination.limit.unwrap_or(10) as usize)
        .map(|k| k.unwrap().1)
        .collect();

    Ok(to_json_binary(&GetPendingSwapsResponse { pending_swaps })?)
}

// Returns the pending liquidity transactions for a user with pagination
pub fn pending_liquidity(
    deps: Deps,
    user: Addr,
    pagination: Pagination<Uint128>,
) -> Result<Binary, ContractError> {
    let min = pagination.min.map(Bound::inclusive);
    let max = pagination.max.map(Bound::inclusive);

    let pending_add_liquidity = PENDING_ADD_LIQUIDITY
        .prefix(user)
        .range(deps.storage, min, max, Order::Ascending)
        .skip(pagination.skip.unwrap_or(0) as usize)
        .take(pagination.limit.unwrap_or(10) as usize)
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
    pagination: Pagination<Uint128>,
) -> Result<Binary, ContractError> {
    let min = pagination.min.map(Bound::inclusive);
    let max = pagination.max.map(Bound::inclusive);

    let pending_remove_liquidity = PENDING_REMOVE_LIQUIDITY
        .prefix(user)
        .range(deps.storage, min, max, Order::Ascending)
        .skip(pagination.skip.unwrap_or(0) as usize)
        .take(pagination.limit.unwrap_or(10) as usize)
        .flat_map(|k| -> Result<_, ContractError> { Ok(k?.1) })
        .collect();

    Ok(to_json_binary(&GetPendingRemoveLiquidityResponse {
        pending_remove_liquidity,
    })?)
}

pub fn get_chain_type(deps: Deps, env: &Env) -> Result<ChainType, ContractError> {
    let state = STATE.load(deps.storage)?;
    if state.is_native {
        Ok(ChainType::Native {})
    } else {
        Ok(ChainType::Cosmos(CosmosChain {
            chain_id: env.block.chain_id.clone(),
        }))
    }
}
