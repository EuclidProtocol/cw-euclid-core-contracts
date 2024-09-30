use cosmwasm_std::{
    ensure, from_json, to_json_binary, Addr, Binary, CosmosMsg, DepsMut, Env, IbcMsg, IbcTimeout,
    MessageInfo, Response, SubMsg, Uint128, WasmMsg,
};

use euclid::{
    chain::{Chain, ChainUid, CrossChainUser, CrossChainUserWithLimit},
    error::ContractError,
    events::{tx_event, TxType},
    msgs::{
        router::{ExecuteMsg, RegisterFactoryChainType},
        virtual_balance::ExecuteBurn,
    },
    timeout::get_timeout,
    token::Token,
    utils::tx::generate_tx,
    virtual_balance::BalanceKey,
};
use euclid_ibc::msg::{ChainIbcExecuteMsg, HubIbcExecuteMsg};

use crate::{
    ibc::receive,
    query::verify_cross_chain_addresses,
    state::{
        State, CHAIN_UID_TO_CHAIN, CHANNEL_TO_CHAIN_UID, DEREGISTERED_CHAINS, ESCROW_BALANCES,
        STATE,
    },
};

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

pub fn execute_register_factory(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    chain_uid: ChainUid,
    chain_info: RegisterFactoryChainType,
) -> Result<Response, ContractError> {
    let chain_uid = chain_uid.validate()?.to_owned();

    ensure!(
        !CHAIN_UID_TO_CHAIN.has(deps.storage, chain_uid.clone()),
        ContractError::new("Factory already exists")
    );

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

    let response = Response::new()
        .add_event(tx_event(
            &tx_id,
            info.sender.as_str(),
            TxType::RegisterFactory,
        ))
        .add_attribute("method", "register_factory");
    let msg = HubIbcExecuteMsg::RegisterFactory {
        chain_uid: chain_uid.clone(),
        tx_id: tx_id.clone(),
    };
    match chain_info {
        RegisterFactoryChainType::Ibc(ibc_info) => {
            let timeout = get_timeout(ibc_info.timeout)?;
            let packet = IbcMsg::SendPacket {
                channel_id: ibc_info.channel.clone(),
                data: to_json_binary(&msg)?,
                timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(timeout)),
            };

            Ok(response
                .add_attribute("channel", ibc_info.channel)
                .add_attribute("timeout", timeout.to_string())
                .add_message(CosmosMsg::Ibc(packet)))
        }
        RegisterFactoryChainType::Native(native_info) => {
            // Save chain info because this call will fail if the tx is not sucessful
            let chain = Chain {
                factory: native_info.factory_address,
                factory_chain_id: env.block.chain_id.clone(),
                chain_type: euclid::chain::ChainType::Native {},
            };
            Ok(response.add_submessage(msg.to_msg(deps, &env, chain, 0)?))
        }
    }
}

pub fn execute_update_factory_channel(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    new_channel: String,
    chain_uid: ChainUid,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.admin, ContractError::Unauthorized {});

    let chain_uid = chain_uid.validate()?.to_owned();
    let chain_info = CHAIN_UID_TO_CHAIN
        .load(deps.storage, chain_uid.clone())
        .map_err(|_err| ContractError::new("Factory doesn't exist"))?;

    // Make sure that the new channel doesn't already exist
    ensure!(
        !CHANNEL_TO_CHAIN_UID.has(deps.storage, new_channel.clone()),
        ContractError::ChannelAlreadyExists {}
    );

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

    let response = Response::new()
        .add_event(tx_event(
            &tx_id,
            info.sender.as_str(),
            TxType::UpdateFactoryChannel,
        ))
        .add_attribute("method", "update_factory_channel");

    let msg = HubIbcExecuteMsg::UpdateFactoryChannel {
        chain_uid: chain_uid.clone(),
        tx_id: tx_id.clone(),
    };

    if !chain_info.is_native() {
        let timeout = get_timeout(None)?;
        let packet = IbcMsg::SendPacket {
            channel_id: new_channel.clone(),
            data: to_json_binary(&msg)?,
            timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(timeout)),
        };

        Ok(response
            .add_attribute("channel", new_channel)
            .add_attribute("timeout", timeout.to_string())
            .add_message(CosmosMsg::Ibc(packet)))
    } else {
        // Can't update channel for a local chain
        Err(ContractError::NoChannelForLocalChain {})
    }
}

pub fn execute_withdraw_voucher(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    token: Token,
    amount: Option<Uint128>,
    cross_chain_addresses: Vec<CrossChainUserWithLimit>,
    timeout: Option<u64>,
) -> Result<Response, ContractError> {
    verify_cross_chain_addresses(
        deps.as_ref(),
        cross_chain_addresses
            .clone()
            .into_iter()
            .map(|x| x.user)
            .collect(),
    )?;
    let cross_chain_user = CrossChainUser {
        chain_uid: ChainUid::vsl_chain_uid()?,
        address: info.sender.to_string(),
    };
    let tx_id = generate_tx(deps.branch(), &env, &cross_chain_user)?;
    let msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        msg: to_json_binary(&ExecuteMsg::ReleaseEscrowInternal {
            sender: cross_chain_user,
            token,
            amount,
            cross_chain_addresses,
            timeout,
            tx_id: tx_id.clone(),
        })?,
        funds: vec![],
    });

    Ok(Response::new()
        .add_message(msg)
        .add_attribute("method", "withdraw_voucher"))
}

