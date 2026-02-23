use cosmwasm_std::{ensure, from_json, DepsMut, Env, MessageInfo, Response};
use euclid_ibc::{
    factory_ibc::FactoryCrossChainExecuteMsg,
    router_ibc::{
        RouterCrossChainExecuteMsg, RouterCrossChainSwapExecuteMsg,
        RouterCrossChainTransferVoucherExecuteMsg,
    },
};

use crate::state::{LOCKED_CHAINS, RELAYER_CONTRACT};
use euclid::{
    chain::{Chain, ChainUid, CosmosChain, EvmChain},
    cross_chain_user::CrossChainUser,
    error::ContractError,
    events::{tx_event, TxType},
    msgs::{
        hook::MetaReceive,
        router::{ManageRouterState, RegisterFactoryChainType},
    },
    utils::tx::generate_tx,
};

use crate::{
    ibc::receive::reusable_internal_call,
    state::{CHAIN_UID_TO_CHAIN, META_TRANSACTION_CONTRACT, RELEASE_FEES, STATE},
};

pub fn execute_manage_router_state(
    deps: DepsMut,
    info: MessageInfo,
    msg: ManageRouterState,
) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.admin, ContractError::Unauthorized {});
    match msg {
        ManageRouterState::Admin { admin } => {
            state.admin = admin;
            STATE.save(deps.storage, &state)?;
            Ok(Response::new().add_attribute("method", "update_admin"))
        }
        ManageRouterState::Vlp {
            vlp_code_id,
            stable_vlp_code_id,
            concentrated_vlp_code_id,
        } => {
            state.constant_product_vlp_code_id =
                vlp_code_id.unwrap_or(state.constant_product_vlp_code_id);
            state.stable_vlp_code_id = stable_vlp_code_id.unwrap_or(state.stable_vlp_code_id);
            state.concentrated_vlp_code_id =
                concentrated_vlp_code_id.unwrap_or(state.concentrated_vlp_code_id);
            STATE.save(deps.storage, &state)?;
            Ok(Response::new().add_attribute("method", "update_vlp_code_id"))
        }
        ManageRouterState::LockState { locked } => {
            state.locked = locked;
            STATE.save(deps.storage, &state)?;
            Ok(Response::new().add_attribute("method", "update_lock_state"))
        }
        ManageRouterState::RelayerContract { relayer_contract } => {
            let relayer_contract = deps.api.addr_validate(relayer_contract.as_str())?;
            RELAYER_CONTRACT.save(deps.storage, &relayer_contract)?;
            Ok(Response::new().add_attribute("method", "update_relayer_contract"))
        }
        ManageRouterState::MetaTransactionContract {
            meta_transaction_contract,
        } => {
            let meta_transaction_contract =
                deps.api.addr_validate(meta_transaction_contract.as_str())?;
            META_TRANSACTION_CONTRACT.save(deps.storage, &meta_transaction_contract)?;
            Ok(Response::new().add_attribute("method", "update_meta_transaction_contract"))
        }
        ManageRouterState::UpdateReleaseFee {
            token,
            chain_uid,
            release_fee,
        } => {
            RELEASE_FEES.save(deps.storage, (token, chain_uid), &release_fee)?;
            Ok(Response::new().add_attribute("method", "update_release_fee"))
        }
        ManageRouterState::LockChain { chain } => {
            let mut locked_chains = LOCKED_CHAINS.load(deps.storage)?;
            ensure!(
                !locked_chains.contains(&chain),
                ContractError::new("Chain already locked")
            );
            locked_chains.push(chain);
            LOCKED_CHAINS.save(deps.storage, &locked_chains)?;
            Ok(Response::new().add_attribute("method", "lock_chain"))
        }
        ManageRouterState::UnlockChain { chain } => {
            let mut locked_chains = LOCKED_CHAINS.load(deps.storage)?;
            ensure!(
                locked_chains.contains(&chain),
                ContractError::new("Chain already unlocked")
            );
            locked_chains.retain(|x| x != &chain);
            LOCKED_CHAINS.save(deps.storage, &locked_chains)?;
            Ok(Response::new().add_attribute("method", "unlock_chain"))
        }
    }
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
    let sender = CrossChainUser::new(vsl_chain_uid.clone(), info.sender.to_string());

    let tx_id = generate_tx(deps, &env, &sender)?;

    ensure!(
        chain_uid != vsl_chain_uid,
        ContractError::new("Cannot use VSL chain uid")
    );

    let state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.admin, ContractError::Unauthorized {});

    let response = Response::new()
        .add_event(tx_event(
            &tx_id,
            info.sender.as_str(),
            TxType::RegisterFactory,
        ))
        .add_attribute("method", "register_factory");
    let msg = FactoryCrossChainExecuteMsg::RegisterFactory {
        chain_uid: chain_uid.clone(),
        chain_type: chain_info.clone(),
        tx_id: tx_id.clone(),
    };
    match chain_info {
        RegisterFactoryChainType::Cosmos(cosmos_info) => {
            ensure!(
                cosmos_info.factory_address.to_lowercase() == cosmos_info.factory_address,
                ContractError::new("Factory address must be lowercase")
            );
            // Save chain info because this call will fail if the tx is not sucessful
            let chain = Chain {
                chain_uid: chain_uid.clone(),
                factory_address: cosmos_info.factory_address,
                chain_type: euclid::chain::ChainType::Cosmos(CosmosChain {
                    // We will get this later
                    chain_id: cosmos_info.factory_chain_id,
                }),
            };
            Ok(response.add_submessage(msg.to_msg(
                deps,
                &env,
                sender.address,
                chain,
                None,
                None,
            )?))
        }
        RegisterFactoryChainType::Native(native_info) => {
            // Save chain info because this call will fail if the tx is not sucessful
            let chain = Chain {
                chain_uid: chain_uid.clone(),
                factory_address: native_info.factory_address,
                chain_type: euclid::chain::ChainType::Native {},
            };
            Ok(response.add_submessage(msg.to_msg(
                deps,
                &env,
                sender.address,
                chain,
                None,
                None,
            )?))
        }
        RegisterFactoryChainType::Evm(evm_info) => {
            // Save chain info because this call will fail if the tx is not sucessful
            ensure!(
                evm_info.factory_address.to_lowercase() == evm_info.factory_address,
                ContractError::new("Factory address must be lowercase")
            );
            let chain = Chain {
                chain_uid: chain_uid.clone(),
                factory_address: evm_info.factory_address,
                chain_type: euclid::chain::ChainType::Evm(EvmChain {
                    chain_id: evm_info.factory_chain_id,
                }),
            };
            Ok(response.add_submessage(msg.to_msg(
                deps,
                &env,
                sender.address,
                chain,
                None,
                None,
            )?))
        }
    }
}

