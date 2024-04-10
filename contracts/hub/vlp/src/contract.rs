#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult};
use cw2::set_contract_version;

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, GetCountResponse, InstantiateMsg, QueryMsg};
use crate::state::{State, STATE, POOLS};

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
    
    let state = State {
       pair: msg.pair,
        router: info.sender.to_string(),
        fee: msg.fee,
        last_updated: 0,
        total_reserve_1: msg.pool.reserve_1,
        total_reserve_2: msg.pool.reserve_2,
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
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::RegisterPool {pool} => execute::register_pool(deps, info, pool),
        
    }
}

pub mod execute {


    use cosmwasm_std::{CosmosMsg, Env, IbcMsg, IbcReceiveResponse, IbcTimeout, Uint128};
    use euclid::{pool::Pool, token};

    use crate::{ack::make_ack_success, msg::IbcExecuteMsg};

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

        // [TODO] Verify function is called by Router Contract

        // Verify that chain pool does not already exist
        if POOLS.may_load(deps.storage, &pool.chain)?.is_some() {
            return Err(ContractError::PoolAlreadyExists {});
        }
        // Store the pool in the map
        POOLS.save(deps.storage, &pool.chain,&pool)?;

        // Add pool liquidity to total reserves of VLP
        let mut state = STATE.load(deps.storage)?;
        state.total_reserve_1.checked_add(pool.reserve_1);
        state.total_reserve_2.checked_add(pool.reserve_2);
        STATE.save(deps.storage, &state)?;

        Ok(Response::new().add_attribute("action", "register_pool")
        .add_attribute("pool_chain", pool.chain))
    }

    pub fn add_liquidity(deps: DepsMut, chain_id: String, token_1_liquidity: Uint128, token_2_liquidity: Uint128) -> Result<IbcReceiveResponse, ContractError> {
        // Get the pool for the chain_id provided
        let pool = POOLS.load(deps.storage, &chain_id)?;
        // Add liquidity to the pool
        pool.reserve_1.checked_add(token_1_liquidity);
        pool.reserve_2.checked_add(token_2_liquidity);
        POOLS.save(deps.storage, &chain_id, &pool)?;
        
        // Add to total liquidity
        let mut state = STATE.load(deps.storage)?;
        state.total_reserve_1.checked_add(token_1_liquidity);
        state.total_reserve_2.checked_add(token_2_liquidity);
        STATE.save(deps.storage, &state)?;

        
        Ok(IbcReceiveResponse::new().add_attribute("action", "add_liquidity")
        .add_attribute("chain_id", chain_id).set_ack(make_ack_success()))
    }


    // Temporary function to test IBC packet receive using the contracts and if everything works, should come from actual Pool.
    pub fn add_liquidity_chain(deps: DepsMut, env: Env, chain_id: String, token_1_liquidity: Uint128, token_2_liquidity: Uint128, channel: String) -> Result<Response, ContractError> {
        let msg = IbcMsg::SendPacket { channel_id: channel,
                data: to_json_binary(&IbcExecuteMsg::AddLiquidity { chain_id: chain_id ,
                token_1_liquidity: token_1_liquidity, token_2_liquidity: token_2_liquidity }).unwrap(),
                 timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(120))};
        
        Ok(Response::new().add_attribute("method", "adding_liquidity_chain").add_message(CosmosMsg::Ibc(msg)))
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

/* 
#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{coins, from_json};

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies();

        let msg = InstantiateMsg { count: 17 };
        let info = mock_info("creator", &coins(1000, "earth"));

        // we can just call .unwrap() to assert this was a success
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());

        // it worked, let's query the state
        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetCount {}).unwrap();
        let value: GetCountResponse = from_json(&res).unwrap();
        assert_eq!(17, value.count);
    }

    #[test]
    fn increment() {
        let mut deps = mock_dependencies();

        let msg = InstantiateMsg { count: 17 };
        let info = mock_info("creator", &coins(2, "token"));
        let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        // beneficiary can release it
        let info = mock_info("anyone", &coins(2, "token"));
        let msg = ExecuteMsg::Increment {};
        let _res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        // should increase counter by 1
        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetCount {}).unwrap();
        let value: GetCountResponse = from_json(&res).unwrap();
        assert_eq!(18, value.count);
    }

    #[test]
    fn reset() {
        let mut deps = mock_dependencies();

        let msg = InstantiateMsg { count: 17 };
        let info = mock_info("creator", &coins(2, "token"));
        let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        // beneficiary can release it
        let unauth_info = mock_info("anyone", &coins(2, "token"));
        let msg = ExecuteMsg::Reset { count: 5 };
        let res = execute(deps.as_mut(), mock_env(), unauth_info, msg);
        match res {
            Err(ContractError::Unauthorized {}) => {}
            _ => panic!("Must return unauthorized error"),
        }

        // only the original creator can reset the counter
        let auth_info = mock_info("creator", &coins(2, "token"));
        let msg = ExecuteMsg::Reset { count: 5 };
        let _res = execute(deps.as_mut(), mock_env(), auth_info, msg).unwrap();

        // should now be 5
        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetCount {}).unwrap();
        let value: GetCountResponse = from_json(&res).unwrap();
        assert_eq!(5, value.count);
    }
}
*/