#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult};
use cw2::set_contract_version;
use euclid::error::ContractError;
// use cw2::set_contract_version;

use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::{State, STATE};

use self::execute::execute_request_pool_creation;


// version info for migration info
const CONTRACT_NAME: &str = "crates.io:factory";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");


#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    
    let state = State {
        router_contract: msg.router_contract.clone(),
        chain_id: msg.chain_id.clone(),
        admin: info.sender.clone().to_string(),
        pool_code_id: msg.pool_code_id.clone(),
    };

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    
    STATE.save(deps.storage, &state)?;
    
    Ok(Response::new()
    .add_attribute("method", "instantiate")
    .add_attribute("router_contract", msg.router_contract)
    .add_attribute("chain_id", msg.chain_id.clone())
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
        ExecuteMsg::RequestPoolCreation { pair_info, token_1_reserve, token_2_reserve, channel  } => execute_request_pool_creation(deps, env, info,  pair_info, token_1_reserve, token_2_reserve, channel),
    }
}

pub mod execute {
    use cosmwasm_std::{to_json_binary, CosmosMsg, DepsMut, Env, IbcMsg, IbcPacket, IbcTimeout, MessageInfo, Response, Uint128};
    use euclid::{error::ContractError, pool::PoolRequest, token::PairInfo};
    use euclid_ibc::msg::IbcExecuteMsg;

    use crate::state::{POOL_REQUESTS, STATE};


    // Function to send IBC request to Router in VLS to create a new pool
    pub fn execute_request_pool_creation(deps: DepsMut,
         env: Env,
        info: MessageInfo,
        pair_info: PairInfo,
        token_1_reserve: Uint128,
        token_2_reserve: Uint128,
        channel: String
        ) -> Result<Response, ContractError> {

        // Load the state
        let state = STATE.load(deps.storage)?;
        
        // Fetch 2 tokens from pair info
        let token_1 = pair_info.token_1.clone();
        let token_2 = pair_info.token_2.clone();

        let mut msgs: Vec<CosmosMsg> = Vec::new();
        // For smart contract token, create message to deposit token to Factory
        if token_1.is_smart() {
            let msg = token_1.create_transfer_msg(token_1_reserve, env.contract.address.to_string());
            msgs.push(msg);
        }
        // DO same for token 2 
        if token_2.is_smart() {
            let msg = token_2.create_transfer_msg(token_2_reserve, env.contract.address.to_string());
            msgs.push(msg);
        }

        // If native, check funds sent to ensure that the contract has enough funds to create the pool
        if token_1.is_native() {
            // If funds empty return error
        if info.funds.is_empty() {
            return Err(ContractError::InsufficientDeposit {  });
        }
        // Check for funds sent with the message
        let amt = info.funds.iter().find(|x| x.denom == token_1.get_denom()).unwrap();
        if amt.amount < token_1_reserve {
            return Err(ContractError::InsufficientDeposit {  });
        }
        }

        // Same for token 2
        if token_2.is_native() {
            // If funds empty return error
        if info.funds.is_empty() {
            return Err(ContractError::InsufficientDeposit {  });
        }
        // Check for funds sent with the message
        let amt = info.funds.iter().find(|x| x.denom == token_2.get_denom()).unwrap();
        if amt.amount < token_2_reserve {
            return Err(ContractError::InsufficientDeposit {  });
        }
        }

        // Create pool request id
        let pool_rq_id = format!("{}-{}-{}", info.sender.clone().to_string() , env.block.height.clone(), env.block.time.to_string());

        // Create a Request in state
        let pool_request = PoolRequest {
            chain: state.chain_id.clone(),
            pool_rq_id: pool_rq_id.clone(),
            channel: channel.clone(),
        };
        POOL_REQUESTS.save(deps.storage, info.sender.clone().to_string(), &pool_request)?;

        // Create IBC packet to send to Router
        let ibc_packet = IbcMsg::SendPacket {
            channel_id: channel.clone(),
            data: to_json_binary(&IbcExecuteMsg::RequestPoolCreation {
                chain_id: state.chain_id.clone(),
                pair_info: pair_info.clone(),
                token_1_reserve: token_1_reserve.clone(),
                token_2_reserve: token_2_reserve.clone(),
                pool_rq_id: pool_rq_id.clone(),
            }).unwrap(),

            timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(60))
        };

        msgs.push(ibc_packet.into());

        Ok(Response::new()
        .add_attribute("method", "request_pool_creation")
        .add_messages(msgs))

    }

}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(_deps: Deps, _env: Env, _msg: QueryMsg) -> StdResult<Binary> {
    unimplemented!()
}

#[cfg(test)]
mod tests {}
