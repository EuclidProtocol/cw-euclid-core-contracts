#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_json_binary, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response, StdResult};
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
        ExecuteMsg::ExecuteSwap { asset, asset_amount, min_amount_out, channel } => execute::execute_swap_request(deps, info, env, asset, asset_amount, min_amount_out,channel, None),
        ExecuteMsg::AddLiquidity { token_1_liquidity, token_2_liquidity, slippage_tolerance, channel } => execute::add_liquidity_request(deps, info, env, token_1_liquidity, token_2_liquidity, slippage_tolerance, channel, None),
        ExecuteMsg:: Receive(msg) => execute::receive_cw20(deps, env, info, msg),
    }
}

pub mod execute {
    use cosmwasm_std::{ensure, from_json, Coin, IbcMsg, IbcTimeout, Uint128};
    use cw20::Cw20ReceiveMsg;
    use euclid::{swap::{LiquidityTxInfo, SwapInfo}, token::TokenInfo};
    use euclid_ibc::msg::IbcExecuteMsg;

    use crate::{msg::Cw20HookMsg, state::{CONNECTION_COUNTS, PENDING_LIQUIDITY, PENDING_SWAPS}};

    use super::*;

    pub fn execute_swap_request(deps: DepsMut, info: MessageInfo, env: Env, asset: TokenInfo, asset_amount: Uint128, min_amount_out: Uint128, channel: String, msg_sender: Option<String>) -> Result<Response, ContractError> {
        
        let state = STATE.load(deps.storage)?;
        
        // if `msg_sender` is not None, then the sender is the one who initiated the swap
        let sender = match msg_sender {
            Some(sender) => sender,
            None => info.sender.clone().to_string(),
        };

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
            if info.sender.clone().to_string() != asset.get_contract_address() {
                return Err(ContractError::Unauthorized {  });
            }
        }

        // Get token from tokenInfo
        let token = asset.get_token();

        // Generate a unique identifier for this swap
        let swap_id = format!("{}-{}-{}", sender, env.block.height, env.transaction.unwrap().index);

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
        let mut pending_swaps = PENDING_SWAPS.may_load(deps.storage, sender.to_string())?.unwrap_or_default();
        
        // Append the new swap to the list 
        pending_swaps.push(swap_info);

        // Save the new list of pending swaps
        PENDING_SWAPS.save(deps.storage, sender.clone().to_string(), &pending_swaps)?;
        

        

        Ok(Response::new()
        .add_attribute("method", "execute_swap_request")
        .add_message(res)
    )
    }


    /// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
///
/// * **cw20_msg** is the CW20 message that has to be processed.
pub fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    match from_json(&cw20_msg.msg)? {
        // Allow to swap using a CW20 hook message
        Cw20HookMsg::Swap { asset, min_amount_out, channel } => {
            let contract_adr = info.sender.clone();

            // ensure that contract address is same as asset being swapped
            ensure!(
                contract_adr == asset.get_contract_address(),
                ContractError::AssetDoesNotExist {  }
            );
            // Add sender as the option

            // ensure that the contract address is the same as the asset contract address
            execute_swap_request(deps, info, env, asset, cw20_msg.amount, min_amount_out, channel, Some(cw20_msg.sender))
        },

        
    }
}


