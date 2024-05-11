#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_json_binary, Binary, Deps, DepsMut, Env, Isqrt, MessageInfo, Response, StdResult, Uint128};
use cw2::set_contract_version;


use euclid::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::{State, STATE, POOLS};

use euclid::pool::MINIMUM_LIQUIDITY;

use self::query::{query_liquidity, query_simulate_swap};
// version info for migration info
const CONTRACT_NAME: &str = "crates.io:vlp";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    
    let sq_root = Isqrt::isqrt(msg.pool.reserve_1.checked_mul(msg.pool.reserve_2).unwrap());
    let lp_tokens = sq_root.checked_sub(Uint128::new(MINIMUM_LIQUIDITY)).unwrap();

    let state = State {
       pair: msg.pair,
        router: info.sender.to_string(),
        fee: msg.fee,
        last_updated: 0,
        total_reserve_1: msg.pool.reserve_1,
        total_reserve_2: msg.pool.reserve_2,
        total_lp_tokens: lp_tokens
        
    };
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    STATE.save(deps.storage, &state)?;
    // stores initial pool to map
    POOLS.save(deps.storage, &msg.pool.chain,&msg.pool)?;
    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("owner", info.sender)
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
        ExecuteMsg::RegisterPool {pool} => execute::register_pool(deps, info, pool),
        // Test command to be removed, solely for testing.
        ExecuteMsg::AddLiquidity { chain_id, token_1_liquidity, token_2_liquidity, channel, slippage_tolerance } => execute::add_liquidity_chain(deps, env, chain_id, token_1_liquidity, token_2_liquidity, channel, slippage_tolerance),
    }
}

pub mod execute {




    use cosmwasm_std::{Env, IbcMsg, IbcReceiveResponse, IbcTimeout, Uint128};
    use euclid::{pool::Pool, swap, token::Token};
    use euclid_ibc::msg::{AcknowledgementMsg, IbcExecuteMsg, SwapResponse};

    use crate::{ack::make_ack_success, state};

    use super::*;

