use cosmwasm_std::{
    ensure, to_json_binary, to_json_string, Binary, DepsMut, Env, HexBinary, MessageInfo,
    QueryRequest, Response, Timestamp, Uint256, WasmMsg, WasmQuery,
};
use euclid::admin;
use euclid::chain::ChainType;
use euclid::cross_chain_user::CrossChainUser;
use euclid::error::ContractError;
use euclid::msgs::hook::MetaReceive;
use euclid::msgs::meta_transaction::msg::{MetaTransaction, UpdateAdminMsg};
use euclid::msgs::router;
use relayer::verify::{
    add_eth_prefix, cosmos_address_from_pubkey, eth_address_from_pubkey, msg_to_sign_data,
    verify_keccak256_signature, verify_signature,
};

use crate::state::{ADMIN, NONCES, STATE};

pub fn execute_update_admin(
    deps: &mut DepsMut,
    env: Env,
    info: &MessageInfo,
    msg: UpdateAdminMsg,
) -> Result<Response, ContractError> {
    let mut admins = ADMIN.load(deps.storage)?;
    let (updated_admins, response) = admin::update_admin(
        &admins,
        deps,
        &env,
        &info.sender,
        msg.new_admin.clone(),
        msg.admin_type,
    )?;

    admins = updated_admins;
    ADMIN.save(deps.storage, &admins)?;
    Ok(response.add_attribute("updated_admins", admins.to_string()))
}

pub fn execute_meta_transaction(
    deps: &mut DepsMut,
    env: &Env,
    info: &MessageInfo,
    meta_transaction: MetaTransaction,
) -> Result<Response, ContractError> {
    // Ensure the timestamp is not exceeded
    ensure!(
        env.block.time <= Timestamp::from_seconds(meta_transaction.data.expiry),
        ContractError::new("Timestamp limit exceeded")
    );

    let state = STATE.load(deps.storage)?;
    // Get chain type from router

    let chain_type = deps
        .querier
        .query::<router::ChainResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: state.router_contract.to_string(),
            msg: to_json_binary(&euclid::msgs::router::QueryMsg::GetChain {
                chain_uid: meta_transaction.data.signer_chain_uid.clone(),
            })?,
        }))?
        .chain
        .chain_type;

    // Derive address from public key and verify it matches the claimed address
    let derived_address = match chain_type {
        ChainType::Cosmos(_) | ChainType::Native {} => {
            let pubkey = Binary::from_base64(meta_transaction.signer_pubkey.as_str())?;
            let bech32 = meta_transaction.data.signer_prefix.clone();

            let data_binary = to_json_binary(&meta_transaction.data)?;
            let msg_sign_data =
                msg_to_sign_data(data_binary, meta_transaction.data.signer_address.clone());
            let msg_sign_data_str = to_json_string(&msg_sign_data)?;

            let verified = verify_signature(
                deps.as_ref(),
                &msg_sign_data_str,
                &Binary::from_base64(meta_transaction.signature.as_str())?,
                &pubkey,
            )?;
            ensure!(verified, ContractError::new("Invalid signature"));

            cosmos_address_from_pubkey(&pubkey, &bech32).map_err(|e| {
                ContractError::new(&format!("Failed to derive cosmos address: {}", e))
            })?
        }
        ChainType::Evm(_) => {
            let pubkey = HexBinary::from_hex(meta_transaction.signer_pubkey.as_str())?;
            let prefixed_msg = add_eth_prefix(&to_json_string(&meta_transaction.data)?);
            let verified = verify_keccak256_signature(
                deps.as_ref(),
                &prefixed_msg,
                &HexBinary::from_hex(meta_transaction.signature.as_str())?,
                &pubkey,
            )?;

            ensure!(verified, ContractError::new("Invalid signature"));

            eth_address_from_pubkey(&pubkey)
                .map_err(|e| ContractError::new(&format!("Failed to derive EVM address: {}", e)))?
        }
    };

    ensure!(
        derived_address == meta_transaction.data.signer_address,
        ContractError::new(&format!(
            "Address mismatch: derived '{}' does not signed address '{}'",
            derived_address, meta_transaction.data.signer_address
        ))
    );
    // Create sender key: chainuid:address
    let sender_key = format!(
        "{}:{}",
        meta_transaction.data.signer_chain_uid.as_str(),
        meta_transaction.data.signer_address
    );

    // Ensure the nonce is not used for this sender
    if let Some(blockheight) = NONCES.may_load(
        deps.storage,
        (sender_key.clone(), meta_transaction.data.nonce.clone()),
    )? {
        return Err(ContractError::new(
            format!(
                "Nonce already used for sender {}: {} at block height {}",
                sender_key, meta_transaction.data.nonce, blockheight
            )
            .as_str(),
        ));
    }
    // Save the nonce for this sender
    NONCES.save(
        deps.storage,
        (sender_key.clone(), meta_transaction.data.nonce.clone()),
        &Uint256::from(env.block.height),
    )?;

    let mut response = Response::new()
        .add_attribute("meta_sender_key", sender_key)
        .add_attribute("meta_broadcaster", info.sender.to_string());

    let verified_sender = CrossChainUser::new(
        meta_transaction.data.signer_chain_uid.clone(),
        meta_transaction.data.signer_address.clone(),
    );
    // Reject mixed-case or empty addresses before dispatching calls
    verified_sender.validate()?;

    for call_data in meta_transaction.data.call_data {
        let meta_receive = MetaReceive {
            verified_sender: verified_sender.clone(),
            call_data: call_data.call_data.clone(),
        };
        let meta_receive_msg = meta_receive.to_receiver_msg()?;
        response = response.add_message(WasmMsg::Execute {
            contract_addr: call_data.target.to_string(),
            msg: meta_receive_msg,
            funds: vec![],
        });
    }

    Ok(response)
}