// Add liquidity to the pool
pub fn add_liquidity_request(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    token_1_liquidity: Uint128,
    token_2_liquidity: Uint128,
    slippage_tolerance: u64,
    channel: String,
    msg_sender: Option<String>,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;

    // Check that slippage tolerance is between 1 and 100
    if slippage_tolerance < 1 || slippage_tolerance > 100 {
        return Err(ContractError::InvalidSlippageTolerance {  });
    }

    // if `msg_sender` is not None, then the sender is the one who initiated the swap
    let sender = match msg_sender {
        Some(sender) => sender,
        None => info.sender.clone().to_string(),
    };

    // Check that the channel exists
    let count: Option<u32> = CONNECTION_COUNTS.may_load(deps.storage, channel.clone())?;
    if count.is_none() {
        return Err(ContractError::ChannelDoesNotExist {  });
    }

    // Check that the token_1 liquidity is greater than 0
    if token_1_liquidity.is_zero() || token_2_liquidity.is_zero() {
        return Err(ContractError::ZeroAssetAmount {  });
    }

    // Get the token 1 and token 2 from the pair info
    let token_1 = state.pair_info.token_1.clone();
    let token_2 = state.pair_info.token_2.clone();

    // Prepare msg vector
    let mut msgs: Vec<CosmosMsg> = Vec::new();

    // IF TOKEN IS A SMART CONTRACT IT REQUIRES APPROVAL FOR TRANSFER 
    if token_1.is_smart() {
        let msg = token_1.create_transfer_msg(token_1_liquidity, env.contract.address.clone().to_string());
        msgs.push(msg);
    } else {
        // If funds empty return error
        if info.funds.is_empty() {
            return Err(ContractError::InsufficientDeposit {  });
        }
        // Check for funds sent with the message
        let amt = info.funds.iter().find(|x| x.denom == token_1.get_denom()).unwrap();
        if amt.amount < token_1_liquidity {
            return Err(ContractError::InsufficientDeposit {  });
        }
    }

    // Same for token 2
    if token_2.is_smart() {
        let msg = token_2.create_transfer_msg(token_2_liquidity, env.contract.address.clone().to_string());
        msgs.push(msg);
    } else {
        if info.funds.is_empty() {
            return Err(ContractError::InsufficientDeposit {  });
        }
        let amt = info.funds.iter().find(|x| x.denom == token_2.get_denom()).unwrap();
        if amt.amount < token_2_liquidity.clone() {
            return Err(ContractError::InsufficientDeposit {  });
        }
    }

    // Save the pending liquidity transaction
    let liquidity_id = format!("{}-{}-{}", info.sender, env.block.height, env.transaction.unwrap().index);
    // Create new Liquidity Info
    let liquidity_info: LiquidityTxInfo = LiquidityTxInfo {
        sender: sender.clone(),
        token_1_liquidity: token_1_liquidity,
        token_2_liquidity: token_2_liquidity,
        liquidity_id: liquidity_id.clone()
    };

    // Store Liquidity Info
    let mut pending_liquidity = PENDING_LIQUIDITY.may_load(deps.storage, sender.clone())?.unwrap_or_default();
    pending_liquidity.push(liquidity_info);
    PENDING_LIQUIDITY.save(deps.storage, sender.clone(), &pending_liquidity)?;

    // Prepare IBC Packet to send to VLP 
    let ibc_packet = IbcMsg::SendPacket {
        channel_id: channel.clone(),
        data: to_json_binary(&IbcExecuteMsg::AddLiquidity {
            chain_id: state.chain_id.clone(),
            token_1_liquidity: token_1_liquidity,
            token_2_liquidity: token_2_liquidity,
            slippage_tolerance: slippage_tolerance,
            liquidity_id: liquidity_id.clone(),
        }).unwrap(),
        timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(60))
    };

    msgs.push(ibc_packet.into());


    Ok(Response::new()
    .add_attribute("method", "add_liquidity_request")
    .add_attribute("token_1_liquidity", token_1_liquidity)
    .add_attribute("token_2_liquidity", token_2_liquidity)
    .add_messages(msgs))

}


}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::PairInfo {} => query::pair_info(deps),
        QueryMsg::PendingSwapsUser { user ,upper_limit, lower_limit } => query::pending_swaps(deps, user, lower_limit, upper_limit, ),
           }
}

pub mod query {

    use crate::{msg::{GetPairInfoResponse, GetPendingSwapsResponse}, state::PENDING_SWAPS};

    use super::*;

    // Returns the Pair Info of the Pair in the pool
    pub fn pair_info(deps: Deps) -> StdResult<Binary> {
        let state = STATE.load(deps.storage)?;
        to_json_binary(&GetPairInfoResponse { pair_info: state.pair_info })
    }

    // Returns the pending swaps for this pair with pagination
    pub fn pending_swaps(deps: Deps, user: String, lower_limit: u32, upper_limit: u32) -> StdResult<Binary> {
        // Fetch pending swaps for user
        let pending_swaps = PENDING_SWAPS.may_load(deps.storage, user.clone())?.unwrap_or_default();
        // Get the upper limit
        let upper_limit = upper_limit as usize;
        // Get the lower limit
        let lower_limit = lower_limit as usize;
        // Get the pending swaps within the range
        let pending_swaps = pending_swaps[lower_limit..upper_limit].to_vec();
        // Return the response
        to_json_binary(&GetPendingSwapsResponse { pending_swaps })
    }


    
}

#[cfg(test)]
mod tests {



}
