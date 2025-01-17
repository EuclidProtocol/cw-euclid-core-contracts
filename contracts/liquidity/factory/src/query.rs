use anybuf::Anybuf;
use cosmwasm_std::{
    to_binary, to_vec, Addr, Binary, ContractResult, Deps, QuerierWrapper, StdError, StdResult,
    SystemResult, Uint128,
};
use euclid::{
    error::ContractError,
    liquidity::{AddLiquidityRequest, RemoveLiquidityRequest},
    msgs::factory::{
        AllPoolsResponse, AllTokensResponse, GetEscrowResponse, GetLPTokenResponse,
        GetPendingLiquidityResponse, GetPendingRemoveLiquidityResponse, GetPendingSwapsResponse,
        GetVlpResponse, PartnerFeesCollectedPerDenomResponse, PartnerFeesCollectedResponse,
        PoolVlpResponse, StateResponse,
    },
    swap::SwapRequest,
    token::{Pair, Token},
    utils::pagination::{Pagination, DEFAULT_PAGINATION_LIMIT, DEFAULT_PAGINATION_SKIP},
};

use crate::state::{
    HUB_CHANNEL, PAIR_TO_VLP, PENDING_ADD_LIQUIDITY, PENDING_REMOVE_LIQUIDITY, PENDING_SWAPS, PROXY, STATE, TOKEN_TO_ESCROW, VLP_TO_SNIP20
};

// Returns the VLP address
pub fn get_vlp(deps: Deps, pair: Pair) -> Result<Binary, ContractError> {
    let vlp_address = PAIR_TO_VLP.get(deps.storage, &pair.get_tupple()).unwrap();
    Ok(to_binary(&GetVlpResponse { vlp_address })?)
}

// Returns the total partner fees collected
pub fn get_partner_fees_collected(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_binary(&PartnerFeesCollectedResponse {
        total: state.partner_fees_collected,
    })?)
}

pub fn get_partner_fees_collected_per_denom(
    deps: Deps,
    denom: String,
) -> Result<Binary, ContractError> {
    let partner_fees_collected = STATE.load(deps.storage)?.partner_fees_collected;

    Ok(to_binary(&PartnerFeesCollectedPerDenomResponse {
        total: partner_fees_collected.get_fee(denom.as_str()),
    })?)
}

// Returns the LP token address
pub fn get_lp_token_address(deps: Deps, vlp: String) -> Result<Binary, ContractError> {
    let token_address = VLP_TO_SNIP20.get(deps.storage, &vlp).unwrap();
    Ok(to_binary(&GetLPTokenResponse { token_address })?)
}

// Returns the Escrow address alongside allowed denoms if available
pub fn get_escrow(deps: Deps, token_id: String) -> Result<Binary, ContractError> {
    let escrow_info = TOKEN_TO_ESCROW.get(deps.storage, &Token::create(token_id)?);
    let mut response = GetEscrowResponse {
        escrow_address: escrow_info.clone().unwrap().addr,
        escrow_code_hash: escrow_info.clone().unwrap().code_hash,
        denoms: vec![],
    };
    if !escrow_info.clone().unwrap().addr.into_string().is_empty() {
        let denoms: euclid::msgs::escrow::AllowedDenomsResponse = deps.querier.query_wasm_smart(
            escrow_info.clone().unwrap().code_hash,
            escrow_info.unwrap().addr,
            &euclid::msgs::escrow::QueryMsg::AllowedDenoms {},
        )?;
        response.denoms = denoms.denoms;
    }
    Ok(to_binary(&response)?)
}

pub fn query_state(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    let proxy = PROXY.load(deps.storage)?;
    let hub = HUB_CHANNEL.may_load(deps.storage)?;
    Ok(to_binary(&StateResponse {
        chain_uid: state.chain_uid,
        router_contract: state.router_contract,
        admin: state.admin,
        hub_channel: hub,
        escrow_code_id: state.escrow_code_id,
        escrow_code_hash: state.escrow_code_hash,
        snip20_code_id: state.snip20_code_id,
        snip20_code_hash: state.snip20_code_hash,
        is_native: state.is_native,
        partner_fees_collected: state.partner_fees_collected,
        proxy_address : proxy.address,
        proxy_code_hash: proxy.code_hash
    })?)
}