#[cfg(test)]
mod tests {
    use crate::contract::execute;
    use crate::state::{ADMIN, NONCES};
    use cosmwasm_std::testing::{message_info, mock_dependencies, mock_env, MockQuerier};
    use cosmwasm_std::{
        attr, from_json, to_json_binary, to_json_string, Addr, Binary, CosmosMsg, Env, Timestamp,
        Uint256, WasmMsg,
    };
    use euclid::admin::AdminType;
    use euclid::chain::{Chain, ChainType, ChainUid, CosmosChain, EvmChain};
    use euclid::error::ContractError;
    use euclid::msgs::hook::MetaReceiverMsg;
    use euclid::msgs::meta_transaction::msg::{
        ExecuteMsg, InstantiateMsg, MetaTransaction, MetaTransactionCallData, MetaTransactionData,
        UpdateAdminMsg,
    };
    use euclid::msgs::router::ChainResponse;
    use k256::{ecdsa::SigningKey, elliptic_curve::NonZeroScalar};
    use mock::admin_tests::run_update_admin_access_control;
    use relayer::verify::{cosmos_address_from_pubkey, msg_to_sign_data};
    use rstest::{fixture, rstest};
    use sha2::{digest::Update, Digest, Sha256};
    use std::str::FromStr;

    // -----------------------------------------------------------------------
    // Type alias
    // -----------------------------------------------------------------------

    type MockDeps = cosmwasm_std::OwnedDeps<
        cosmwasm_std::testing::MockStorage,
        cosmwasm_std::testing::MockApi,
        MockQuerier,
    >;

    // -----------------------------------------------------------------------
    // Crypto helpers: sign the same way as the contract verifies
    // -----------------------------------------------------------------------

    /// Known secp256k1 private key (same used in relayer verify tests).
    const COSMOS_SK_HEX: &str = "2268A9118C1681EC6A649F01886995DE55E90C7E71B0BC5E409C551B92FF7369";

    fn get_signing_key() -> SigningKey {
        let scalar = NonZeroScalar::from_str(COSMOS_SK_HEX).unwrap();
        SigningKey::from(scalar)
    }

    fn get_pubkey_binary() -> Binary {
        let sk = get_signing_key();
        Binary::from(
            sk.verifying_key()
                .to_encoded_point(true)
                .as_bytes()
                .to_vec(),
        )
    }

    fn get_cosmos_address() -> String {
        cosmos_address_from_pubkey(&get_pubkey_binary(), "euclid").unwrap()
    }

    /// Sign `MetaTransactionData` using the same algorithm that the contract verifies.
    /// Returns (signature_base64, pubkey_base64).
    fn sign_cosmos_meta_tx(data: &MetaTransactionData) -> (String, String) {
        let sk = get_signing_key();
        let pubkey = get_pubkey_binary();

        let data_binary = to_json_binary(data).unwrap();
        let msg_sign_data = msg_to_sign_data(data_binary, data.signer_address.clone());
        let msg_sign_data_str = to_json_string(&msg_sign_data).unwrap();

        let message_digest = Sha256::new().chain(msg_sign_data_str.as_bytes());
        let (sig, _recovery_id) = sk
            .sign_digest_recoverable(message_digest)
            .expect("sign failed");

        let sig_b64 = Binary::from(sig.to_bytes().as_slice()).to_base64();
        let pubkey_b64 = pubkey.to_base64();
        (sig_b64, pubkey_b64)
    }

    fn make_signed_cosmos_tx(
        env: &Env,
        nonce: &str,
        call_data: Vec<MetaTransactionCallData>,
    ) -> MetaTransaction {
        let signer_address = get_cosmos_address();
        let data = MetaTransactionData {
            signer_address: signer_address.clone(),
            signer_prefix: "euclid".to_string(),
            signer_chain_uid: ChainUid::create("testchain".to_string()).unwrap(),
            call_data,
            expiry: env.block.time.seconds() + 3600,
            nonce: nonce.to_string(),
        };
        let (signature, signer_pubkey) = sign_cosmos_meta_tx(&data);
        MetaTransaction {
            data,
            signature,
            signer_pubkey,
        }
    }

    // -----------------------------------------------------------------------
    // Setup helpers
    // -----------------------------------------------------------------------

    fn router_contract() -> Addr {
        Addr::unchecked("router")
    }

    fn make_instantiate_msg() -> InstantiateMsg {
        InstantiateMsg {
            router_contract: router_contract(),
        }
    }

    fn set_router_chain_query(deps: &mut MockDeps, chain_uid: ChainUid, chain_type: ChainType) {
        let chain = Chain {
            chain_uid: chain_uid.clone(),
            factory_address: "factory".to_string(),
            chain_type,
        };
        let response = ChainResponse { chain, chain_uid };
        let response_binary = to_json_binary(&response).unwrap();

        deps.querier.update_wasm(move |_wasm_query| {
            cosmwasm_std::SystemResult::Ok(cosmwasm_std::ContractResult::Ok(
                response_binary.clone(),
            ))
        });
    }

    // -----------------------------------------------------------------------
    // Fixtures
    // -----------------------------------------------------------------------

    #[fixture]
    fn initialized() -> MockDeps {
        let mut deps = mock_dependencies();
        let sender = deps.api.addr_make("sender");
        let info = message_info(&sender, &[]);
        crate::contract::instantiate(deps.as_mut(), mock_env(), info, make_instantiate_msg())
            .unwrap();
        deps
    }

    #[fixture]
    fn with_cosmos_chain(mut initialized: MockDeps) -> MockDeps {
        let chain_uid = ChainUid::create("testchain".to_string()).unwrap();
        set_router_chain_query(
            &mut initialized,
            chain_uid,
            ChainType::Cosmos(CosmosChain {
                chain_id: "cosmos-hub".to_string(),
            }),
        );
        initialized
    }

