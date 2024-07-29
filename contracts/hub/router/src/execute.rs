use cosmwasm_std::{
    ensure, to_json_binary, CosmosMsg, DepsMut, Env, IbcMsg, IbcTimeout, MessageInfo, Response,
    SubMsg, Uint128, WasmMsg,
};
use euclid::{
    chain::{ChainUid, CrossChainUser, CrossChainUserWithLimit},
    error::ContractError,
    events::{tx_event, TxType},
    msgs::vcoin::ExecuteBurn,
    timeout::get_timeout,
    token::Token,
    utils::generate_tx,
    vcoin::BalanceKey,
};
use euclid_ibc::msg::HubIbcExecuteMsg;

use crate::{
    reply::VCOIN_BURN_REPLY_ID,
    state::{CHAIN_UID_TO_CHAIN, DEREGISTERED_CHAINS, ESCROW_BALANCES, STATE},
};

// Function to update the pool code ID
pub fn execute_update_vlp_code_id(
    deps: DepsMut,
    info: MessageInfo,
    new_vlp_code_id: u64,
) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;

    ensure!(info.sender == state.admin, ContractError::Unauthorized {});

    state.vlp_code_id = new_vlp_code_id;

    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("method", "update_pool_code_id")
        .add_attribute("new_vlp_code_id", new_vlp_code_id.to_string()))
}

pub fn execute_update_lock(deps: DepsMut, info: MessageInfo) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.admin, ContractError::Unauthorized {});

    // Switch to opposite lock state
    state.locked = !state.locked;

    STATE.save(deps.storage, &state)?;
    let lock_message = if state.locked { "locked" } else { "unlocked" };

    Ok(Response::new()
        .add_attribute("method", "update_lock")
        .add_attribute("new_lock_state", lock_message.to_string()))
}

pub fn execute_deregister_chain(
    deps: DepsMut,
    info: MessageInfo,
    chain: ChainUid,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.admin, ContractError::Unauthorized {});
    let mut deregistered_chains = DEREGISTERED_CHAINS.load(deps.storage)?;

    ensure!(
        !deregistered_chains.contains(&chain),
        ContractError::ChainAlreadyExist {}
    );

    deregistered_chains.push(chain.clone());

    DEREGISTERED_CHAINS.save(deps.storage, &deregistered_chains)?;

    Ok(Response::new()
        .add_attribute("method", "deregister_chain")
        .add_attribute("chain", chain.to_string()))
}

pub fn execute_reregister_chain(
    deps: DepsMut,
    info: MessageInfo,
    chain: ChainUid,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.admin, ContractError::Unauthorized {});
    let mut deregistered_chains = DEREGISTERED_CHAINS.load(deps.storage)?;

    ensure!(
        deregistered_chains.contains(&chain),
        ContractError::ChainNotFound {}
    );

    deregistered_chains.retain(|x| x != &chain);

    DEREGISTERED_CHAINS.save(deps.storage, &deregistered_chains)?;

    Ok(Response::new()
        .add_attribute("method", "reregister_chain")
        .add_attribute("chain", chain.to_string()))
}

// Function to update the pool code ID
pub fn execute_register_factory(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    chain_uid: ChainUid,
    channel: String,
    timeout: Option<u64>,
) -> Result<Response, ContractError> {
    let chain_uid = chain_uid.validate()?.to_owned();

    let vsl_chain_uid = ChainUid::vsl_chain_uid()?;
    let sender = CrossChainUser {
        chain_uid: vsl_chain_uid.clone(),
        address: info.sender.to_string(),
    };

    let tx_id = generate_tx(deps.branch(), &env, &sender)?;

    ensure!(
        chain_uid != vsl_chain_uid,
        ContractError::new("Cannot use VSL chain uid")
    );

    // TODO: Add check for existing chain ids
    let state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.admin, ContractError::Unauthorized {});

    let msg = HubIbcExecuteMsg::RegisterFactory {
        chain_uid,
        tx_id: tx_id.clone(),
    };

    let timeout = get_timeout(timeout)?;

    let packet = IbcMsg::SendPacket {
        channel_id: channel.clone(),
        data: to_json_binary(&msg)?,
        timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(timeout)),
    };

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            info.sender.as_str(),
            TxType::RegisterFactory,
        ))
        .add_attribute("method", "register_factory")
        .add_attribute("channel", channel)
        .add_attribute("timeout", timeout.to_string())
        .add_message(CosmosMsg::Ibc(packet)))
}

