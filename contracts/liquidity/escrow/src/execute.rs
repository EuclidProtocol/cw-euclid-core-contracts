use std::collections::HashSet;

use cosmwasm_std::{
    ensure, from_json, to_json_binary, CosmosMsg, DepsMut, Env, IbcBasicResponse, IbcMsg,
    IbcTimeout, MessageInfo, Response, Uint128,
};
use cw20::Cw20ReceiveMsg;
use euclid::{
    error::ContractError,
    liquidity,
    msgs::pool::Cw20HookMsg,
    pool::LiquidityResponse,
    swap::{self, SwapResponse},
    timeout::get_timeout,
    token::{Token, TokenInfo},
};
use euclid_ibc::msg::ChainIbcExecuteMsg;

use crate::state::{ALLOWED_DENOMS, FACTORY_ADDRESS};

fn check_duplicates(denoms: Vec<String>) -> Result<(), ContractError> {
    let mut seen = HashSet::new();
    for denom in denoms {
        if seen.contains(&denom) {
            return Err(ContractError::DuplicateDenominations {});
        }
        seen.insert(denom);
    }
    Ok(())
}

// Function to add a new list of allowed denoms, this overwrites the previous list
pub fn execute_update_allowed_denoms(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    denoms: Vec<String>,
) -> Result<Response, ContractError> {
    // Only the factory can call this function
    let factory_address = FACTORY_ADDRESS.load(deps.storage)?;
    ensure!(
        info.sender == factory_address,
        ContractError::Unauthorized {}
    );

    let allowed_denoms = ALLOWED_DENOMS.load(deps.storage)?;
    // Check for duplicate denoms
    check_duplicates(denoms)?;
    ALLOWED_DENOMS.save(deps.storage, &denoms)?;

    Ok(Response::new().add_attribute("method", "update_allowed_denoms"))
}