    /// Registers a new pool in the contract. Function called by Router Contract
    ///
    /// # Arguments
    ///
    /// * `deps` - The mutable dependencies for the contract execution.
    /// * `info` - The message info containing the sender and other information.
    /// * `pool` - The pool to be registered.
    ///
    /// # Errors
    ///
    /// Returns an error if the pool already exists.
    ///
    /// # Returns
    ///
    /// Returns a response with the action and pool chain attributes if successful.
    pub fn register_pool(deps: DepsMut, info: MessageInfo, pool: Pool) -> Result<Response, ContractError> {
        let mut state = STATE.load(deps.storage)?;
        // Verify function is called by Router Contract
        if info.sender != state.router {
            return Err(ContractError::Unauthorized {});
        }
        // Verify that chain pool does not already exist
        if POOLS.may_load(deps.storage, &pool.chain)?.is_some() {
            return Err(ContractError::PoolAlreadyExists {});
        }
        // Store the pool in the map
        POOLS.save(deps.storage, &pool.chain,&pool)?;

        // Calculate LP share allocation
        let lp_allocation = calculate_lp_allocation(pool.reserve_1, pool.reserve_2, state.total_reserve_1, state.total_reserve_2, state.total_lp_tokens);

        // Add pool liquidity to total reserves of VLP
        
        state.total_reserve_1 = state.total_reserve_1.checked_add(pool.reserve_1).unwrap();
        state.total_reserve_2 = state.total_reserve_2.checked_add(pool.reserve_2).unwrap();
        state.total_lp_tokens = state.total_lp_tokens.checked_add(lp_allocation).unwrap();
        STATE.save(deps.storage, &state)?;

        Ok(Response::new().add_attribute("action", "register_pool")
        .add_attribute("pool_chain", pool.chain))
    }   

 
    /// Adds liquidity to the VLP
    /// 
    /// # Arguments
    /// 
    /// * `deps` - The mutable dependencies for the contract execution.
    /// * `chain_id` - The chain id of the pool to add liquidity to.
    /// * `token_1_liquidity` - The amount of token 1 to add to the pool.
    /// * `token_2_liquidity` - The amount of token 2 to add to the pool.
    /// 
    /// # Errors
    /// 
    /// Returns an error if the pool does not exist.
    /// 
    /// # Returns
    /// 
    /// Returns a response with the action and chain id attributes if successful.
    pub fn add_liquidity(deps: DepsMut, chain_id: String, token_1_liquidity: Uint128, token_2_liquidity: Uint128, slippage_tolerance: u64) -> Result<IbcReceiveResponse, ContractError> {
        // Get the pool for the chain_id provided
        let mut pool = POOLS.load(deps.storage, &chain_id)?;
        let mut state = STATE.load(deps.storage)?;
        // Verify that ratio of assets provided is equal to the ratio of assets in the pool
        let ratio = token_1_liquidity.checked_div(token_2_liquidity).unwrap();
        let pool_ratio = pool.reserve_1.checked_div(pool.reserve_2).unwrap();

        // Verify slippage tolerance is between 0 and 100
        if slippage_tolerance > 100 {
            return Err(ContractError::InvalidSlippageTolerance {});
        }
        let lower_ratio = 100 - slippage_tolerance;
        let upper_ratio = 100 + slippage_tolerance;
        // Create an upper and lower bound for pool_ratio and slippage tolerance
        let upper_bound = pool_ratio.multiply_ratio(upper_ratio, 100u128);
        let lower_bound = pool_ratio.multiply_ratio(lower_ratio, 100u128);

        // Verify that the ratio of assets provided is within the slippage tolerance
        if ratio <= lower_bound || ratio >= upper_bound {
            return Err(ContractError::SlippageExceeded {amount: upper_bound, min_amount_out: lower_bound});
        }
        // Add liquidity to the pool
        pool.reserve_1 = pool.reserve_1.checked_add(token_1_liquidity).unwrap();
        pool.reserve_2 = pool.reserve_2.checked_add(token_2_liquidity).unwrap();
        POOLS.save(deps.storage, &chain_id, &pool)?;
        
        // Calculate liquidity added share for LP provider from total liquidity
        let lp_allocation = calculate_lp_allocation(token_1_liquidity, token_2_liquidity, state.total_reserve_1, state.total_reserve_2, state.total_lp_tokens);


        // Add to total liquidity and total lp allocation
        state.total_reserve_1 = state.total_reserve_1.checked_add(token_1_liquidity).unwrap();
        state.total_reserve_2 = state.total_reserve_2.checked_add(token_2_liquidity).unwrap();
        state.total_lp_tokens = state.total_lp_tokens.checked_add(lp_allocation).unwrap();
        STATE.save(deps.storage, &state)?;

        // Add current balance to SNAPSHOT MAP


        Ok(IbcReceiveResponse::new().add_attribute("action", "add_liquidity")
        .add_attribute("chain_id", chain_id)
        .add_attribute("lp_allocation", lp_allocation)
        .add_attribute("liquidity_1_added", token_1_liquidity)
        .add_attribute("liquidity_2_added", token_2_liquidity)
        .set_ack(make_ack_success())
        )
    }