pub fn query_all_pools(deps: Deps) -> Result<Binary, ContractError> {
    let mut pools_res: Vec<PoolVlpResponse> = Vec::new();

    let binding = PAIR_TO_VLP;
    let iter = binding.iter(deps.storage)?;

    for item in iter {
        let ((token_a, token_b), vlp) = item?;

        pools_res.push(PoolVlpResponse {
            pair: Pair::new(token_a, token_b)?,
            vlp,
        });
    }

    // Sort pools_res by token names in ascending order
    pools_res.sort_by(|a, b| {
        a.pair
            .token_1
            .cmp(&b.pair.token_1)
            .then_with(|| a.pair.token_2.cmp(&b.pair.token_2))
    });

    to_binary(&AllPoolsResponse { pools: pools_res }).map_err(Into::into)
}

pub fn query_all_tokens(deps: Deps) -> Result<Binary, ContractError> {
    let mut tokens_res: Vec<Token> = Vec::new();

    let binding = TOKEN_TO_ESCROW;
    let iter = binding.iter(deps.storage)?;

    for item in iter {
        let (token, _) = item?;

        tokens_res.push(token);
    }

    // Sort the tokens in ascending order
    tokens_res.sort_by(|a, b| a.cmp(b));

    to_binary(&AllTokensResponse { tokens: tokens_res }).map_err(Into::into)
}

// Returns the pending swaps for this pair with pagination
pub fn pending_swaps(
    deps: Deps,
    user: Addr,
    pagination: Pagination<Uint128>,
) -> Result<Binary, ContractError> {
    let skip = pagination.skip.unwrap_or(DEFAULT_PAGINATION_SKIP) as usize;
    let limit = pagination.limit.unwrap_or(DEFAULT_PAGINATION_LIMIT) as usize;

    // Initialize a vector to store user-specific swaps
    let mut user_swaps: Vec<(String, SwapRequest)> = Vec::new();

    // Attempt to get an iterator over all entries in PENDING_SWAPS
    let binding = PENDING_SWAPS;
    let iter = binding.iter(deps.storage)?;

    // Manually iterate over all entries
    for item in iter {
        let ((entry_user, tx_id), swap_request) = item?;
        // Check if the entry belongs to the specified user
        if entry_user == user {
            // Filter by min and max bounds if specified
            let meets_min = pagination.min.map_or(true, |min| tx_id >= min.to_string());
            let meets_max = pagination.max.map_or(true, |max| tx_id <= max.to_string());

            // Add entry if it meets all criteria
            if meets_min && meets_max {
                user_swaps.push((tx_id, swap_request));
            }
        }
    }

    // Sort by transaction ID (tx_id) in ascending order
    user_swaps.sort_by(|a, b| a.0.cmp(&b.0));

    // Apply pagination by skipping and taking only the required entries
    let paginated_swaps: Vec<SwapRequest> = user_swaps
        .into_iter()
        .skip(skip)
        .take(limit)
        .map(|(_, swap_request)| swap_request)
        .collect();

    // Convert the response to binary format
    Ok(to_binary(&GetPendingSwapsResponse {
        pending_swaps: paginated_swaps,
    })?)
}

// Returns the pending liquidity transactions for a user with pagination
pub fn pending_liquidity(
    deps: Deps,
    user: Addr,
    pagination: Pagination<Uint128>,
) -> Result<Binary, ContractError> {
    let skip = pagination.skip.unwrap_or(DEFAULT_PAGINATION_SKIP) as usize;
    let limit = pagination.limit.unwrap_or(DEFAULT_PAGINATION_LIMIT) as usize;

    // Initialize a vector to store user-specific liquidity requests
    let mut user_liquidity: Vec<(String, AddLiquidityRequest)> = Vec::new();

    // Manually iterate over all entries in PENDING_ADD_LIQUIDITY
    let binding = PENDING_ADD_LIQUIDITY;
    let iter = binding.iter(deps.storage)?;

    for item in iter {
        let ((entry_user, tx_id), add_liquidity_request) = item?;

        // Check if the entry belongs to the specified user
        if entry_user == user {
            // Filter by min and max bounds if specified
            let tx_id_str = tx_id.clone();
            let meets_min = pagination
                .min
                .map_or(true, |min| tx_id_str >= min.to_string());
            let meets_max = pagination
                .max
                .map_or(true, |max| tx_id_str <= max.to_string());

            // Add entry if it meets all criteria
            if meets_min && meets_max {
                user_liquidity.push((tx_id_str, add_liquidity_request));
            }
        }
    }

    // Sort by transaction ID (tx_id) in ascending order
    user_liquidity.sort_by(|a, b| a.0.cmp(&b.0));

    // Apply pagination by skipping and taking only the required entries
    let paginated_liquidity: Vec<AddLiquidityRequest> = user_liquidity
        .into_iter()
        .skip(skip)
        .take(limit)
        .map(|(_, add_liquidity_request)| add_liquidity_request)
        .collect();

    // Convert the response to binary format
    Ok(to_binary(&GetPendingLiquidityResponse {
        pending_add_liquidity: paginated_liquidity,
    })?)
}