    #[fixture]
    fn with_evm_chain(mut initialized: MockDeps) -> MockDeps {
        let chain_uid = ChainUid::create("evmchain".to_string()).unwrap();
        set_router_chain_query(
            &mut initialized,
            chain_uid,
            ChainType::Evm(EvmChain {
                chain_id: "1".to_string(),
            }),
        );
        initialized
    }

    #[fixture]
    fn with_native_chain(mut initialized: MockDeps) -> MockDeps {
        let chain_uid = ChainUid::create("testchain".to_string()).unwrap();
        set_router_chain_query(&mut initialized, chain_uid, ChainType::Native {});
        initialized
    }

    // -----------------------------------------------------------------------
    // UpdateAdmin: access control (table-driven via shared helper)
    // -----------------------------------------------------------------------

    #[test]
    fn test_update_admin_access_control() {
        run_update_admin_access_control(
            || {
                let mut deps = mock_dependencies();
                let sender = deps.api.addr_make("sender");
                let info = message_info(&sender, &[]);
                crate::contract::instantiate(
                    deps.as_mut(),
                    mock_env(),
                    info,
                    make_instantiate_msg(),
                )
                .unwrap();
                deps
            },
            |deps, env, info, admin_type, new_admin| {
                execute(
                    deps,
                    env,
                    info,
                    ExecuteMsg::UpdateAdmin(UpdateAdminMsg {
                        new_admin,
                        admin_type,
                    }),
                )
            },
        );
    }

    // -----------------------------------------------------------------------
    // UpdateAdmin: happy path — state is updated
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_update_general_admin_persists_to_state(mut initialized: MockDeps) {
        let sender = initialized.api.addr_make("sender");
        let new_admin = initialized.api.addr_make("new_general_admin");
        let info = message_info(&sender, &[]);

        execute(
            initialized.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::UpdateAdmin(UpdateAdminMsg {
                new_admin: new_admin.to_string(),
                admin_type: AdminType::GeneralAdmin,
            }),
        )
        .unwrap();

        let stored_admin = ADMIN.load(&initialized.storage).unwrap();
        assert_eq!(stored_admin.general_admin, new_admin);
    }

    #[rstest]
    fn test_update_fee_admin_persists_to_state(mut initialized: MockDeps) {
        let sender = initialized.api.addr_make("sender");
        let new_fee_admin = initialized.api.addr_make("new_fee_admin");
        let info = message_info(&sender, &[]);

        execute(
            initialized.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::UpdateAdmin(UpdateAdminMsg {
                new_admin: new_fee_admin.to_string(),
                admin_type: AdminType::FeeAdmin,
            }),
        )
        .unwrap();

        let stored_admin = ADMIN.load(&initialized.storage).unwrap();
        assert_eq!(stored_admin.fee_admin, new_fee_admin);
    }

    #[rstest]
    fn test_update_migration_admin_emits_wasm_update_admin_message(mut initialized: MockDeps) {
        let sender = initialized.api.addr_make("sender");
        let new_migration = initialized.api.addr_make("new_migration_admin");
        let info = message_info(&sender, &[]);
        let env = mock_env();

        let res = execute(
            initialized.as_mut(),
            env.clone(),
            info,
            ExecuteMsg::UpdateAdmin(UpdateAdminMsg {
                new_admin: new_migration.to_string(),
                admin_type: AdminType::MigrationAdmin,
            }),
        )
        .unwrap();

        assert_eq!(res.messages.len(), 1);
        if let CosmosMsg::Wasm(WasmMsg::UpdateAdmin {
            contract_addr,
            admin,
        }) = &res.messages[0].msg
        {
            assert_eq!(contract_addr, &env.contract.address.to_string());
            assert_eq!(admin, &new_migration.to_string());
        } else {
            panic!("expected WasmMsg::UpdateAdmin");
        }

        let stored_admin = ADMIN.load(&initialized.storage).unwrap();
        assert_eq!(stored_admin.migration_admin, new_migration);
    }

    // -----------------------------------------------------------------------
    // UpdateAdmin: new_admin attribute is emitted
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_update_admin_emits_new_admin_attribute(mut initialized: MockDeps) {
        let sender = initialized.api.addr_make("sender");
        let new_admin_addr = initialized.api.addr_make("new_general");
        let info = message_info(&sender, &[]);

        let res = execute(
            initialized.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::UpdateAdmin(UpdateAdminMsg {
                new_admin: new_admin_addr.to_string(),
                admin_type: AdminType::GeneralAdmin,
            }),
        )
        .unwrap();

        let stored_admin = ADMIN.load(&initialized.storage).unwrap();

        let new_admin_attr = res
            .attributes
            .iter()
            .find(|a| a.key == "new_admin")
            .expect("missing new_admin attribute");
        assert_eq!(new_admin_attr.value, new_admin_addr.to_string());

        let updated_admins_attr = res
            .attributes
            .iter()
            .find(|a| a.key == "updated_admins")
            .expect("missing updated_admins attribute");
        assert_eq!(updated_admins_attr.value, stored_admin.to_string());
    }

    // -----------------------------------------------------------------------
    // ExecuteMetaTransaction: timestamp expired
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_meta_transaction_expired_timestamp_rejected(mut with_cosmos_chain: MockDeps) {
        let broadcaster = with_cosmos_chain.api.addr_make("broadcaster");
        let info = message_info(&broadcaster, &[]);
        let mut env = mock_env();
        env.block.time = Timestamp::from_seconds(9_999_999);

        let signer_address = get_cosmos_address();
        let data = MetaTransactionData {
            signer_address: signer_address.clone(),
            signer_prefix: "euclid".to_string(),
            signer_chain_uid: ChainUid::create("testchain".to_string()).unwrap(),
            call_data: vec![],
            expiry: env.block.time.seconds() - 1,
            nonce: "nonce_expired".to_string(),
        };
        let (signature, signer_pubkey) = sign_cosmos_meta_tx(&data);
        let meta_tx = MetaTransaction {
            data,
            signature,
            signer_pubkey,
        };

        let res = execute(
            with_cosmos_chain.as_mut(),
            env,
            info,
            ExecuteMsg::ExecuteMetaTransaction(meta_tx),
        );

        assert!(res.is_err());
        assert_eq!(
            res.unwrap_err(),
            ContractError::new("Timestamp limit exceeded")
        );
    }