    /// Removes liquidity from the VLP
    /// 
    /// # Arguments
    /// 
    /// * `deps` - The mutable dependencies for the contract execution.
    /// * `chain_id` - The chain id of the pool to remove liquidity from.
    /// * `token_1_liquidity` - The amount of token 1 to remove from the pool.
    /// * `token_2_liquidity` - The amount of token 2 to remove from the pool.
    /// 
    /// # Errors
    /// 
    /// Returns an error if the pool does not exist.
    /// 
    /// # Returns
    /// 
    /// Returns a response with the action and chain id attributes if successful.
    pub fn remove_liquidity(deps: DepsMut, chain_id: String, lp_allocation: Uint128) -> Result<IbcReceiveResponse, ContractError> {
        
        // Get the pool for the chain_id provided
        let mut pool = POOLS.load(deps.storage, &chain_id)?;
        let mut state = STATE.load(deps.storage)?;

        // Fetch allocated liquidity to LP tokens
        let lp_tokens = state.total_lp_tokens;
        let lp_share = lp_allocation.multiply_ratio(Uint128::from(100u128), lp_tokens);

        // Calculate tokens_1 to send
        let token_1_liquidity = pool.reserve_1.multiply_ratio(lp_share, Uint128::from(100u128));
        // Calculate tokens_2 to send
        let token_2_liquidity = pool.reserve_2.multiply_ratio(lp_share, Uint128::from(100u128));


        // Remove liquidity from the pool
        pool.reserve_1 = pool.reserve_1.checked_sub(token_1_liquidity).unwrap();
        pool.reserve_2 = pool.reserve_2.checked_sub(token_2_liquidity).unwrap();
        POOLS.save(deps.storage, &chain_id, &pool)?;
        
        // Remove from total VLP liquidity
        
        state.total_reserve_1 = state.total_reserve_1.checked_sub(token_1_liquidity).unwrap();
        state.total_reserve_2 = state.total_reserve_2.checked_sub(token_2_liquidity).unwrap();
        state.total_lp_tokens = state.total_lp_tokens.checked_sub(lp_allocation).unwrap();
        STATE.save(deps.storage, &state)?;

        
        Ok(IbcReceiveResponse::new().add_attribute("action", "remove_liquidity")
        .add_attribute("chain_id", chain_id)
        .add_attribute("token_1_removed_liquidity", token_1_liquidity)
        .add_attribute("token_2_removed_liquidity", token_2_liquidity)
        .add_attribute("burn_lp", lp_allocation)
        .set_ack(make_ack_success()))
    }

    pub fn execute_swap(deps: DepsMut, chain_id: String, asset: Token, asset_amount: Uint128, min_token_out: Uint128, swap_id: String) -> Result<IbcReceiveResponse, ContractError> {
        
        // Get the pool for the chain_id provided 
        let mut pool = POOLS.load(deps.storage, &chain_id)?;
        let mut state = state::STATE.load(deps.storage)?;
        // Verify that the asset exists for the VLP 
       

        let asset_info = asset.clone().id;
        if asset_info != state.clone().pair.token_1.id && asset_info != state.clone().pair.token_2.id {
            return Err(ContractError::AssetDoesNotExist {});
        }

        // Verify that the asset amount is non-zero
        if asset_amount.is_zero() {
            return Err(ContractError::ZeroAssetAmount {});
        }

        // Get Fee from the state
        let fee = state.clone().fee;
        
        // Calcuate the sum of fees
        let total_fee = fee.lp_fee + fee.staker_fee + fee.treasury_fee;

        // Remove the fee from the asset amount
        let fee_amount = asset_amount.multiply_ratio(Uint128::from(total_fee),Uint128::from(100u128)) ;
        
        // Calculate the amount of asset to be swapped
        let swap_amount = asset_amount.checked_sub(fee_amount).unwrap();

        // verify if asset is token 1 or token 2 
        let swap_info = if asset_info == state.clone().pair.token_1.id {
            (swap_amount, state.clone().total_reserve_1, state.clone().total_reserve_2)
        } else {
            (swap_amount, state.clone().total_reserve_2, state.clone().total_reserve_1)
        };

        let receive_amount = calculate_swap(swap_info.0, swap_info.1, swap_info.2);
        
        // Verify that the receive amount is greater than the minimum token out
        if receive_amount <= min_token_out {
            return Err(ContractError::SlippageExceeded {amount: receive_amount, min_amount_out: min_token_out});
        }

        // Verify that the pool has enough liquidity to swap to user
        // Should activate ELP algorithm to get liquidity from other available pool
        if asset_info == state.clone().pair.token_1.id {
            if pool.reserve_1 < swap_amount {
                return Err(ContractError::SlippageExceeded { amount: swap_amount, min_amount_out: min_token_out});
            }
        } else {
            if pool.reserve_2 < swap_amount {
                return Err(ContractError::SlippageExceeded { amount: swap_amount, min_amount_out: min_token_out });
            }
        }

        // Move liquidity from the pool
        if asset_info == state.clone().pair.token_1.id {
            pool.reserve_1 = pool.reserve_1.checked_add(swap_amount).unwrap();
            pool.reserve_2 = pool.reserve_2.checked_sub(receive_amount).unwrap();
        } else {
            pool.reserve_2 = pool.reserve_2.checked_add(swap_amount).unwrap();
            pool.reserve_1 = pool.reserve_1.checked_sub(receive_amount).unwrap();
        }
        
        // Save the state of the pool
        POOLS.save(deps.storage, &chain_id, &pool)?;

        // Move liquidity for the state
        if asset_info == state.clone().pair.token_1.id {
            state.total_reserve_1 = state.clone().total_reserve_1.checked_add(swap_amount).unwrap();
            state.total_reserve_2 = state.clone().total_reserve_2.checked_sub(receive_amount).unwrap();
        } else {
            state.total_reserve_2 = state.clone().total_reserve_2.checked_add(swap_amount).unwrap();
            state.total_reserve_1 = state.clone().total_reserve_1.checked_sub(receive_amount).unwrap();
        }

        // Get asset to be recieved by user
        let asset_out = if asset_info == state.pair.token_1.id {
            state.clone().pair.token_2
        } else {
            state.clone().pair.token_1
        };

        STATE.save(deps.storage, &state)?;

        // Finalize ack response to swap pool
        let swap_response = SwapResponse {
            asset: asset,
            asset_out: asset_out,
            asset_amount: asset_amount,
            amount_out: receive_amount,
            swap_id: swap_id,
        };

        // Prepare acknowledgement
        let acknowledgement: Binary = to_json_binary(&AcknowledgementMsg::Ok(swap_response))?;

        Ok(IbcReceiveResponse::new().add_attribute("action", "swap")
        .add_attribute("chain_id", chain_id)
        .add_attribute("swap_amount", asset_amount)
        .add_attribute("total_fee", fee_amount)
        .add_attribute("receive_amount", receive_amount)
        .set_ack(acknowledgement))
    }




