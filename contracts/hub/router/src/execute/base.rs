use cosmwasm_std::{ensure, from_json, DepsMut, Env, MessageInfo, Response};
use euclid_ibc::{
    factory_ibc::FactoryCrossChainExecuteMsg,
    router_ibc::{
        RouterCrossChainExecuteMsg, RouterCrossChainSwapExecuteMsg,
        RouterCrossChainTransferVoucherExecuteMsg,
    },
};

use crate::state::{
    CHAIN_TIMEOUT_SECONDS, DEFAULT_RELEASE_FEE, FEE_STATE, LOCKED_CHAINS, RELAYER_CONTRACT,
};
use euclid::{
    admin,
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
    state::{ADMIN, CHAIN_UID_TO_CHAIN, META_TRANSACTION_CONTRACT, RELEASE_FEES, STATE},
};

pub fn execute_manage_router_state(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ManageRouterState,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;
    let mut state = STATE.load(deps.storage)?;
    let mut admins = ADMIN.load(deps.storage)?;
    match msg {
        ManageRouterState::Admins { admin_type, admin } => {
            let (updated_admins, response) =
                admin::update_admin(&admins, &deps, &env, &info.sender, admin, admin_type)?;
            admins = updated_admins;
            ADMIN.save(deps.storage, &admins)?;
            Ok(response)
        }
        ManageRouterState::Vlp {
            vlp_code_id,
            stable_vlp_code_id,
            concentrated_vlp_code_id,
        } => {
            ensure!(
                info.sender == admins.migration_admin,
                ContractError::Unauthorized {}
            );
            state.constant_product_vlp_code_id =
                vlp_code_id.unwrap_or(state.constant_product_vlp_code_id);
            state.stable_vlp_code_id = stable_vlp_code_id.unwrap_or(state.stable_vlp_code_id);
            state.concentrated_vlp_code_id =
                concentrated_vlp_code_id.unwrap_or(state.concentrated_vlp_code_id);
            STATE.save(deps.storage, &state)?;
            Ok(Response::new().add_attribute("method", "update_vlp_code_id"))
        }
        ManageRouterState::LockState { locked } => {
            ensure!(
                info.sender == admins.general_admin,
                ContractError::Unauthorized {}
            );
            state.locked = locked;
            STATE.save(deps.storage, &state)?;
            Ok(Response::new()
                .add_attribute("method", "update_lock_state")
                .add_attribute("locked", locked.to_string()))
        }
        ManageRouterState::RelayerContract { relayer_contract } => {
            ensure!(
                info.sender == admins.general_admin,
                ContractError::Unauthorized {}
            );
            let relayer_contract = deps.api.addr_validate(relayer_contract.as_str())?;
            RELAYER_CONTRACT.save(deps.storage, &relayer_contract)?;
            Ok(Response::new().add_attribute("method", "update_relayer_contract"))
        }
        ManageRouterState::MetaTransactionContract {
            meta_transaction_contract,
        } => {
            ensure!(
                info.sender == admins.general_admin,
                ContractError::Unauthorized {}
            );
            let meta_transaction_contract =
                deps.api.addr_validate(meta_transaction_contract.as_str())?;
            META_TRANSACTION_CONTRACT.save(deps.storage, &meta_transaction_contract)?;
            Ok(Response::new()
                .add_attribute("method", "update_meta_transaction_contract")
                .add_attribute(
                    "meta_transaction_contract",
                    meta_transaction_contract.to_string(),
                ))
        }
        ManageRouterState::UpdateFeeState {
            release_fee_recipient,
            default_fee_recipient,
        } => {
            ensure!(
                info.sender == admins.fee_admin,
                ContractError::Unauthorized {}
            );
            let mut fee_state = FEE_STATE.load(deps.storage)?;
            if let Some(release_fee_recipient) = release_fee_recipient {
                fee_state.release_fee_recipient = release_fee_recipient;
            }
            if let Some(default_fee_recipient) = default_fee_recipient {
                fee_state.default_fee_recipient = default_fee_recipient;
            }
            FEE_STATE.save(deps.storage, &fee_state)?;
            Ok(Response::new()
                .add_attribute("method", "update_fee_state")
                .add_attribute(
                    "release_fee_recipient",
                    fee_state.release_fee_recipient.to_string(),
                )
                .add_attribute(
                    "default_fee_recipient",
                    fee_state.default_fee_recipient.to_string(),
                ))
        }
        ManageRouterState::UpdateReleaseFee {
            token,
            chain_uid,
            release_fee,
        } => {
            ensure!(
                info.sender == admins.fee_admin,
                ContractError::Unauthorized {}
            );
            RELEASE_FEES.save(
                deps.storage,
                (token.clone(), chain_uid.clone()),
                &release_fee,
            )?;
            Ok(Response::new()
                .add_attribute("method", "update_release_fee")
                .add_attribute("token", token.to_string())
                .add_attribute("chain_uid", chain_uid.to_string())
                .add_attribute("release_fee", release_fee.to_string()))
        }
        ManageRouterState::UpdateDefaultReleaseFee {
            default_release_fee,
        } => {
            ensure!(
                info.sender == admins.fee_admin,
                ContractError::Unauthorized {}
            );
            DEFAULT_RELEASE_FEE.save(deps.storage, &default_release_fee)?;
            Ok(Response::new()
                .add_attribute("method", "update_default_release_fee")
                .add_attribute("default_release_fee", default_release_fee.to_string()))
        }
        ManageRouterState::LockChain { chain } => {
            ensure!(
                info.sender == admins.general_admin,
                ContractError::Unauthorized {}
            );
            let mut locked_chains = LOCKED_CHAINS.load(deps.storage)?;
            ensure!(
                !locked_chains.contains(&chain),
                ContractError::new("Chain already locked")
            );
            locked_chains.push(chain.clone());
            LOCKED_CHAINS.save(deps.storage, &locked_chains)?;
            Ok(Response::new()
                .add_attribute("method", "lock_chain")
                .add_attribute("chain", chain.to_string())
                .add_attribute("locked", "true"))
        }
        ManageRouterState::UnlockChain { chain } => {
            ensure!(
                info.sender == admins.general_admin,
                ContractError::Unauthorized {}
            );
            let mut locked_chains = LOCKED_CHAINS.load(deps.storage)?;
            ensure!(
                locked_chains.contains(&chain),
                ContractError::new("Chain already unlocked")
            );
            locked_chains.retain(|x| x != &chain);
            LOCKED_CHAINS.save(deps.storage, &locked_chains)?;
            Ok(Response::new()
                .add_attribute("method", "unlock_chain")
                .add_attribute("chain", chain.to_string())
                .add_attribute("locked", "false"))
        }
        ManageRouterState::UpdateChainTimeout { chain_uid, timeout } => {
            ensure!(
                info.sender == admins.general_admin,
                ContractError::Unauthorized {}
            );
            CHAIN_TIMEOUT_SECONDS.save(deps.storage, chain_uid.clone(), &timeout)?;
            Ok(Response::new()
                .add_attribute("method", "update_chain_timeout")
                .add_attribute("chain_uid", chain_uid.to_string())
                .add_attribute("timeout", timeout.to_string()))
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
    cw_utils::nonpayable(&info)?;
    let admins = ADMIN.load(deps.storage)?;
    ensure!(
        info.sender == admins.general_admin,
        ContractError::Unauthorized {}
    );

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
    cw_utils::nonpayable(&info)?;
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

#[cfg(test)]
mod tests {
    use cosmwasm_std::{
        testing::{message_info, mock_env},
        to_json_binary, Addr, Uint256,
    };
    use euclid::{
        admin::AdminType,
        chain::ChainUid,
        cross_chain_user::CrossChainUser,
        error::ContractError,
        msgs::{
            hook::MetaReceive,
            router::{
                ManageRouterState, RegisterFactoryChainCosmos, RegisterFactoryChainEvm,
                RegisterFactoryChainNative, RegisterFactoryChainType,
            },
        },
        swap::NextSwapPair,
        token::{Token, TokenType, TokenWithDenom},
    };
    use euclid_ibc::router_ibc::{RouterCrossChainExecuteMsg, RouterCrossChainSwapExecuteMsg};

    use crate::{
        contract::execute,
        state::{
            ADMIN, FEE_STATE, LOCKED_CHAINS, META_TRANSACTION_CONTRACT, RELAYER_CONTRACT,
            RELEASE_FEES, STATE,
        },
        testing::{
            fixtures::initialized,
            helpers::{seed_chain1_native, MockDeps},
        },
    };
    use euclid::msgs::router::ExecuteMsg;
    use rstest::*;

    // -----------------------------------------------------------------------
    // ManageRouterState: auth guard (covers all variants uniformly)
    // -----------------------------------------------------------------------
    #[rstest]
    #[case::lock_state(ManageRouterState::LockState { locked: true })]
    #[case::vlp_code_id(ManageRouterState::Vlp { vlp_code_id: Some(99), stable_vlp_code_id: None, concentrated_vlp_code_id: None })]
    #[case::relayer_contract(ManageRouterState::RelayerContract { relayer_contract: Addr::unchecked("x") })]
    #[case::meta_transaction_contract(ManageRouterState::MetaTransactionContract { meta_transaction_contract: Addr::unchecked("x") })]
    #[case::update_fee_state(ManageRouterState::UpdateFeeState { release_fee_recipient: None, default_fee_recipient: None })]
    fn test_manage_router_state_rejects_non_admin(
        mut initialized: MockDeps,
        #[case] variant: ManageRouterState,
    ) {
        use cosmwasm_std::testing::{message_info, mock_env};
        use euclid::error::ContractError;

        let non_admin = initialized.api.addr_make("non_admin");
        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&non_admin, &[]),
            ExecuteMsg::ManageRouterState(variant),
        );
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});
    }
    // -----------------------------------------------------------------------
    // ManageRouterState: happy paths
    // -----------------------------------------------------------------------
    #[rstest]
    fn test_manage_router_state_lock_state(mut initialized: MockDeps) {
        let env = mock_env();
        let creator = initialized.api.addr_make("creator");
        let info = message_info(&creator, &[]);

        execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::LockState { locked: true }),
        )
        .unwrap();
        assert!(STATE.load(initialized.as_ref().storage).unwrap().locked);

        execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::LockState { locked: false }),
        )
        .unwrap();
        assert!(!STATE.load(initialized.as_ref().storage).unwrap().locked);
    }

    #[rstest]
    fn test_contract_locked_blocks_execute(mut initialized: MockDeps) {
        let env = mock_env();
        let creator = initialized.api.addr_make("creator");
        let info = message_info(&creator, &[]);

        execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::LockState { locked: true }),
        )
        .unwrap();

        let res = execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::RegisterFactory {
                chain_uid: ChainUid::create("chain1".to_string()).unwrap(),
                chain_info: RegisterFactoryChainType::Native(RegisterFactoryChainNative {
                    factory_address: "factory".to_string(),
                    factory_chain_id: "chain1".to_string(),
                }),
            },
        );
        assert_eq!(res.unwrap_err(), ContractError::ContractLocked {});

        // ManageRouterState still works while locked
        assert!(execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::LockState { locked: false }),
        )
        .is_ok());
    }

    #[rstest]
    fn test_manage_router_state_vlp_code_id(mut initialized: MockDeps) {
        let env = mock_env();
        let creator = initialized.api.addr_make("creator");
        let info = message_info(&creator, &[]);

        execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::Vlp {
                vlp_code_id: Some(99),
                stable_vlp_code_id: None,
                concentrated_vlp_code_id: None,
            }),
        )
        .unwrap();
        let state = STATE.load(initialized.as_ref().storage).unwrap();
        assert_eq!(state.constant_product_vlp_code_id, 99);
        assert_eq!(state.stable_vlp_code_id, 3); // unchanged

        execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::Vlp {
                vlp_code_id: None,
                stable_vlp_code_id: Some(77),
                concentrated_vlp_code_id: None,
            }),
        )
        .unwrap();
        let state = STATE.load(initialized.as_ref().storage).unwrap();
        assert_eq!(state.constant_product_vlp_code_id, 99); // unchanged
        assert_eq!(state.stable_vlp_code_id, 77);
    }

    #[rstest]
    fn test_manage_router_state_relayer_contract(mut initialized: MockDeps) {
        let env = mock_env();
        let creator = initialized.api.addr_make("creator");
        let new_relayer = initialized.api.addr_make("new_relayer");
        let info = message_info(&creator, &[]);

        execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::RelayerContract {
                relayer_contract: new_relayer.clone(),
            }),
        )
        .unwrap();
        assert_eq!(
            RELAYER_CONTRACT.load(initialized.as_ref().storage).unwrap(),
            new_relayer
        );
    }

    #[rstest]
    fn test_manage_router_state_meta_transaction_contract(mut initialized: MockDeps) {
        let env = mock_env();
        let creator = initialized.api.addr_make("creator");
        let meta_tx = initialized.api.addr_make("meta_tx_contract");
        let info = message_info(&creator, &[]);

        let res = execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::MetaTransactionContract {
                meta_transaction_contract: meta_tx.clone(),
            }),
        )
        .unwrap();
        assert_eq!(res.attributes[0].value, "update_meta_transaction_contract");
        assert_eq!(res.attributes[1].value, meta_tx.to_string());
        assert_eq!(
            META_TRANSACTION_CONTRACT
                .load(initialized.as_ref().storage)
                .unwrap(),
            meta_tx
        );
    }

    #[rstest]
    fn test_manage_router_state_update_fee_state(mut initialized: MockDeps) {
        let env = mock_env();
        let creator = initialized.api.addr_make("creator");
        let new_recipient = initialized.api.addr_make("new_recipient");
        let info = message_info(&creator, &[]);

        execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::UpdateFeeState {
                release_fee_recipient: Some(new_recipient.clone()),
                default_fee_recipient: None,
            }),
        )
        .unwrap();
        assert_eq!(
            FEE_STATE
                .load(initialized.as_ref().storage)
                .unwrap()
                .release_fee_recipient,
            new_recipient
        );
    }

    #[rstest]
    fn test_manage_router_state_update_release_fee(mut initialized: MockDeps) {
        let env = mock_env();
        let creator = initialized.api.addr_make("creator");
        let info = message_info(&creator, &[]);

        let token = Token::create("usdc".to_string()).unwrap();
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();

        execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::UpdateReleaseFee {
                token: token.clone(),
                chain_uid: chain_uid.clone(),
                release_fee: Uint256::from(50u128),
            }),
        )
        .unwrap();
        assert_eq!(
            RELEASE_FEES
                .load(initialized.as_ref().storage, (token, chain_uid))
                .unwrap(),
            Uint256::from(50u128)
        );
    }

    #[rstest]
    fn test_manage_router_state_update_default_release_fee(mut initialized: MockDeps) {
        let env = mock_env();
        let creator = initialized.api.addr_make("creator");
        let info = message_info(&creator, &[]);

        let res = execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::UpdateDefaultReleaseFee {
                default_release_fee: Uint256::from(100u128),
            }),
        )
        .unwrap();
        assert_eq!(res.attributes[0].value, "update_default_release_fee");
        assert_eq!(res.attributes[1].value, "100");
    }

    #[rstest]
    fn test_manage_router_state_lock_unlock_chain(mut initialized: MockDeps) {
        let env = mock_env();
        let creator = initialized.api.addr_make("creator");
        let non_admin = initialized.api.addr_make("non_admin");
        let info = message_info(&creator, &[]);

        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();

        let res = execute(
            initialized.as_mut(),
            env.clone(),
            message_info(&non_admin, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::LockChain {
                chain: chain_uid.clone(),
            }),
        );
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});

        execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::LockChain {
                chain: chain_uid.clone(),
            }),
        )
        .unwrap();
        assert!(LOCKED_CHAINS
            .load(initialized.as_ref().storage)
            .unwrap()
            .contains(&chain_uid));

        let res = execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::LockChain {
                chain: chain_uid.clone(),
            }),
        );
        assert_eq!(res.unwrap_err(), ContractError::new("Chain already locked"));

        let res = execute(
            initialized.as_mut(),
            env.clone(),
            message_info(&non_admin, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::UnlockChain {
                chain: chain_uid.clone(),
            }),
        );
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});

        execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::UnlockChain {
                chain: chain_uid.clone(),
            }),
        )
        .unwrap();
        assert!(!LOCKED_CHAINS
            .load(initialized.as_ref().storage)
            .unwrap()
            .contains(&chain_uid));

        let res = execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::UnlockChain {
                chain: chain_uid.clone(),
            }),
        );
        assert_eq!(
            res.unwrap_err(),
            ContractError::new("Chain already unlocked")
        );
    }

    #[rstest]
    fn test_manage_router_state_update_chain_timeout(mut initialized: MockDeps) {
        let env = mock_env();
        let creator = initialized.api.addr_make("creator");
        let info = message_info(&creator, &[]);

        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let res = execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::UpdateChainTimeout {
                chain_uid: chain_uid.clone(),
                timeout: 300,
            }),
        )
        .unwrap();
        assert_eq!(res.attributes[0].value, "update_chain_timeout");
        assert_eq!(res.attributes[2].value, "300");
    }

    #[rstest]
    fn test_manage_router_state_update_admins(mut initialized: MockDeps) {
        let env = mock_env();
        let creator = initialized.api.addr_make("creator");
        let new_general_admin = initialized.api.addr_make("new_general_admin");
        let non_admin = initialized.api.addr_make("non_admin");
        let info = message_info(&creator, &[]);

        // Admins returns UnauthorizedWithMsg (not Unauthorized) — tested here separately
        let res = execute(
            initialized.as_mut(),
            env.clone(),
            message_info(&non_admin, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::Admins {
                admin_type: AdminType::GeneralAdmin,
                admin: new_general_admin.to_string(),
            }),
        );
        assert!(res.is_err());

        execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::Admins {
                admin_type: AdminType::GeneralAdmin,
                admin: new_general_admin.to_string(),
            }),
        )
        .unwrap();
        assert_eq!(
            ADMIN
                .load(initialized.as_ref().storage)
                .unwrap()
                .general_admin,
            new_general_admin
        );
    }
    // -----------------------------------------------------------------------
    // Role separation: migration_admin / fee_admin / general_admin
    // -----------------------------------------------------------------------

    /// After changing general_admin, the new address can lock state but the old
    /// one is rejected; the old address still controls migration_admin operations.
    #[rstest]
    fn test_general_admin_change_locks_out_old_admin(mut initialized: MockDeps) {
        let creator = initialized.api.addr_make("creator");
        let new_general = initialized.api.addr_make("new_general");

        // creator delegates general_admin role.
        execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::Admins {
                admin_type: AdminType::GeneralAdmin,
                admin: new_general.to_string(),
            }),
        )
        .unwrap();

        // Old general_admin (creator) can no longer lock state.
        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::LockState { locked: true }),
        );
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});

        // New general_admin can lock state.
        execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&new_general, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::LockState { locked: true }),
        )
        .unwrap();
        assert!(STATE.load(initialized.as_ref().storage).unwrap().locked);
    }

    /// VLP code-id updates require migration_admin — general_admin alone is not enough.
    #[rstest]
    fn test_vlp_update_requires_migration_admin(mut initialized: MockDeps) {
        let creator = initialized.api.addr_make("creator");
        let new_general = initialized.api.addr_make("new_general");

        // Give general_admin role to a different address; creator keeps migration_admin.
        execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::Admins {
                admin_type: AdminType::GeneralAdmin,
                admin: new_general.to_string(),
            }),
        )
        .unwrap();

        // new_general cannot update VLP code IDs.
        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&new_general, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::Vlp {
                vlp_code_id: Some(99),
                stable_vlp_code_id: None,
                concentrated_vlp_code_id: None,
            }),
        );
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});

        // creator (still migration_admin) succeeds.
        execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::Vlp {
                vlp_code_id: Some(99),
                stable_vlp_code_id: None,
                concentrated_vlp_code_id: None,
            }),
        )
        .unwrap();
        assert_eq!(
            STATE
                .load(initialized.as_ref().storage)
                .unwrap()
                .constant_product_vlp_code_id,
            99
        );
    }

    /// Fee operations require fee_admin; general_admin (when distinct) is rejected.
    #[rstest]
    fn test_fee_update_requires_fee_admin(mut initialized: MockDeps) {
        let creator = initialized.api.addr_make("creator");
        let dedicated_fee_admin = initialized.api.addr_make("dedicated_fee_admin");
        let new_general = initialized.api.addr_make("new_general");

        // Separate fee_admin (creator is currently fee_admin so can transfer it).
        execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::Admins {
                admin_type: AdminType::FeeAdmin,
                admin: dedicated_fee_admin.to_string(),
            }),
        )
        .unwrap();
        // Also change general_admin so creator is no longer general_admin.
        execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::Admins {
                admin_type: AdminType::GeneralAdmin,
                admin: new_general.to_string(),
            }),
        )
        .unwrap();

        let dummy_addr = initialized.api.addr_make("r");

        // general_admin (new_general) cannot update fee state.
        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&new_general, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::UpdateFeeState {
                release_fee_recipient: Some(dummy_addr),
                default_fee_recipient: None,
            }),
        );
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});

        // dedicated_fee_admin succeeds.
        execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&dedicated_fee_admin, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::UpdateReleaseFee {
                token: Token::create("usdc".to_string()).unwrap(),
                chain_uid: ChainUid::create("chain1".to_string()).unwrap(),
                release_fee: Uint256::from(20u128),
            }),
        )
        .unwrap();
    }

    /// UpdateDefaultReleaseFee and UpdateChainTimeout carry no auth guard —
    /// any caller can invoke them.
    #[rstest]
    fn test_unguarded_manage_variants_accept_any_caller(mut initialized: MockDeps) {
        let random = initialized.api.addr_make("random_caller");
        let creator = initialized.api.addr_make("creator");

        // UpdateDefaultReleaseFee: no require guard in the contract.
        let err = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&random, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::UpdateDefaultReleaseFee {
                default_release_fee: Uint256::from(42u128),
            }),
        )
        .unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});

        execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::UpdateDefaultReleaseFee {
                default_release_fee: Uint256::from(42u128),
            }),
        )
        .unwrap();

        // UpdateChainTimeout: no auth guard either.
        let err = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&random, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::UpdateChainTimeout {
                chain_uid: ChainUid::create("chain1".to_string()).unwrap(),
                timeout: 600,
            }),
        )
        .unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});

        execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::UpdateChainTimeout {
                chain_uid: ChainUid::create("chain1".to_string()).unwrap(),
                timeout: 600,
            }),
        )
        .unwrap();
    }

    // -----------------------------------------------------------------------
    // State invariant
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_state_invariant_after_manage_sequence(mut initialized: MockDeps) {
        let env = mock_env();
        let creator = initialized.api.addr_make("creator");
        let info = message_info(&creator, &[]);

        execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::Vlp {
                vlp_code_id: Some(99),
                stable_vlp_code_id: Some(88),
                concentrated_vlp_code_id: Some(77),
            }),
        )
        .unwrap();
        execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::LockState { locked: true }),
        )
        .unwrap();

        let state = STATE.load(initialized.as_ref().storage).unwrap();
        assert_eq!(state.constant_product_vlp_code_id, 99);
        assert_eq!(state.stable_vlp_code_id, 88);
        assert_eq!(state.concentrated_vlp_code_id, 77);
        assert!(state.locked);

        execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::LockState { locked: false }),
        )
        .unwrap();
        assert!(!STATE.load(initialized.as_ref().storage).unwrap().locked);
    }

    // -----------------------------------------------------------------------
    // RegisterFactory: happy path
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::by_admin("creator", None)]
    #[case::by_non_admin("non-admin", Some(ContractError::Unauthorized {}))]
    fn test_execute_register_factory(
        mut initialized: MockDeps,
        #[case] sender_name: &str,
        #[case] expected_error: Option<ContractError>,
    ) {
        use cosmwasm_std::WasmMsg;
        use euclid_ibc::state::{
            NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_COUNT, NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_RANGE,
            NATIVE_CROSS_CHAIN_ORIGINAL_MSG_REPLY_QUEUE,
        };

        let chain_uid = ChainUid::create("1".to_string()).unwrap();
        let env = mock_env();
        let sender = initialized.api.addr_make(sender_name);
        let msg = ExecuteMsg::RegisterFactory {
            chain_uid: chain_uid.clone(),
            chain_info: RegisterFactoryChainType::Native(RegisterFactoryChainNative {
                factory_address: "factory".to_string(),
                factory_chain_id: "1".to_string(),
            }),
        };
        let res = execute(initialized.as_mut(), env, message_info(&sender, &[]), msg);
        match expected_error {
            Some(err) => assert_eq!(res.unwrap_err(), err),
            None => {
                use cosmwasm_std::CosmosMsg;

                use crate::state::CHAIN_UID_TO_CHAIN;

                let res = res.unwrap();
                assert_eq!(res.attributes[0].key, "method");
                assert_eq!(res.attributes[0].value, "register_factory");
                assert_eq!(res.messages.len(), 1);

                // For a Native chain the SubMsg is a WasmMsg::Execute targeting the
                // factory contract (not an IBC packet — that only appears after ack).
                match &res.messages[0].msg {
                    CosmosMsg::Wasm(WasmMsg::Execute {
                        contract_addr, msg, ..
                    }) => {
                        use cosmwasm_std::from_json;

                        assert_eq!(contract_addr, "factory");
                        // Inner payload is NativeReceiveCallback wrapping
                        // FactoryCrossChainExecuteMsg::RegisterFactory.
                        let cb: euclid::msgs::factory::msg::ExecuteMsg = from_json(msg).unwrap();
                        if let euclid::msgs::factory::msg::ExecuteMsg::NativeReceiveCallback {
                            msg: inner,
                        } = cb
                        {
                            use euclid_ibc::factory_ibc::FactoryCrossChainExecuteMsg;

                            let factory_msg: FactoryCrossChainExecuteMsg =
                                from_json(inner).unwrap();
                            // Verify chain identity; tx_id includes the bech32 sender
                            // address generated by addr_make so we assert it separately.
                            if let FactoryCrossChainExecuteMsg::RegisterFactory {
                                chain_uid: msg_chain_uid,
                                chain_type,
                                tx_id,
                            } = factory_msg
                            {
                                assert_eq!(
                                    msg_chain_uid,
                                    ChainUid::create("1".to_string()).unwrap()
                                );
                                assert_eq!(
                                    chain_type,
                                    RegisterFactoryChainType::Native(RegisterFactoryChainNative {
                                        factory_address: "factory".to_string(),
                                        factory_chain_id: "1".to_string(),
                                    })
                                );
                                assert!(
                                    tx_id.starts_with("vsl:"),
                                    "tx_id should be VSL-prefixed, got {tx_id}"
                                );
                            } else {
                                panic!("expected RegisterFactory inner message");
                            }
                        } else {
                            panic!("expected NativeReceiveCallback inner message");
                        }
                    }
                    other => panic!("expected WasmMsg::Execute to factory, got {other:?}"),
                }

                // CHAIN_UID_TO_CHAIN is only written after the ack arrives, not here.
                assert!(
                    !CHAIN_UID_TO_CHAIN.has(initialized.as_ref().storage, chain_uid),
                    "CHAIN_UID_TO_CHAIN must not be written before ack"
                );

                // The native IBC queue entry must be enqueued.
                let queue_count = NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_COUNT
                    .load(initialized.as_ref().storage)
                    .unwrap();
                assert_eq!(
                    queue_count,
                    NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_RANGE.0 + 1,
                    "native queue count should be incremented by 1"
                );
                assert!(
                    NATIVE_CROSS_CHAIN_ORIGINAL_MSG_REPLY_QUEUE.has(
                        initialized.as_ref().storage,
                        NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_RANGE.0
                    ),
                    "pending packet should be enqueued at range start"
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // RegisterFactory: validation errors (table-driven)
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::vsl_uid_rejected(
        ChainUid::vsl_chain_uid().unwrap(),
        RegisterFactoryChainType::Native(RegisterFactoryChainNative {
            factory_address: "factory".to_string(),
            factory_chain_id: "vsl".to_string(),
        }),
        ContractError::new("Cannot use VSL chain uid"),
    )]
    #[case::cosmos_uppercase_address(
        ChainUid::create("cosmos1".to_string()).unwrap(),
        RegisterFactoryChainType::Cosmos(RegisterFactoryChainCosmos {
            factory_address: "UPPERCASE_FACTORY".to_string(),
            factory_chain_id: "cosmos1".to_string(),
        }),
        ContractError::new("Factory address must be lowercase"),
    )]
    #[case::evm_uppercase_address(
        ChainUid::create("evm1".to_string()).unwrap(),
        RegisterFactoryChainType::Evm(RegisterFactoryChainEvm {
            factory_address: "0xUpperCase".to_string(),
            factory_chain_id: "evm1".to_string(),
        }),
        ContractError::new("Factory address must be lowercase"),
    )]
    fn test_register_factory_rejects_invalid_input(
        mut initialized: MockDeps,
        #[case] chain_uid: ChainUid,
        #[case] chain_info: RegisterFactoryChainType,
        #[case] expected_error: ContractError,
    ) {
        let creator = initialized.api.addr_make("creator");
        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::RegisterFactory {
                chain_uid,
                chain_info,
            },
        );
        assert_eq!(res.unwrap_err(), expected_error);
    }

    #[rstest]
    fn test_register_factory_duplicate_chain_rejected(mut initialized: MockDeps) {
        // CHAIN_UID_TO_CHAIN is only written by the ack handler, not by the execute
        // itself.  Seed it directly to simulate a chain that already completed
        // registration, then verify execute rejects the duplicate.
        seed_chain1_native(&mut initialized);

        let creator = initialized.api.addr_make("creator");
        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::RegisterFactory {
                chain_uid: ChainUid::create("chain1".to_string()).unwrap(),
                chain_info: RegisterFactoryChainType::Native(RegisterFactoryChainNative {
                    factory_address: "factory1".to_string(),
                    factory_chain_id: "chain1".to_string(),
                }),
            },
        );
        assert_eq!(
            res.unwrap_err(),
            ContractError::new("Factory already exists")
        );
    }

    // -----------------------------------------------------------------------
    // MetaReceive: dispatch business logic
    // -----------------------------------------------------------------------

    fn meta_receive_msg(
        verified_sender: CrossChainUser,
        inner: &RouterCrossChainExecuteMsg,
    ) -> ExecuteMsg {
        let call_data = String::from_utf8(to_json_binary(inner).unwrap().to_vec()).unwrap();
        ExecuteMsg::MetaReceive(MetaReceive {
            verified_sender,
            call_data,
        })
    }

    fn seed_meta_tx_contract(deps: &mut MockDeps) -> Addr {
        let meta_tx = deps.api.addr_make("meta_tx");
        META_TRANSACTION_CONTRACT
            .save(deps.as_mut().storage, &meta_tx)
            .unwrap();
        meta_tx
    }

    /// MetaReceive rejects message types other than Swap and TransferVoucher.
    #[rstest]
    fn test_meta_receive_unsupported_msg_type_fails(mut initialized: MockDeps) {
        let meta_tx = seed_meta_tx_contract(&mut initialized);
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let sender = CrossChainUser::new(chain_uid.clone(), "user".to_string());

        let inner = RouterCrossChainExecuteMsg::RegisterDenom {
            sender: sender.clone(),
            token: TokenWithDenom {
                token: Token::create("usdc".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uusdc".to_string(),
                    decimals: Some(6),
                },
            },
            tx_id: "tx1".to_string(),
        };

        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&meta_tx, &[]),
            meta_receive_msg(sender, &inner),
        );
        assert!(matches!(res.unwrap_err(), ContractError::Generic { .. }));
    }

    /// MetaReceive for Swap requires the inner asset_in to be a voucher token.
    #[rstest]
    fn test_meta_receive_swap_requires_voucher_asset_in(mut initialized: MockDeps) {
        let meta_tx = seed_meta_tx_contract(&mut initialized);
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let sender = CrossChainUser::new(chain_uid.clone(), "user".to_string());
        let token_a = Token::create("aaa".to_string()).unwrap();
        let token_b = Token::create("bbb".to_string()).unwrap();

        let swap = RouterCrossChainSwapExecuteMsg {
            sender: sender.clone(),
            // Native denom — not a voucher.
            asset_in: TokenWithDenom {
                token: token_a.clone(),
                token_type: TokenType::Native {
                    denom: "uaaa".to_string(),
                    decimals: Some(6),
                },
            },
            amount_in: Uint256::from(100u128),
            asset_out: token_b,
            min_amount_out: Uint256::from(80u128),
            swaps: vec![NextSwapPair {
                token_in: token_a,
                token_out: Token::create("bbb".to_string()).unwrap(),
                test_fail: None,
                pool_key: None,
            }],
            recipients: vec![],
            partner_fee_amount: Uint256::zero(),
            partner_fee_recipient: sender.clone(),
            tx_id: "tx_meta_swap".to_string(),
        };

        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&meta_tx, &[]),
            meta_receive_msg(sender, &RouterCrossChainExecuteMsg::Swap(swap)),
        );
        assert_eq!(
            res.unwrap_err(),
            ContractError::new("Asset IN does not match asset OUT")
        );
    }

    /// MetaReceive for Swap rejects when verified_sender address differs from swap.sender.
    #[rstest]
    fn test_meta_receive_swap_sender_address_mismatch_fails(mut initialized: MockDeps) {
        let meta_tx = seed_meta_tx_contract(&mut initialized);
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let real_sender = CrossChainUser::new(chain_uid.clone(), "real_user".to_string());
        let impersonated = CrossChainUser::new(chain_uid.clone(), "victim".to_string());
        let token_a = Token::create("aaa".to_string()).unwrap();
        let token_b = Token::create("bbb".to_string()).unwrap();

        let swap = RouterCrossChainSwapExecuteMsg {
            // Swap claims to come from "victim".
            sender: impersonated,
            asset_in: TokenWithDenom {
                token: token_a.clone(),
                token_type: TokenType::Voucher {},
            },
            amount_in: Uint256::from(100u128),
            asset_out: token_b,
            min_amount_out: Uint256::from(80u128),
            swaps: vec![NextSwapPair {
                token_in: token_a,
                token_out: Token::create("bbb".to_string()).unwrap(),
                test_fail: None,
                pool_key: None,
            }],
            recipients: vec![],
            partner_fee_amount: Uint256::zero(),
            partner_fee_recipient: real_sender.clone(),
            tx_id: "tx_mismatch".to_string(),
        };

        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&meta_tx, &[]),
            // verified_sender is "real_user" but swap.sender is "victim".
            meta_receive_msg(real_sender, &RouterCrossChainExecuteMsg::Swap(swap)),
        );
        assert_eq!(
            res.unwrap_err(),
            ContractError::new("Sender address mismatch")
        );
    } // -----------------------------------------------------------------------
      // MetaReceive: access control
      // -----------------------------------------------------------------------

    #[rstest]
    fn test_meta_receive_rejects_unauthorized_caller(mut initialized: MockDeps) {
        let meta_tx = initialized.api.addr_make("meta_tx_contract");
        META_TRANSACTION_CONTRACT
            .save(initialized.as_mut().storage, &meta_tx)
            .unwrap();
        let attacker = Addr::unchecked("attacker");

        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&attacker, &[]),
            ExecuteMsg::MetaReceive(MetaReceive {
                verified_sender: euclid::cross_chain_user::CrossChainUser::new(
                    ChainUid::vsl_chain_uid().unwrap(),
                    "user".to_string(),
                ),
                call_data: "{}".to_string(),
            }),
        );
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});
    }

    #[rstest]
    fn test_meta_receive_fails_when_contract_not_set(mut initialized: MockDeps) {
        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&Addr::unchecked("anyone"), &[]),
            ExecuteMsg::MetaReceive(MetaReceive {
                verified_sender: euclid::cross_chain_user::CrossChainUser::new(
                    ChainUid::vsl_chain_uid().unwrap(),
                    "user".to_string(),
                ),
                call_data: "{}".to_string(),
            }),
        );
        assert!(res.is_err());
    }
}