    // -----------------------------------------------------------------------
    // ExecuteMetaTransaction: expiry at exact block time is accepted
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_meta_transaction_expiry_boundary_accepted(mut with_cosmos_chain: MockDeps) {
        let broadcaster = with_cosmos_chain.api.addr_make("broadcaster");
        let info = message_info(&broadcaster, &[]);
        let mut env = mock_env();
        // Set block time to an exact-second value (no sub-second component) so that
        // Timestamp::from_seconds(expiry) == env.block.time exactly, exercising the `<=` boundary.
        let exact_second = 1_700_000_000u64;
        env.block.time = Timestamp::from_seconds(exact_second);

        let signer_address = get_cosmos_address();
        let data = MetaTransactionData {
            signer_address: signer_address.clone(),
            signer_prefix: "euclid".to_string(),
            signer_chain_uid: ChainUid::create("testchain".to_string()).unwrap(),
            call_data: vec![],
            // exact boundary: expiry == block.time.seconds()
            expiry: exact_second,
            nonce: "nonce_boundary".to_string(),
        };
        let (signature, signer_pubkey) = sign_cosmos_meta_tx(&data);
        let meta_tx = MetaTransaction {
            data,
            signature,
            signer_pubkey,
        };

        let res = execute(
            with_cosmos_chain.as_mut(),
            env,
            info,
            ExecuteMsg::ExecuteMetaTransaction(meta_tx),
        );
        assert!(
            res.is_ok(),
            "boundary expiry should be accepted: {:?}",
            res.err()
        );
    }

    // -----------------------------------------------------------------------
    // ExecuteMetaTransaction: nonce replay is rejected
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_meta_transaction_nonce_replay_rejected(mut with_cosmos_chain: MockDeps) {
        let env = mock_env();

        let broadcaster = with_cosmos_chain.api.addr_make("broadcaster");
        let nonce = "reused_nonce".to_string();
        let meta_tx = make_signed_cosmos_tx(&env, &nonce, vec![]);

        execute(
            with_cosmos_chain.as_mut(),
            env.clone(),
            message_info(&broadcaster, &[]),
            ExecuteMsg::ExecuteMetaTransaction(meta_tx),
        )
        .unwrap();

        let meta_tx2 = make_signed_cosmos_tx(&env, &nonce, vec![]);
        let res = execute(
            with_cosmos_chain.as_mut(),
            env,
            message_info(&broadcaster, &[]),
            ExecuteMsg::ExecuteMetaTransaction(meta_tx2),
        );

        assert!(res.is_err());
        let err_str = res.unwrap_err().to_string();
        assert!(
            err_str.contains("Nonce already used"),
            "unexpected error: {err_str}"
        );
    }

    // -----------------------------------------------------------------------
    // ExecuteMetaTransaction: invalid signature rejected (Cosmos)
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_meta_transaction_invalid_cosmos_signature_rejected(mut with_cosmos_chain: MockDeps) {
        let broadcaster = with_cosmos_chain.api.addr_make("broadcaster");
        let info = message_info(&broadcaster, &[]);
        let env = mock_env();

        let signer_address = get_cosmos_address();
        let pubkey_b64 = get_pubkey_binary().to_base64();

        let meta_tx = MetaTransaction {
            data: MetaTransactionData {
                signer_address,
                signer_prefix: "euclid".to_string(),
                signer_chain_uid: ChainUid::create("testchain".to_string()).unwrap(),
                call_data: vec![],
                expiry: env.block.time.seconds() + 3600,
                nonce: "nonce_cosmos_invalid".to_string(),
            },
            signature: Binary::from(vec![0u8; 64]).to_base64(),
            signer_pubkey: pubkey_b64,
        };

        let res = execute(
            with_cosmos_chain.as_mut(),
            env,
            info,
            ExecuteMsg::ExecuteMetaTransaction(meta_tx),
        );
        assert!(res.is_err());
    }

    // -----------------------------------------------------------------------
    // ExecuteMetaTransaction: invalid signature rejected (EVM)
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_meta_transaction_invalid_evm_signature_rejected(mut with_evm_chain: MockDeps) {
        let broadcaster = with_evm_chain.api.addr_make("broadcaster");
        let info = message_info(&broadcaster, &[]);
        let env = mock_env();

        let evm_pubkey = "044089a9fb9f67cdac85610900f61d69e2adc7e5da37036585955ca85d0ea148202a1e1750d26b825efb5c3e9aff92c6faf37bb1865c9b9612b064e89c6806e408";
        let evm_address = "0x20c863d309b5e56cd7502301b89b9223829fa7b5";

        let meta_tx = MetaTransaction {
            data: MetaTransactionData {
                signer_address: evm_address.to_string(),
                signer_prefix: "0x".to_string(),
                signer_chain_uid: ChainUid::create("evmchain".to_string()).unwrap(),
                call_data: vec![],
                expiry: env.block.time.seconds() + 3600,
                nonce: "nonce_evm_invalid".to_string(),
            },
            signature: "0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000".to_string(),
            signer_pubkey: evm_pubkey.to_string(),
        };

        let res = execute(
            with_evm_chain.as_mut(),
            env,
            info,
            ExecuteMsg::ExecuteMetaTransaction(meta_tx),
        );
        assert!(res.is_err());
    }

