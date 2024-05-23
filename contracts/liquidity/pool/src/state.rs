use cosmwasm_schema::cw_serde;
use euclid::{
    error::ContractError,
    liquidity::{self, LiquidityTxInfo},
    swap::{self, SwapInfo},
    token::{Pair, PairInfo, TokenInfo},
};

use cosmwasm_std::{DepsMut, IbcTimeout, Uint128};
use cw_storage_plus::{Item, Map};

#[cw_serde]
pub struct State {
    // Store VLP contract address on VLS
    pub vlp_contract: String,
    // Factory Contract
    pub factory_contract: String,
    // Token pair
    pub pair: Pair,
    // Token Pair Info
    pub pair_info: PairInfo,
    // Total cumulative reserves of token_1 in the pool
    // DOES NOT AFFECT SWAP CALCULATIONS
    pub reserve_1: Uint128,
    // Total cumulative reserves of token_2 in the pool
    // DOES NOT AFFECT SWAP CALCULATIONS
    pub reserve_2: Uint128,
    // Store chain Identifier (from factory)
    // The chain IDENTIFIER 'chain_id' does not need to match the chain_id of the chain the contracts are deployed on
    pub chain_id: String,
}

pub const STATE: Item<State> = Item::new("state");

// Map for pending swaps for user
pub const PENDING_SWAPS: Map<(String, u128), SwapInfo> = Map::new("pending_swaps");

// Map for users last pending swap count which will be used for generating ids
pub const PENDING_SWAPS_COUNT: Map<String, u128> = Map::new("pending_swaps_count");

pub fn generate_swap_req(
    deps: DepsMut,
    sender: String,
    asset: TokenInfo,
    asset_out: TokenInfo,
    asset_amount: Uint128,
    timeout: IbcTimeout,
) -> Result<SwapInfo, ContractError> {
    let count = PENDING_SWAPS_COUNT
        .may_load(deps.storage, sender.clone())?
        .unwrap_or_default();

    let rq_id = swap::genarate_id(&sender, count);
    let request = SwapInfo {
        asset,
        asset_out,
        asset_amount,
        timeout,
        swap_id: rq_id,
    };
    // If a pool request already exist, throw error, else create a new request
    PENDING_SWAPS.update(
        deps.storage,
        (sender.clone(), count),
        |existing| match existing {
            Some(req) => Err(ContractError::SwapAlreadyExist { req }),
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
) -> Result<LiquidityTxInfo, ContractError> {
    let count = PENDING_LIQUIDITY_COUNT
        .may_load(deps.storage, sender.clone())?
        .unwrap_or_default();

    let rq_id = liquidity::genarate_id(&sender, count);
    let request = LiquidityTxInfo {
        token_1_liquidity,
        token_2_liquidity,
        sender: sender.clone(),
        liquidity_id: rq_id,
    };
    // If a pool request already exist, throw error, else create a new request
    PENDING_LIQUIDITY.update(
        deps.storage,
        (sender.clone(), count),
        |existing| match existing {
            Some(req) => Err(ContractError::LiquidityTxAlreadyExist { req }),
            None => Ok(request.clone()),
        },
    )?;
    PENDING_LIQUIDITY_COUNT.save(deps.storage, sender, &count.wrapping_add(1))?;
    Ok(request)
}