// Function to update the pool code ID
pub fn execute_release_escrow(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    sender: CrossChainUser,
    token: Token,
    amount: Uint128,
    cross_chain_addresses: Vec<CrossChainUserWithLimit>,
    timeout: Option<u64>,
    tx_id: String,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(
        info.sender == env.contract.address,
        ContractError::Unauthorized {}
    );

    let vcoin_address = state
        .vcoin_address
        .ok_or(ContractError::new("vcoin doesn't exist"))?
        .into_string();

    let user_balance: euclid::msgs::vcoin::GetBalanceResponse = deps.querier.query_wasm_smart(
        vcoin_address.clone(),
        &euclid::msgs::vcoin::QueryMsg::GetBalance {
            balance_key: BalanceKey {
                cross_chain_user: sender.clone(),
                token_id: token.to_string(),
            },
        },
    )?;

    // Ensure that user has enough vcoin balance to actually trigger escrow release
    ensure!(
        user_balance.amount.ge(&amount),
        ContractError::InsufficientFunds {}
    );

    let timeout = get_timeout(timeout)?;
    let mut ibc_messages: Vec<CosmosMsg> = vec![];

    let mut cross_chain_addresses_iterator = cross_chain_addresses.into_iter().peekable();
    let mut remaining_withdraw_amount = amount;

    let mut transfer_amount = Uint128::zero();
    // Ensure that the amount desired doesn't exceed the current balance
    while !remaining_withdraw_amount.is_zero() && cross_chain_addresses_iterator.peek().is_some() {
        let cross_chain_address = cross_chain_addresses_iterator
            .next()
            .ok_or(ContractError::new("Cross Chain Address Iter Faiiled"))?;
        let chain =
            CHAIN_UID_TO_CHAIN.load(deps.storage, cross_chain_address.user.chain_uid.clone())?;

        let escrow_key =
            ESCROW_BALANCES.key((token.clone(), cross_chain_address.user.chain_uid.clone()));
        let escrow_balance = escrow_key
            .may_load(deps.storage)?
            .unwrap_or(Uint128::zero());

        let release_amount = if remaining_withdraw_amount.ge(&escrow_balance) {
            escrow_balance
        } else {
            remaining_withdraw_amount
        };

        let release_amount = release_amount.min(cross_chain_address.limit.unwrap_or(Uint128::MAX));

        if release_amount.is_zero() {
            continue;
        }

        escrow_key.save(deps.storage, &escrow_balance.checked_sub(release_amount)?)?;

        transfer_amount = transfer_amount.checked_add(release_amount)?;

        // Prepare IBC Release Message
        let send_msg = HubIbcExecuteMsg::ReleaseEscrow {
            sender: sender.clone(),
            amount: release_amount,
            token: token.clone(),
            to_address: cross_chain_address.user.address.clone(),
            tx_id: tx_id.clone(),
        };
        let packet = IbcMsg::SendPacket {
            channel_id: chain.from_hub_channel,
            data: to_json_binary(&send_msg)?,
            timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(timeout)),
        };

        remaining_withdraw_amount = remaining_withdraw_amount.checked_sub(release_amount)?;
        ibc_messages.push(CosmosMsg::Ibc(packet));
    }

    ensure!(
        transfer_amount.checked_add(remaining_withdraw_amount)? == amount,
        ContractError::new("Amount mismatch after trasnfer calculations")
    );
    let burn_vcoin_msg = euclid::msgs::vcoin::ExecuteMsg::Burn(ExecuteBurn {
        amount: transfer_amount,
        balance_key: BalanceKey {
            cross_chain_user: sender.clone(),
            token_id: token.to_string(),
        },
    });

    let burn_vcoin_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: vcoin_address,
        msg: to_json_binary(&burn_vcoin_msg)?,
        funds: vec![],
    });

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            sender.address.as_str(),
            TxType::EscrowRelease,
        ))
        .add_attribute("tx_id", tx_id)
        .add_submessage(SubMsg::reply_always(burn_vcoin_msg, VCOIN_BURN_REPLY_ID))
        .add_attribute("method", "release_escrow")
        .add_attribute("release_expected", amount)
        .add_attribute("actual_released", transfer_amount)
        .add_messages(ibc_messages))
}
