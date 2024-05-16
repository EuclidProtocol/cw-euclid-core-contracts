use cosmwasm_std::{
    to_json_binary, CosmosMsg, DepsMut, Env, IbcMsg, IbcTimeout, MessageInfo, Response, Uint128,
};
use euclid::{
    error::ContractError,
    pool::PoolRequest,
    token::{PairInfo, Token},
};
use euclid_ibc::msg::IbcExecuteMsg;

use crate::state::{POOL_REQUESTS, STATE, VLP_TO_POOL};

// Function to send IBC request to Router in VLS to create a new pool
pub fn execute_request_pool_creation(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    pair_info: PairInfo,
    token_1_reserve: Uint128,
    token_2_reserve: Uint128,
    channel: String,
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
            return Err(ContractError::InsufficientDeposit {});
        }
        // Check for funds sent with the message
        let amt = info
            .funds
            .iter()
            .find(|x| x.denom == token_1.get_denom())
            .unwrap();
        if amt.amount < token_1_reserve {
            return Err(ContractError::InsufficientDeposit {});
        }
    }

    // Same for token 2
    if token_2.is_native() {
        // If funds empty return error
        if info.funds.is_empty() {
            return Err(ContractError::InsufficientDeposit {});
        }
        // Check for funds sent with the message
        let amt = info
            .funds
            .iter()
            .find(|x| x.denom == token_2.get_denom())
            .unwrap();
        if amt.amount < token_2_reserve {
            return Err(ContractError::InsufficientDeposit {});
        }
    }

    // Create pool request id
    let pool_rq_id = format!(
        "{}-{}-{}",
        info.sender.clone().to_string(),
        env.block.height.clone(),
        env.block.time.to_string()
    );

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
        })
        .unwrap(),

        timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(60)),
    };

    msgs.push(ibc_packet.into());

    Ok(Response::new()
        .add_attribute("method", "request_pool_creation")
        .add_messages(msgs))
}

// Function to send IBC request to Router in VLS to perform a swap
pub fn execute_swap(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    asset: Token,
    asset_amount: Uint128,
    min_amount_out: Uint128,
    channel: String,
    swap_id: String,
) -> Result<Response, ContractError> {
    // Load the state
    let state = STATE.load(deps.storage)?;

    let pool_address = info.sender;
    

    
    // Create IBC packet to send to Router
    let ibc_packet = IbcMsg::SendPacket {
        channel_id: channel.clone(),
        data: to_json_binary(&IbcExecuteMsg::Swap {
            chain_id: state.chain_id,
            asset,
            asset_amount,
            min_amount_out,
            channel,
            swap_id,
            pool_address,
        })
        .unwrap(),
        timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(60)),
    };

    let msg = CosmosMsg::Ibc(ibc_packet);

    Ok(Response::new()
        .add_attribute("method", "request_pool_creation")
        .add_message(msg))
}