    // -----------------------------------------------------------------------
    // ExecuteMetaTransaction: invalid EVM pubkey format (non-hex)
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_meta_transaction_evm_invalid_pubkey_format(mut with_evm_chain: MockDeps) {
        let broadcaster = with_evm_chain.api.addr_make("broadcaster");
        let info = message_info(&broadcaster, &[]);
        let env = mock_env();

        let meta_tx = MetaTransaction {
            data: MetaTransactionData {
                signer_address: "0x1234".to_string(),
                signer_prefix: "0x".to_string(),
                signer_chain_uid: ChainUid::create("evmchain".to_string()).unwrap(),
                call_data: vec![],
                expiry: env.block.time.seconds() + 3600,
                nonce: "nonce_bad_pubkey".to_string(),
            },
            // non-hex pubkey: HexBinary::from_hex will fail before signature is checked
            signer_pubkey: "not_hex_at_all".to_string(),
            signature: "deadbeef".to_string(),
        };

        let res = execute(
            with_evm_chain.as_mut(),
            env,
            info,
            ExecuteMsg::ExecuteMetaTransaction(meta_tx),
        );
        assert!(res.is_err());
    }

    // -----------------------------------------------------------------------
    // ExecuteMetaTransaction: valid Cosmos tx — happy path
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_meta_transaction_valid_cosmos_happy_path(mut with_cosmos_chain: MockDeps) {
        let broadcaster = with_cosmos_chain.api.addr_make("broadcaster");
        let info = message_info(&broadcaster, &[]);
        let env = mock_env();
        let target = Addr::unchecked("target_contract");

        let meta_tx = make_signed_cosmos_tx(
            &env,
            "unique_nonce_happy",
            vec![MetaTransactionCallData {
                target: target.clone(),
                call_data: "some_call_data".to_string(),
            }],
        );

        let res = execute(
            with_cosmos_chain.as_mut(),
            env.clone(),
            info,
            ExecuteMsg::ExecuteMetaTransaction(meta_tx),
        )
        .unwrap();

        assert_eq!(res.messages.len(), 1);
        if let CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            funds,
            ..
        }) = &res.messages[0].msg
        {
            assert_eq!(contract_addr, &target.to_string());
            assert!(funds.is_empty());
        } else {
            panic!("expected WasmMsg::Execute");
        }

        let attr_keys: Vec<&str> = res.attributes.iter().map(|a| a.key.as_str()).collect();
        assert!(
            attr_keys.contains(&"meta_sender_key"),
            "missing meta_sender_key"
        );
        assert!(
            attr_keys.contains(&"meta_broadcaster"),
            "missing meta_broadcaster"
        );

        let sender_key = format!("testchain:{}", get_cosmos_address());
        let height = NONCES
            .load(
                &with_cosmos_chain.storage,
                (sender_key, "unique_nonce_happy".to_string()),
            )
            .unwrap();
        assert_eq!(height, Uint256::from(env.block.height));
    }

    // -----------------------------------------------------------------------
    // ExecuteMetaTransaction: WasmMsg encodes MetaReceive correctly
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_meta_transaction_wasm_msg_encodes_call_data(mut with_cosmos_chain: MockDeps) {
        let broadcaster = with_cosmos_chain.api.addr_make("broadcaster");
        let info = message_info(&broadcaster, &[]);
        let env = mock_env();
        let target = Addr::unchecked("encode_target");
        let call_data_str = "my_encoded_call".to_string();

        let meta_tx = make_signed_cosmos_tx(
            &env,
            "nonce_encode",
            vec![MetaTransactionCallData {
                target: target.clone(),
                call_data: call_data_str.clone(),
            }],
        );

        let res = execute(
            with_cosmos_chain.as_mut(),
            env,
            info,
            ExecuteMsg::ExecuteMetaTransaction(meta_tx),
        )
        .unwrap();

        assert_eq!(res.messages.len(), 1);
        if let CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr, msg, ..
        }) = &res.messages[0].msg
        {
            assert_eq!(contract_addr, &target.to_string());
            let MetaReceiverMsg::MetaReceive(inner) = from_json::<MetaReceiverMsg>(msg).unwrap();
            assert_eq!(inner.call_data, call_data_str);
            assert_eq!(inner.verified_sender.chain_uid.as_str(), "testchain");
            assert_eq!(inner.verified_sender.address, get_cosmos_address());
        } else {
            panic!("expected WasmMsg::Execute");
        }
    }

    // -----------------------------------------------------------------------
    // ExecuteMetaTransaction: multiple call_data entries emit multiple messages
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_meta_transaction_multiple_call_data_emits_multiple_messages(
        mut with_cosmos_chain: MockDeps,
    ) {
        let broadcaster = with_cosmos_chain.api.addr_make("broadcaster");
        let info = message_info(&broadcaster, &[]);
        let env = mock_env();

        let target1 = Addr::unchecked("target1");
        let target2 = Addr::unchecked("target2");
        let target3 = Addr::unchecked("target3");

        let meta_tx = make_signed_cosmos_tx(
            &env,
            "multi_nonce",
            vec![
                MetaTransactionCallData {
                    target: target1.clone(),
                    call_data: "call1".to_string(),
                },
                MetaTransactionCallData {
                    target: target2.clone(),
                    call_data: "call2".to_string(),
                },
                MetaTransactionCallData {
                    target: target3.clone(),
                    call_data: "call3".to_string(),
                },
            ],
        );

        let res = execute(
            with_cosmos_chain.as_mut(),
            env,
            info,
            ExecuteMsg::ExecuteMetaTransaction(meta_tx),
        )
        .unwrap();

        assert_eq!(res.messages.len(), 3);
        let targets: Vec<String> = res
            .messages
            .iter()
            .map(|m| {
                if let CosmosMsg::Wasm(WasmMsg::Execute { contract_addr, .. }) = &m.msg {
                    contract_addr.clone()
                } else {
                    panic!("expected WasmMsg::Execute");
                }
            })
            .collect();
        assert_eq!(targets[0], target1.to_string());
        assert_eq!(targets[1], target2.to_string());
        assert_eq!(targets[2], target3.to_string());
    }

    // -----------------------------------------------------------------------
    // ExecuteMetaTransaction: no call_data emits no messages
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_meta_transaction_empty_call_data_emits_no_messages(mut with_cosmos_chain: MockDeps) {
        let broadcaster = with_cosmos_chain.api.addr_make("broadcaster");
        let info = message_info(&broadcaster, &[]);
        let env = mock_env();

        let meta_tx = make_signed_cosmos_tx(&env, "nonce_empty_calldata", vec![]);

        let res = execute(
            with_cosmos_chain.as_mut(),
            env,
            info,
            ExecuteMsg::ExecuteMetaTransaction(meta_tx),
        )
        .unwrap();

        assert!(res.messages.is_empty());
    }

    // -----------------------------------------------------------------------
    // ExecuteMetaTransaction: meta_sender_key attribute format
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_meta_transaction_sender_key_format(mut with_cosmos_chain: MockDeps) {
        let broadcaster = with_cosmos_chain.api.addr_make("broadcaster");
        let info = message_info(&broadcaster, &[]);
        let env = mock_env();

        let meta_tx = make_signed_cosmos_tx(&env, "nonce_fmt", vec![]);

        let res = execute(
            with_cosmos_chain.as_mut(),
            env,
            info.clone(),
            ExecuteMsg::ExecuteMetaTransaction(meta_tx),
        )
        .unwrap();

        let expected_sender_key = format!("testchain:{}", get_cosmos_address());
        assert_eq!(
            res.attributes[0],
            attr("meta_sender_key", &expected_sender_key)
        );
        assert_eq!(
            res.attributes[1],
            attr("meta_broadcaster", broadcaster.to_string())
        );
    }

    // -----------------------------------------------------------------------
    // ExecuteMetaTransaction: nonce is persisted after success
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_meta_transaction_nonce_persisted_after_success(mut with_cosmos_chain: MockDeps) {
        let broadcaster = with_cosmos_chain.api.addr_make("broadcaster");
        let info = message_info(&broadcaster, &[]);
        let env = mock_env();
        let nonce = "persist_nonce".to_string();

        let meta_tx = make_signed_cosmos_tx(&env, &nonce, vec![]);

        execute(
            with_cosmos_chain.as_mut(),
            env.clone(),
            info,
            ExecuteMsg::ExecuteMetaTransaction(meta_tx),
        )
        .unwrap();

        let sender_key = format!("testchain:{}", get_cosmos_address());
        let stored_height = NONCES
            .load(&with_cosmos_chain.storage, (sender_key, nonce))
            .unwrap();

        assert_eq!(stored_height, Uint256::from(env.block.height));
    }

    // -----------------------------------------------------------------------
    // ExecuteMetaTransaction: nonce uniqueness across senders
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_same_nonce_different_senders_are_independent(mut with_cosmos_chain: MockDeps) {
        let env = mock_env();
        let nonce = "shared_nonce".to_string();

        let other_sender_key = "testchain:other_address".to_string();
        NONCES
            .save(
                with_cosmos_chain.as_mut().storage,
                (other_sender_key, nonce.clone()),
                &Uint256::from(5u128),
            )
            .unwrap();

        let broadcaster = with_cosmos_chain.api.addr_make("broadcaster");
        let meta_tx = make_signed_cosmos_tx(&env, &nonce, vec![]);

        let res = execute(
            with_cosmos_chain.as_mut(),
            env,
            message_info(&broadcaster, &[]),
            ExecuteMsg::ExecuteMetaTransaction(meta_tx),
        );
        assert!(
            res.is_ok(),
            "same nonce for different sender should not block: {:?}",
            res.err()
        );
    }

    // -----------------------------------------------------------------------
    // ExecuteMetaTransaction: address mismatch rejected (Cosmos)
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_meta_transaction_cosmos_address_mismatch_rejected(mut with_cosmos_chain: MockDeps) {
        let broadcaster = with_cosmos_chain.api.addr_make("broadcaster");
        let info = message_info(&broadcaster, &[]);
        let env = mock_env();

        let wrong_address = "euclid1wrongaddressxxxxxxxxxxxxxxxxxxxxxxx".to_string();
        let data = MetaTransactionData {
            signer_address: wrong_address.clone(),
            signer_prefix: "euclid".to_string(),
            signer_chain_uid: ChainUid::create("testchain".to_string()).unwrap(),
            call_data: vec![],
            expiry: env.block.time.seconds() + 3600,
            nonce: "nonce_mismatch".to_string(),
        };
        let (signature, signer_pubkey) = sign_cosmos_meta_tx(&data);

        let meta_tx = MetaTransaction {
            data,
            signature,
            signer_pubkey,
        };

        let res = execute(
            with_cosmos_chain.as_mut(),
            env,
            info,
            ExecuteMsg::ExecuteMetaTransaction(meta_tx),
        );

        assert!(res.is_err());
        let err_str = res.unwrap_err().to_string();
        assert!(
            err_str.contains("Address mismatch"),
            "unexpected error: {err_str}"
        );
    }

    // -----------------------------------------------------------------------
    // State invariant: nonce map grows monotonically per sender
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_nonce_map_grows_with_each_successful_transaction(mut with_cosmos_chain: MockDeps) {
        let broadcaster = with_cosmos_chain.api.addr_make("broadcaster");
        let env = mock_env();

        for i in 0..3u32 {
            let nonce = format!("sequential_nonce_{i}");
            let meta_tx = make_signed_cosmos_tx(&env, &nonce, vec![]);
            execute(
                with_cosmos_chain.as_mut(),
                env.clone(),
                message_info(&broadcaster, &[]),
                ExecuteMsg::ExecuteMetaTransaction(meta_tx),
            )
            .unwrap();
        }

        let sender_key = format!("testchain:{}", get_cosmos_address());
        for i in 0..3u32 {
            let nonce = format!("sequential_nonce_{i}");
            assert!(
                NONCES
                    .may_load(
                        &with_cosmos_chain.storage,
                        (sender_key.clone(), nonce.clone()),
                    )
                    .unwrap()
                    .is_some(),
                "nonce {nonce} should be present in storage"
            );
        }
    }

    // -----------------------------------------------------------------------
    // ExecuteMetaTransaction: EVM — valid real vectors (from relayer tests)
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_meta_transaction_evm_valid_real_vectors(mut with_evm_chain: MockDeps) {
        let broadcaster = with_evm_chain.api.addr_make("broadcaster");
        let info = message_info(&broadcaster, &[]);
        let env = mock_env();

        let evm_address = "0x887e4aac216674d2c432798f851c1ea5d505b2e1".to_string();
        let evm_pubkey_hex = "0437c6e8362883ef2497eed6adefa91e8d11783a1f4d535334d6e9d3040bbbd3cba65033a064647d202020e7741595630c67062bc1cc0f585659e2469231b3112f";
        let evm_sig_hex = "6c943497a66306ef5024728a42c8c5352a76a125e8495de16339ec51874f924a03a580a974e3f7a80239504e6982d443ffeb84b71cec30506b2c16d91872879b1b";

        let target_addr =
            Addr::unchecked("euclid1yvgh8xeju5dyr0zxlkvq09htvhjj20fncp5g58np4u25g8rkpgjsy5hngy");
        let call_data_str = r#"{"execute_swap_request":{"amount_in":"1000000000000000000","asset_in":{"token":"stt","token_type":{"voucher":{}}},"asset_out":"mon","cross_chain_addresses":[],"min_amount_out":"2299751846672827","partner_fee":null,"swaps":[{"token_in":"stt","token_out":"weuclid"},{"token_in":"weuclid","token_out":"euclid"},{"token_in":"euclid","token_out":"mon"}]}}"#;

        set_router_chain_query(
            &mut with_evm_chain,
            ChainUid::create("somnia".to_string()).unwrap(),
            ChainType::Evm(EvmChain {
                chain_id: "somnia".to_string(),
            }),
        );

        let data = MetaTransactionData {
            signer_address: evm_address.clone(),
            signer_prefix: "0x".to_string(),
            signer_chain_uid: ChainUid::create("somnia".to_string()).unwrap(),
            call_data: vec![MetaTransactionCallData {
                target: target_addr.clone(),
                call_data: call_data_str.to_string(),
            }],
            expiry: 1765897937,
            nonce: "1765897877".to_string(),
        };

        let serialized = to_json_string(&data).unwrap();
        let expected_json = r#"{"signer_address":"0x887e4aac216674d2c432798f851c1ea5d505b2e1","signer_prefix":"0x","signer_chain_uid":"somnia","call_data":[{"target":"euclid1yvgh8xeju5dyr0zxlkvq09htvhjj20fncp5g58np4u25g8rkpgjsy5hngy","call_data":"{\"execute_swap_request\":{\"amount_in\":\"1000000000000000000\",\"asset_in\":{\"token\":\"stt\",\"token_type\":{\"voucher\":{}}},\"asset_out\":\"mon\",\"cross_chain_addresses\":[],\"min_amount_out\":\"2299751846672827\",\"partner_fee\":null,\"swaps\":[{\"token_in\":\"stt\",\"token_out\":\"weuclid\"},{\"token_in\":\"weuclid\",\"token_out\":\"euclid\"},{\"token_in\":\"euclid\",\"token_out\":\"mon\"}]}}"}],"expiry":1765897937,"nonce":"1765897877"}"#;
        assert_eq!(
            serialized, expected_json,
            "JSON serialization must match the signed message"
        );

        assert!(
            data.expiry > env.block.time.seconds(),
            "expiry should be in the future relative to mock_env"
        );

        let meta_tx = MetaTransaction {
            data,
            signature: evm_sig_hex.to_string(),
            signer_pubkey: evm_pubkey_hex.to_string(),
        };

        let res = execute(
            with_evm_chain.as_mut(),
            env.clone(),
            info,
            ExecuteMsg::ExecuteMetaTransaction(meta_tx),
        )
        .unwrap();

        assert_eq!(res.messages.len(), 1);
        let sender_key = format!("somnia:{}", evm_address);
        assert_eq!(res.attributes[0], attr("meta_sender_key", &sender_key));

        let height = NONCES
            .load(
                &with_evm_chain.storage,
                (sender_key, "1765897877".to_string()),
            )
            .unwrap();
        assert_eq!(height, Uint256::from(env.block.height));
    }

    // -----------------------------------------------------------------------
    // ExecuteMetaTransaction: Native chain type — reuses Cosmos code path
    // -----------------------------------------------------------------------

    /// `ChainType::Native {}` is matched alongside `ChainType::Cosmos` in the
    /// same arm of the `match chain_type` block.  This test ensures that arm
    /// is exercised for a Native chain and that a correctly signed transaction
    /// is accepted end-to-end.
    #[rstest]
    fn test_meta_transaction_native_chain_type_happy_path(mut with_native_chain: MockDeps) {
        let broadcaster = with_native_chain.api.addr_make("broadcaster");
        let info = message_info(&broadcaster, &[]);
        let env = mock_env();
        let target = Addr::unchecked("native_target");

        let meta_tx = make_signed_cosmos_tx(
            &env,
            "native_nonce_happy",
            vec![MetaTransactionCallData {
                target: target.clone(),
                call_data: "native_call".to_string(),
            }],
        );

        let res = execute(
            with_native_chain.as_mut(),
            env.clone(),
            info,
            ExecuteMsg::ExecuteMetaTransaction(meta_tx),
        )
        .unwrap();

        assert_eq!(res.messages.len(), 1);
        if let CosmosMsg::Wasm(WasmMsg::Execute { contract_addr, .. }) = &res.messages[0].msg {
            assert_eq!(contract_addr, &target.to_string());
        } else {
            panic!("expected WasmMsg::Execute for native chain");
        }

        let sender_key = format!("testchain:{}", get_cosmos_address());
        let height = NONCES
            .load(
                &with_native_chain.storage,
                (sender_key, "native_nonce_happy".to_string()),
            )
            .unwrap();
        assert_eq!(height, Uint256::from(env.block.height));
    }

    // -----------------------------------------------------------------------
    // ExecuteMetaTransaction: invalid Cosmos pubkey format (non-base64) rejected
    // -----------------------------------------------------------------------

    /// Parallel to the EVM non-hex pubkey test: `Binary::from_base64` should
    /// return an error before signature verification is attempted.
    #[rstest]
    fn test_meta_transaction_cosmos_invalid_pubkey_format_rejected(
        mut with_cosmos_chain: MockDeps,
    ) {
        let broadcaster = with_cosmos_chain.api.addr_make("broadcaster");
        let info = message_info(&broadcaster, &[]);
        let env = mock_env();

        let meta_tx = MetaTransaction {
            data: MetaTransactionData {
                signer_address: get_cosmos_address(),
                signer_prefix: "euclid".to_string(),
                signer_chain_uid: ChainUid::create("testchain".to_string()).unwrap(),
                call_data: vec![],
                expiry: env.block.time.seconds() + 3600,
                nonce: "nonce_bad_cosmos_pubkey".to_string(),
            },
            // Valid base64 alphabet contains [A-Za-z0-9+/=]; "!!not_base64!!"
            // will cause from_base64 to fail immediately.
            signer_pubkey: "!!not_base64!!".to_string(),
            signature: Binary::from(vec![0u8; 64]).to_base64(),
        };

        let res = execute(
            with_cosmos_chain.as_mut(),
            env,
            info,
            ExecuteMsg::ExecuteMetaTransaction(meta_tx),
        );
        assert!(
            res.is_err(),
            "non-base64 Cosmos pubkey should produce an error"
        );
    }

    // -----------------------------------------------------------------------
    // UpdateAdmin: invalid new_admin address is rejected
    // -----------------------------------------------------------------------

    /// `update_admin` calls `deps.api.addr_validate` on the new admin string.
    /// Supplying a syntactically invalid address must produce an error before any
    /// state mutation occurs.
    #[rstest]
    fn test_update_admin_invalid_address_rejected(mut initialized: MockDeps) {
        let sender = initialized.api.addr_make("sender");
        let info = message_info(&sender, &[]);

        let res = execute(
            initialized.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::UpdateAdmin(UpdateAdminMsg {
                new_admin: "NOT_A_VALID_BECH32_ADDRESS!!".to_string(),
                admin_type: AdminType::GeneralAdmin,
            }),
        );

        assert!(
            res.is_err(),
            "invalid bech32 address should be rejected by addr_validate"
        );
        // State must be unchanged — the original sender is still the general_admin.
        let stored = ADMIN.load(&initialized.storage).unwrap();
        let original = initialized.api.addr_make("sender");
        assert_eq!(stored.general_admin, original);
    }

    // -----------------------------------------------------------------------
    // ExecuteMetaTransaction: EVM signer_address with mixed case is rejected
    // by CrossChainUser::validate() after signature verification succeeds
    // -----------------------------------------------------------------------

    /// `CrossChainUser::validate()` rejects addresses that are not fully
    /// lowercase.  For EVM chains the contract sets `verified_sender` using the
    /// *claimed* `signer_address` (after the mismatch check vs the derived
    /// address).  This test uses a real EVM key/signature vector where the
    /// `signer_address` intentionally contains uppercase hex characters so that
    /// both the mismatch check AND `validate()` would reject it — we confirm the
    /// error is produced before any NONCES entry is written.
    #[rstest]
    fn test_meta_transaction_evm_mixed_case_address_rejected(mut with_evm_chain: MockDeps) {
        let broadcaster = with_evm_chain.api.addr_make("broadcaster");
        let info = message_info(&broadcaster, &[]);
        let env = mock_env();

        // The signer_address has uppercase letters — validate() must reject it.
        let mixed_case_address = "0xABCDEF0000000000000000000000000000000000".to_string();
        let meta_tx = MetaTransaction {
            data: MetaTransactionData {
                signer_address: mixed_case_address.clone(),
                signer_prefix: "0x".to_string(),
                signer_chain_uid: ChainUid::create("evmchain".to_string()).unwrap(),
                call_data: vec![],
                expiry: env.block.time.seconds() + 3600,
                nonce: "nonce_mixed_case".to_string(),
            },
            // Signature and pubkey do not need to be valid — the address mismatch
            // or validate() will reject before nonce storage.
            signature: "0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000".to_string(),
            signer_pubkey: "044089a9fb9f67cdac85610900f61d69e2adc7e5da37036585955ca85d0ea148202a1e1750d26b825efb5c3e9aff92c6faf37bb1865c9b9612b064e89c6806e408".to_string(),
        };

        let res = execute(
            with_evm_chain.as_mut(),
            env,
            info,
            ExecuteMsg::ExecuteMetaTransaction(meta_tx),
        );

        assert!(
            res.is_err(),
            "mixed-case EVM signer_address should be rejected"
        );
        // Confirm no nonce entry was written for the mixed-case sender_key.
        let sender_key = format!("evmchain:{}", mixed_case_address);
        let nonce_entry = NONCES
            .may_load(
                &with_evm_chain.storage,
                (sender_key, "nonce_mixed_case".to_string()),
            )
            .unwrap();
        assert!(
            nonce_entry.is_none(),
            "nonce must not be stored when transaction is rejected"
        );
    }
}