// Returns the pending liquidity transactions for a user with pagination
pub fn pending_remove_liquidity(
    deps: Deps,
    user: Addr,
    pagination: Pagination<Uint128>,
) -> Result<Binary, ContractError> {
    let skip = pagination.skip.unwrap_or(DEFAULT_PAGINATION_SKIP) as usize;
    let limit = pagination.limit.unwrap_or(DEFAULT_PAGINATION_LIMIT) as usize;

    // Initialize a vector to store user-specific remove liquidity requests
    let mut user_remove_liquidity: Vec<(String, RemoveLiquidityRequest)> = Vec::new();

    // Iterate over all entries in PENDING_REMOVE_LIQUIDITY
    let binding = PENDING_REMOVE_LIQUIDITY;
    let iter = binding.iter(deps.storage)?;
    for item in iter {
        let ((entry_user, tx_id), remove_liquidity_request) = item?;

        // Check if the entry belongs to the specified user
        if entry_user == user {
            // Filter by min and max bounds if specified
            let tx_id_str = tx_id.clone();
            let meets_min = pagination
                .min
                .map_or(true, |min| tx_id_str >= min.to_string());
            let meets_max = pagination
                .max
                .map_or(true, |max| tx_id_str <= max.to_string());

            // Add entry if it meets all criteria
            if meets_min && meets_max {
                user_remove_liquidity.push((tx_id_str, remove_liquidity_request));
            }
        }
    }

    // Sort by transaction ID (tx_id) in ascending order
    user_remove_liquidity.sort_by(|a, b| a.0.cmp(&b.0));

    // Apply pagination by skipping and taking only the required entries
    let paginated_remove_liquidity: Vec<RemoveLiquidityRequest> = user_remove_liquidity
        .into_iter()
        .skip(skip)
        .take(limit)
        .map(|(_, remove_liquidity_request)| remove_liquidity_request)
        .collect();

    // Convert the response to binary format
    Ok(to_binary(&GetPendingRemoveLiquidityResponse {
        pending_remove_liquidity: paginated_remove_liquidity,
    })?)
}

pub fn get_contract_code_hash(
    querier: QuerierWrapper,
    contract_address: String,
) -> StdResult<String> {
    let code_hash_query: cosmwasm_std::QueryRequest<cosmwasm_std::Empty> =
        cosmwasm_std::QueryRequest::Stargate {
            path: "/secret.compute.v1beta1.Query/CodeHashByContractAddress".into(),
            data: Binary(Anybuf::new().append_string(1, contract_address).into_vec()),
        };

    let raw = to_vec(&code_hash_query).map_err(|serialize_err| {
        StdError::generic_err(format!("Serializing QueryRequest: {}", serialize_err))
    })?;

    let code_hash = match querier.raw_query(&raw) {
        SystemResult::Err(system_err) => Err(StdError::generic_err(format!(
            "Querier system error: {}",
            system_err
        ))),
        SystemResult::Ok(ContractResult::Err(contract_err)) => Err(StdError::generic_err(format!(
            "Querier contract error: {}",
            contract_err
        ))),
        SystemResult::Ok(ContractResult::Ok(value)) => Ok(value),
    }?;

    // Remove the "\n@" if it exists at the start of the code_hash
    let mut code_hash_str = String::from_utf8(code_hash.to_vec())
        .map_err(|err| StdError::generic_err(format!("Invalid UTF-8 sequence: {}", err)))?;

    if code_hash_str.starts_with("\n@") {
        code_hash_str = code_hash_str.trim_start_matches("\n@").to_string();
    }

    Ok(code_hash_str)
}