pub fn execute_meta_receive(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    msg: MetaReceive,
) -> Result<Response, ContractError> {
    let meta_transaction_contract = META_TRANSACTION_CONTRACT.load(deps.storage)?;
    ensure!(
        info.sender == meta_transaction_contract,
        ContractError::Unauthorized {}
    );
    let sender = msg.verified_sender;
    let data: RouterCrossChainExecuteMsg = from_json(msg.call_data.clone())?;
    match data {
        RouterCrossChainExecuteMsg::Swap(swap_msg) => {
            process_swap_meta_transaction(deps, env, info, sender, swap_msg)
        }
        RouterCrossChainExecuteMsg::TransferVoucher(transfer_voucher_msg) => {
            process_transfer_voucher_meta_transaction(deps, env, info, sender, transfer_voucher_msg)
        }
        _ => Err(ContractError::Generic {
            err: "Unsupported message type".to_string(),
        }),
    }
}

fn process_swap_meta_transaction(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    sender: CrossChainUser,
    mut swap_msg: RouterCrossChainSwapExecuteMsg,
) -> Result<Response, ContractError> {
    ensure!(
        sender.chain_uid == swap_msg.sender.chain_uid,
        ContractError::new("Chain UID mismatch")
    );

    ensure!(
        swap_msg.sender.address == sender.address,
        ContractError::new("Sender address mismatch")
    );

    ensure!(
        swap_msg.asset_in.token_type.is_voucher(),
        ContractError::new("Asset IN does not match asset OUT")
    );

    swap_msg.tx_id = generate_tx(deps, &env, &sender)?;

    reusable_internal_call(
        deps,
        env,
        info,
        RouterCrossChainExecuteMsg::Swap(swap_msg),
        sender.chain_uid,
    )
}

fn process_transfer_voucher_meta_transaction(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    sender: CrossChainUser,
    mut transfer_voucher_msg: RouterCrossChainTransferVoucherExecuteMsg,
) -> Result<Response, ContractError> {
    ensure!(
        sender.chain_uid == transfer_voucher_msg.sender.chain_uid,
        ContractError::new("Chain UID mismatch")
    );

    ensure!(
        transfer_voucher_msg.sender.address == sender.address,
        ContractError::new("Sender address mismatch")
    );

    transfer_voucher_msg.tx_id = generate_tx(deps, &env, &sender)?;

    reusable_internal_call(
        deps,
        env,
        info,
        RouterCrossChainExecuteMsg::TransferVoucher(transfer_voucher_msg),
        sender.chain_uid,
    )
}
