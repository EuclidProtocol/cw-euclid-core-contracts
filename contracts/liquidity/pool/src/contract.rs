#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_json_binary, BankMsg, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response, StdResult};
use cw2::set_contract_version;

use euclid::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::{State, STATE};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:pool";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    

    let state = State {
        vlp_contract: msg.vlp_contract.clone(),
        pair: msg.token_pair.clone(),
        pair_info: msg.pair_info.clone(),
        reserve_1: msg.pool.reserve_1.clone(),
        reserve_2: msg.pool.reserve_2.clone(),
        // Store factory contract
        factory_contract: info.sender.clone().to_string(),
        chain_id: msg.chain_id,
    };

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let mut msgs = Vec::new();
    // Check if tokens are smart contract tokens to create transfer message
    if msg.pair_info.token_1.is_smart() {
        let msg = msg.pair_info.token_1.create_transfer_msg(msg.pool.reserve_1.clone(), env.contract.address.clone().to_string());
        msgs.push(msg);
    }


    if msg.pair_info.token_2.is_smart() {
        let msg = msg.pair_info.token_2.create_transfer_msg(msg.pool.reserve_1.clone(), env.contract.address.clone().to_string());
        msgs.push(msg);
    }
    
    // Validate for deposit of native tokens
    if msg.pair_info.token_1.is_native() {
        // Query the balance of the contract for the native token
        let balance = deps.querier.query_balance(env.contract.address.clone(), msg.pair_info.token_1.get_denom()).unwrap();
        // Verify that the balance is greater than the reserve added
        if balance.amount < msg.pool.reserve_1 {
            return Err(ContractError::InsufficientDeposit {  });
        }
    }

    // Same for token 2
    if msg.pair_info.token_2.is_native() {
        let balance = deps.querier.query_balance(env.contract.address.clone(), msg.pair_info.token_2.get_denom()).unwrap();
        if balance.amount < msg.pool.reserve_2 {
            return Err(ContractError::InsufficientDeposit {  });
        }
    }

    // Save the state
    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("token_1", msg.token_pair.token_1.id)
        .add_attribute("token_2", msg.token_pair.token_2.id)
        .add_attribute("factory_contract", info.sender.clone().to_string())
        .add_attribute("vlp_contract", msg.vlp_contract)
        .add_attribute("chain_id", "chain_id")
        .add_messages(msgs)
    )
}   

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::ExecuteSwap { asset, asset_amount, min_amount_out, channel } => execute::execute_swap_request(deps, info, env, asset, asset_amount, min_amount_out,channel),

    }
}

pub mod execute {
    use cosmwasm_std::{IbcMsg, IbcTimeout, Uint128};
    use euclid::{swap::SwapInfo, token::TokenInfo};
    use euclid_ibc::msg::IbcExecuteMsg;

    use crate::state::{CONNECTION_COUNTS, PENDING_SWAPS};

    use super::*;

    pub fn execute_swap_request(deps: DepsMut, info: MessageInfo, env: Env, asset: TokenInfo, asset_amount: Uint128, min_amount_out: Uint128, channel: String) -> Result<Response, ContractError> {
        
        let state = STATE.load(deps.storage)?;
        
        // Verify that the asset exists in the pool
        if asset != state.pair_info.token_1 && asset != state.pair_info.token_2 {
            return Err(ContractError::AssetDoesNotExist {  });
        }

        // Verify that the asset amount is greater than 0
        if asset_amount.is_zero() {
            return Err(ContractError::ZeroAssetAmount {  });
        }

        // Verify that the min amount out is greater than 0
        if min_amount_out.is_zero() {
            return Err(ContractError::ZeroAssetAmount {  } );
        }

        // Verify that the channel exists
        let count: Option<u32> = CONNECTION_COUNTS.may_load(deps.storage, channel.clone())?;
        if count.is_none() {
            return Err(ContractError::ChannelDoesNotExist {  });
        }
        
        // Verify if the token is native
        if asset.is_native() {
            // Get the denom of native token
            let denom = asset.get_denom();

            // Verify thatthe amount of funds passed is greater than the asset amount
            if info.funds.iter().find(|x| x.denom == denom).unwrap().amount < asset_amount {
                return Err(ContractError::Unauthorized {  });
            }
            
        } else {
            // Verify that the contract address is the same as the asset contract address
            if info.sender != asset.get_contract_address() {
                return Err(ContractError::Unauthorized {  });
            }
        }

        // Get token from tokenInfo
        let token = asset.get_token();

        // Generate a unique identifier for this swap
        let swap_id = format!("{}-{}-{}", info.sender, env.block.height, env.transaction.unwrap().index);

        // Send an IBC packet to VLP to perform swap
        let res = IbcMsg::SendPacket { channel_id: channel.clone(),
             data: to_json_binary(&IbcExecuteMsg::Swap{
                chain_id: state.chain_id.clone(),
                asset: token,
                asset_amount: asset_amount,
                min_amount_out: min_amount_out,
                swap_id: swap_id.clone(),
                channel: channel }).unwrap(),
              timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(60))
            };
        
        // Get alternative token
        let asset_out: TokenInfo = state.pair_info.get_other_token(asset.clone());

        // Add the deposit to Pending Swaps
        let swap_info = SwapInfo {
            asset: asset.clone(),
            asset_out: asset_out.clone(),
            asset_amount: asset_amount,
            timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(60)),
            swap_id: swap_id,

        };

        // Load previous pending swaps for user
        let mut pending_swaps = PENDING_SWAPS.may_load(deps.storage, info.sender.clone().to_string())?.unwrap_or_default();
        
        // Append the new swap to the list 
        pending_swaps.push(swap_info);

        // Save the new list of pending swaps
        PENDING_SWAPS.save(deps.storage, info.sender.clone().to_string(), &pending_swaps)?;
        

        

        Ok(Response::new()
        .add_attribute("method", "execute_swap_request")
        .add_message(res)
    )
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
           }
}

pub mod query {
    use super::*;

}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{coins, from_json};


}