#[allow(clippy::too_many_arguments)]
pub fn execute_release_escrow(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    sender: CrossChainUser,
    token: Token,
    // Leaving this empty means that we will release the entire balance
    amount: Option<Uint128>,
    cross_chain_addresses: Vec<CrossChainUserWithLimit>,
    timeout: Option<u64>,
    tx_id: String,
) -> Result<Response, ContractError> {
    ensure!(
        info.sender == env.contract.address,
        ContractError::Unauthorized {}
    );

    let state = STATE.load(deps.storage)?;

    let virtual_balance_address = state
        .virtual_balance_address
        .ok_or(ContractError::new("virtual balance doesn't exist"))?
        .into_string();

    let user_balance: euclid::msgs::virtual_balance::GetBalanceResponse =
        deps.querier.query_wasm_smart(
            virtual_balance_address.clone(),
            &euclid::msgs::virtual_balance::QueryMsg::GetBalance {
                balance_key: BalanceKey {
                    cross_chain_user: sender.clone(),
                    token_id: token.to_string(),
                },
            },
        )?;

    // Ensure that user has enough virtual balance balance to actually trigger escrow release
    let amount = amount.unwrap_or(user_balance.amount);
    ensure!(
        user_balance.amount.ge(&amount),
        ContractError::InsufficientFunds {}
    );

    let mut response = Response::new()
        .add_event(tx_event(
            &tx_id,
            sender.address.as_str(),
            TxType::EscrowRelease,
        ))
        .add_attribute("tx_id", tx_id);

    let timeout = get_timeout(timeout)?;
    let mut release_msgs: Vec<SubMsg> = vec![];

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
            // We can't use same tx id because it might conflict with pending requests on receiving chain
            tx_id: generate_tx(deps.branch(), &env, &sender)?,
            chain_uid: cross_chain_address.user.chain_uid.clone(),
        }
        .to_msg(deps, &env, chain, timeout)?;

        response = response.add_attribute(
            format!(
                "release_escrow_expected_{sender}",
                sender = cross_chain_address.user.to_sender_string()
            ),
            release_amount,
        );

        remaining_withdraw_amount = remaining_withdraw_amount.checked_sub(release_amount)?;
        release_msgs.push(send_msg);
    }

    ensure!(
        transfer_amount.checked_add(remaining_withdraw_amount)? == amount,
        ContractError::new("Amount mismatch after trasnfer calculations")
    );

    if !transfer_amount.is_zero() {
        let burn_virtual_balance_msg =
            euclid::msgs::virtual_balance::ExecuteMsg::Burn(ExecuteBurn {
                amount: transfer_amount,
                balance_key: BalanceKey {
                    cross_chain_user: sender.clone(),
                    token_id: token.to_string(),
                },
            });

        let burn_virtual_balance_msg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: virtual_balance_address,
            msg: to_json_binary(&burn_virtual_balance_msg)?,
            funds: vec![],
        });
        response = response.add_message(burn_virtual_balance_msg);
    }

    Ok(response
        .add_attribute("method", "release_escrow")
        .add_attribute("release_expected", amount)
        .add_attribute("actual_released", transfer_amount)
        .add_submessages(release_msgs))
}

pub fn execute_native_receive_callback(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    chain_uid: ChainUid,
    msg: Binary,
) -> Result<Response, ContractError> {
    let chain_uid = chain_uid.validate()?.clone();
    let msg: ChainIbcExecuteMsg = from_json(msg)?;
    let chain = CHAIN_UID_TO_CHAIN.load(deps.storage, chain_uid.clone())?;
    // Only native chains can directly use this messages
    ensure!(chain.is_native(), ContractError::Unauthorized {});

    // Only registered factory contract can execute this message
    ensure!(chain.factory == info.sender, ContractError::Unauthorized {});
    receive::reusable_internal_call(deps, env, info, msg, chain_uid)
}

pub fn execute_update_router_state(
    deps: DepsMut,
    info: MessageInfo,
    admin: Option<String>,
    vlp_code_id: Option<u64>,
    virtual_balance_address: Option<Addr>,
    locked: Option<bool>,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.admin, ContractError::Unauthorized {});

    let verified_virtual_balance_address: Result<Option<Addr>, ContractError> =
        virtual_balance_address
            .as_ref()
            .map_or(Ok(state.virtual_balance_address), |address| {
                let validated_addr = Some(deps.api.addr_validate(address.as_str())?);
                Ok(validated_addr)
            });

    // Validate Admin Address if provided
    let verified_admin = if let Some(ref admin) = admin {
        deps.api.addr_validate(admin.as_str())?.to_string()
    } else {
        state.admin
    };

    let state = State {
        admin: verified_admin,
        vlp_code_id: vlp_code_id.unwrap_or(state.vlp_code_id),
        virtual_balance_address: verified_virtual_balance_address?,
        locked: locked.unwrap_or(state.locked),
    };

    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("method", "update_state")
        .add_attribute("admin", admin.unwrap_or("unchanged".to_string()))
        .add_attribute(
            "vlp_code_id",
            vlp_code_id.map_or("unchanged".to_string(), |code_id| code_id.to_string()),
        )
        .add_attribute(
            "virtual_balance_address",
            virtual_balance_address.map_or("unchanged".to_string(), |addr| addr.to_string()),
        )
        .add_attribute(
            "locked",
            locked.map_or("unchanged".to_string(), |locked_val| locked_val.to_string()),
        ))
}