    // Temporary function to test IBC packet receive using the contracts and if everything works, should come from actual Pool.
    pub fn add_liquidity_chain(_deps: DepsMut, env: Env, chain_id: String, token_1_liquidity: Uint128, token_2_liquidity: Uint128, channel: String, slippage_tolerance: u64) -> Result<Response, ContractError> {
        let msg = IbcMsg::SendPacket { channel_id: channel,
                data: to_json_binary(&IbcExecuteMsg::AddLiquidity { chain_id: chain_id ,
                token_1_liquidity: token_1_liquidity, token_2_liquidity: token_2_liquidity, slippage_tolerance: slippage_tolerance }).unwrap(),
                 timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(120))};
        
        Ok(Response::default().add_attribute("method", "adding_liquidity_chain").add_message(msg))
    }



}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::SimulateSwap { asset, asset_amount } => query_simulate_swap(deps, asset, asset_amount),
        QueryMsg::Liquidity {  } => query_liquidity(deps),
        QueryMsg::LiquidityInfo {  } => query::query_liquidity_info(deps),
    }
}

pub mod query {
    use euclid::token::{Pair, Token};

    use crate::msg::{GetLiquidityResponse, GetSwapResponse, LiquidityInfoResponse};

    use super::*;

    // Function to simulate swap in a query
    pub fn query_simulate_swap(deps: Deps, asset: Token, asset_amount: Uint128) -> Result<Binary, ContractError> {
        
        let state = STATE.load(deps.storage)?;

        // Verify that the asset exists for the VLP 
        let asset_info = asset.id;
        if asset_info != state.pair.token_1.id && asset_info != state.pair.token_2.id {
            return Err(ContractError::AssetDoesNotExist {  });
        }

        // Verify that the asset amount is non-zero
        if asset_amount.is_zero() {
            return Err(ContractError::ZeroAssetAmount {});
        }

        // Get Fee from the state
        let fee = state.fee;
        
        // Calcuate the sum of fees
        let total_fee = fee.lp_fee + fee.staker_fee + fee.treasury_fee;

        // Remove the fee from the asset amount
        let fee_amount = asset_amount.multiply_ratio(Uint128::from(total_fee),Uint128::from(100u128)) ;
        
        // Calculate the amount of asset to be swapped
        let swap_amount = asset_amount.checked_sub(fee_amount).unwrap();

        // verify if asset is token 1 or token 2 
        let swap_info = if asset_info == state.pair.token_1.id {
            (swap_amount, state.total_reserve_1, state.total_reserve_2)
        } else {
            (swap_amount, state.total_reserve_2, state.total_reserve_1)
        };

        let receive_amount = calculate_swap(swap_info.0, swap_info.1, swap_info.2);
        
        // Return the amount of token to be recieved
        Ok(to_json_binary(&GetSwapResponse { token_out: receive_amount }).unwrap())
    }



