use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, DepsMut, IbcTimeout, Uint128};
use cw_storage_plus::{Item, Map};
use euclid::{
    error::ContractError,
    liquidity::{self, LiquidityTxInfo},
    pool::{self, Pool, PoolRequest},
    swap::{self, NextSwap, SwapInfo},
    token::{PairInfo, Token, TokenInfo},
};

#[cw_serde]
pub struct State {
    // The Unique Chain Identifier
    // THIS IS DIFFERENT THAN THE CHAIN_ID OF THE CHAIN, THIS REPRESENTS A UNIQUE IDENTIFIER FOR THE CHAIN
    // IN THE EUCLID ECOSYSTEM
    pub chain_id: String,
    // The Router Contract Address on the Virtual Settlement Layer
    pub router_contract: String,
    // Channel that connects factory to hub chain. This is set after factory registration call from router
    pub hub_channel: Option<String>,
    // Contract admin
    pub admin: String,
    // Escrow Code ID
    pub escrow_code_id: u64,
}

pub const STATE: Item<State> = Item::new("state");

// Map VLP address to Pool address
pub const VLP_TO_POOL: Map<String, PairInfo> = Map::new("vlp_to_pool");

// New Factory states
pub const TOKEN_TO_ESCROW: Map<Token, Addr> = Map::new("token_to_escrow");

// Map sender of Pool request to Pool address
pub const POOL_REQUESTS: Map<String, PoolRequest> = Map::new("request_to_pool");

// Pool Requests Counter
pub const POOL_REQUEST_COUNT: Map<String, u128> = Map::new("request_to_pool_count");

pub fn generate_pool_req(
    deps: DepsMut,
    sender: &Addr,
    chain: String,
    channel: String,
    pair_info: PairInfo,
) -> Result<PoolRequest, ContractError> {
    let count = POOL_REQUEST_COUNT
        .may_load(deps.storage, sender.to_string())?
        .unwrap_or_default();

    let pool_rq_id = pool::generate_id(sender.as_str(), count);
    let pool_request = PoolRequest {
        chain,
        channel,
        pool_rq_id: pool_rq_id.clone(),
        pair_info,
    };
    // If a pool request already exist, throw error, else create a new request
    POOL_REQUESTS.update(deps.storage, pool_rq_id, |existing| match existing {
        Some(_req) => Err(ContractError::PoolRequestAlreadyExists {}),
        None => Ok(pool_request.clone()),
    })?;
    POOL_REQUEST_COUNT.save(deps.storage, sender.to_string(), &count.wrapping_add(1))?;
    Ok(pool_request)
}

// Map for pending swaps for user
pub const PENDING_SWAPS: Map<(String, u128), SwapInfo> = Map::new("pending_swaps");

// Map for users last pending swap count which will be used for generating ids
pub const PENDING_SWAPS_COUNT: Map<String, u128> = Map::new("pending_swaps_count");

pub fn generate_swap_req(
    deps: DepsMut,
    sender: String,
    asset_in: TokenInfo,
    asset_out: TokenInfo,
    amount_in: Uint128,
    min_amount_out: Uint128,
    swaps: Vec<NextSwap>,
    timeout: IbcTimeout,
) -> Result<SwapInfo, ContractError> {
    let count = PENDING_SWAPS_COUNT
        .may_load(deps.storage, sender.clone())?
        .unwrap_or_default();

    let rq_id = swap::generate_id(&sender, count);
    let request = SwapInfo {
        asset_in,
        asset_out,
        amount_in,
        min_amount_out,
        swaps,
        timeout,
        swap_id: rq_id,
    };
    // If a pool request already exist, throw error, else create a new request
    PENDING_SWAPS.update(
        deps.storage,
        (sender.clone(), count),
        |existing| match existing {
            Some(_req) => Err(ContractError::SwapAlreadyExist {}),
            None => Ok(request.clone()),
        },
    )?;
    PENDING_SWAPS_COUNT.save(deps.storage, sender, &count.wrapping_add(1))?;
    Ok(request)
}

// Map for PENDING liquidity transactions
pub const PENDING_LIQUIDITY: Map<(String, u128), LiquidityTxInfo> = Map::new("pending_liquidity");

// Map for users last pending liquidity count which will be used for generating ids
pub const PENDING_LIQUIDITY_COUNT: Map<String, u128> = Map::new("pending_liquidity_count");

pub fn generate_liquidity_req(
    deps: DepsMut,
    sender: String,
    token_1_liquidity: Uint128,
    token_2_liquidity: Uint128,
    vlp_address: String,
    pair_info: PairInfo,
) -> Result<LiquidityTxInfo, ContractError> {
    let count = PENDING_LIQUIDITY_COUNT
        .may_load(deps.storage, sender.clone())?
        .unwrap_or_default();

    let rq_id = liquidity::generate_id(&sender, count);
    let request = LiquidityTxInfo {
        token_1_liquidity,
        token_2_liquidity,
        sender: sender.clone(),
        liquidity_id: rq_id,
        vlp_address,
        pair_info,
    };
    // If a pool request already exist, throw error, else create a new request
    PENDING_LIQUIDITY.update(
        deps.storage,
        (sender.clone(), count),
        |existing| match existing {
            Some(_req) => Err(ContractError::LiquidityTxAlreadyExist {}),
            None => Ok(request.clone()),
        },
    )?;
    PENDING_LIQUIDITY_COUNT.save(deps.storage, sender, &count.wrapping_add(1))?;
    Ok(request)
}
