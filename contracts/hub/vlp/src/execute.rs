use crate::state::{BALANCES, CHAIN_LP_TOKENS, STATE};
use cosmwasm_std::{ensure, DepsMut, Env, MessageInfo, Response, Uint128};
use euclid::{
    chain::CrossChainUser, error::ContractError, pool::add_liquidity, token::PairWithAmount,
};

pub fn register_pool_with_funds(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    sender: CrossChainUser,
    pair_with_amount: PairWithAmount,
    slippage_tolerance_bps: u64,
    tx_id: String,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.router, ContractError::Unauthorized {});
    // Verify that chain pool does not already exist
    ensure!(
        !CHAIN_LP_TOKENS.has(deps.storage, sender.chain_uid.clone()),
        ContractError::PoolAlreadyExists {}
    );
    // Check for token id
    ensure!(
        state.pair.get_tupple() == pair_with_amount.get_pair()?.get_tupple(),
        ContractError::AssetDoesNotExist {}
    );
    // Store the pool in the map
    CHAIN_LP_TOKENS.save(deps.storage, sender.chain_uid.clone(), &Uint128::zero())?;

    // Add liquidity part //
    add_liquidity(
        deps.branch(),
        env,
        info,
        &STATE,
        &BALANCES,
        &CHAIN_LP_TOKENS,
        sender,
        pair_with_amount,
        slippage_tolerance_bps,
        tx_id,
    )
}