    // Function to query the total liquidity 
    pub fn query_liquidity(deps: Deps) -> Result<Binary, ContractError> {
        let state = STATE.load(deps.storage)?;
        Ok(to_json_binary(&GetLiquidityResponse {
            token_1_reserve: state.total_reserve_1,
            token_2_reserve: state.total_reserve_2,
        
        }).unwrap())
    }

    // Function to query the total liquidity with pair information
    pub fn query_liquidity_info(deps: Deps) -> Result<Binary, ContractError> {
        let state = STATE.load(deps.storage)?;
        Ok(to_json_binary(&LiquidityInfoResponse {
            pair: state.pair,
            token_1_reserve: state.total_reserve_1,
            token_2_reserve: state.total_reserve_2,
        
        }).unwrap())
    }

}




// Function to calculate the asset to be recieved after a swap
pub fn calculate_swap(swap_amount: Uint128, reserve_in: Uint128, reserve_out: Uint128) -> Uint128 {
    // Calculate the k constant product
    let k = reserve_in.checked_mul(reserve_out).unwrap();
    // Calculate the new reserve of token 1
    let new_reserve_in = reserve_in.checked_add(swap_amount).unwrap();
    // Calculate the new reserve of token 2
    let new_reserve_out = k.checked_div(new_reserve_in).unwrap();
    // Calculate the amount of token 2 to be recieved
    let token_2_recieved = reserve_out.checked_sub(new_reserve_out).unwrap();

    token_2_recieved
}


pub fn calculate_lp_allocation(token_1_amount: Uint128, token_2_amount: Uint128, total_liquidity_1: Uint128, total_liquidity_2: Uint128, total_lp_supply: Uint128) -> Uint128 {
    let share_1 = token_1_amount.checked_div(total_liquidity_1).unwrap();
    let share_2 = token_2_amount.checked_div(total_liquidity_2).unwrap();
    
    // LP allocation is minimum of the two shares multiplied by the total_lp_supply
    let lp_allocation = share_1.min(share_2).checked_mul(total_lp_supply).unwrap();
    lp_allocation
}



#[cfg(test)]
mod tests {

    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{coins, Uint128};
    use euclid::fee::Fee;
    use euclid::pool::Pool;
    use euclid::token::{Pair, PairInfo, Token, TokenInfo};

    #[test]
    // Write a test for instantiation
    fn proper_instantiation() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = mock_info("creator", &coins(1000, "earth"));
        let msg = InstantiateMsg {
            router: "router".to_string(),
            pair: Pair {
                token_1: Token {
                    id: "token_1".to_string(),
                },
                token_2: Token {
                    id: "token_2".to_string(),
                },
            },
            fee: Fee {
                lp_fee: 1,
                treasury_fee: 1,
                staker_fee: 1,
            },
            pool: Pool {
                chain: "chain".to_string(),
                contract_address: "contract_address".to_string(),
                pair: PairInfo {
                    token_1: TokenInfo::Native { denom: "token_1".to_string(),
                 token: Token { id: "token_1".to_string() }},

                    token_2: TokenInfo::Native { denom: "token_2".to_string(),
                    token: Token { id: "token_2".to_string()},
                    },
                },
                reserve_1: Uint128::new(10000),
                reserve_2: Uint128::new(10000),
            },
        };
        let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(0, res.messages.len());
    }

}
