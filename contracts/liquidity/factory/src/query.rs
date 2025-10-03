use cosmwasm_std::{to_json_binary, Addr, Binary, Deps, Order, Uint128};
use cw_storage_plus::Bound;
use euclid::{
    chain::{ChainType, IbcChain},
    error::ContractError,
    msgs::factory::{
        AllPoolsResponse, AllTokensResponse, GetEscrowResponse, GetLPTokenResponse,
        GetPendingLiquidityResponse, GetPendingRemoveLiquidityResponse, GetPendingSwapsResponse,
        GetRelayerResponse, GetVlpResponse, PartnerFeesCollectedPerDenomResponse,
        PartnerFeesCollectedResponse, PoolVlpResponse, ReleaseFeeQuery, ReleaseFeesQueryResponse,
        StateResponse, TokenAndChainUid,
    },
    token::{Pair, Token},
    utils::pagination::Pagination,
};

use crate::state::{
    HUB_CHANNEL, MOCK_RELAYER_ADDRESS, PAIR_TO_VLP, PENDING_ADD_LIQUIDITY,
    PENDING_REMOVE_LIQUIDITY, PENDING_SWAPS, RELEASE_FEES, STATE, TOKEN_TO_ESCROW, VLP_TO_CW20,
};

// Returns the VLP address
pub fn get_vlp(deps: Deps, pair: Pair) -> Result<Binary, ContractError> {
    let vlp_address = PAIR_TO_VLP.load(deps.storage, pair.get_tupple())?;
    Ok(to_json_binary(&GetVlpResponse { vlp_address })?)
}

// Returns the total partner fees collected
pub fn get_partner_fees_collected(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&PartnerFeesCollectedResponse {
        total: state.partner_fees_collected,
    })?)
}

pub fn get_partner_fees_collected_per_denom(
    deps: Deps,
    denom: String,
) -> Result<Binary, ContractError> {
    let partner_fees_collected = STATE.load(deps.storage)?.partner_fees_collected;

    Ok(to_json_binary(&PartnerFeesCollectedPerDenomResponse {
        total: partner_fees_collected.get_fee(denom.as_str()),
    })?)
}

// Returns the LP token address
pub fn get_lp_token_address(deps: Deps, vlp: String) -> Result<Binary, ContractError> {
    let token_address = VLP_TO_CW20.load(deps.storage, vlp)?;
    Ok(to_json_binary(&GetLPTokenResponse { token_address })?)
}

// Returns the Escrow address alongside allowed denoms if available
pub fn get_escrow(deps: Deps, token_id: String) -> Result<Binary, ContractError> {
    let escrow_address = TOKEN_TO_ESCROW.may_load(deps.storage, Token::create(token_id)?)?;
    let mut response = GetEscrowResponse {
        escrow_address: escrow_address.clone(),
        denoms: vec![],
    };
    if escrow_address.is_some() {
        let denoms: euclid::msgs::escrow::AllowedDenomsResponse = deps.querier.query_wasm_smart(
            escrow_address.unwrap(),
            &euclid::msgs::escrow::QueryMsg::AllowedDenoms {},
        )?;
        response.denoms = denoms.denoms;
    }
    Ok(to_json_binary(&response)?)
}

pub fn query_state(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    let hub = HUB_CHANNEL.may_load(deps.storage)?;
    Ok(to_json_binary(&StateResponse {
        chain_uid: state.chain_uid,
        router_contract: state.router_contract,
        admin: state.admin,
        hub_channel: hub,
        escrow_code_id: state.escrow_code_id,
        cw20_code_id: state.cw20_code_id,
        is_native: state.is_native,
        partner_fees_collected: state.partner_fees_collected,
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

pub fn get_chain_type(deps: Deps) -> Result<ChainType, ContractError> {
    let state = STATE.load(deps.storage)?;
    if state.is_native {
        Ok(ChainType::Native {})
    } else {
        let channel = HUB_CHANNEL.load(deps.storage)?;
        Ok(ChainType::Ibc(IbcChain {
            from_hub_channel: "not-implemented".to_string(),
            from_factory_channel: channel,
        }))
    }
}

pub fn query_relayer(deps: Deps) -> Result<Binary, ContractError> {
    let relayer = MOCK_RELAYER_ADDRESS.load(deps.storage)?;
    Ok(to_json_binary(&GetRelayerResponse {
        relayer_address: relayer.to_string(),
    })?)
}

pub fn query_release_fees(
    deps: Deps,
    token_and_chain_uid: Option<TokenAndChainUid>,
) -> Result<Binary, ContractError> {
    let release_fee_query = match token_and_chain_uid {
        Some(token_and_chain_uid) => {
            let release_fee = RELEASE_FEES.load(deps.storage, token_and_chain_uid.key())?;
            ReleaseFeesQueryResponse {
                fees: vec![ReleaseFeeQuery {
                    token_and_chain_uid: token_and_chain_uid.key(),
                    fee: release_fee,
                }],
            }
        }
        // If token_and_chain_uid is not provided, return all release fees
        None => {
            let release_fees = RELEASE_FEES
                .range(deps.storage, None, None, Order::Ascending)
                .map(|v| {
                    let v = v?;
                    Ok(ReleaseFeeQuery {
                        token_and_chain_uid: v.0,
                        fee: v.1,
                    })
                })
                .collect::<Result<_, ContractError>>()?;
            ReleaseFeesQueryResponse { fees: release_fees }
        }
    };
    Ok(to_json_binary(&release_fee_query)?)
}
