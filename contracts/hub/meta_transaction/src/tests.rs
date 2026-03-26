#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests {
    use crate::contract::{execute, instantiate, query};
    use crate::state::{ADMIN, NONCES, STATE};
    use cosmwasm_std::testing::{message_info, mock_dependencies, mock_env, MockQuerier};
    use cosmwasm_std::{
        attr, from_json, to_json_binary, to_json_string, Addr, Binary, CosmosMsg, Env, Timestamp,
        Uint128, WasmMsg,
    };
    use euclid::admin::{AdminType, EuclidAdmin};
    use euclid::chain::{Chain, ChainType, ChainUid, CosmosChain, EvmChain};
    use euclid::error::ContractError;
    use euclid::msgs::meta_transaction::msg::{
        ExecuteMsg, InstantiateMsg, MetaTransaction, MetaTransactionCallData, MetaTransactionData,
        QueryMsg, UpdateAdminMsg,
    };
    use euclid::msgs::router::ChainResponse;
    use k256::{ecdsa::SigningKey, elliptic_curve::NonZeroScalar};
    use relayer::verify::{cosmos_address_from_pubkey, msg_to_sign_data};
    use rstest::{fixture, rstest};
    use sha2::{digest::Update, Digest, Sha256};
    use std::str::FromStr;

    // -----------------------------------------------------------------------
    // Type alias
    // -----------------------------------------------------------------------

    type MockDeps = cosmwasm_std::OwnedDeps<
        cosmwasm_std::MemoryStorage,
        cosmwasm_std::testing::MockApi,
        MockQuerier,
    >;

    // -----------------------------------------------------------------------
    // Crypto helpers: sign the same way as the contract verifies
    // -----------------------------------------------------------------------

    /// Known secp256k1 private key (same used in relayer verify tests).
    const COSMOS_SK_HEX: &str = "2268A9118C1681EC6A649F01886995DE55E90C7E71B0BC5E409C551B92FF7369";

    /// Get the k256 signing key (same key used in relayer verify tests).
    fn get_signing_key() -> SigningKey {
        let scalar = NonZeroScalar::from_str(COSMOS_SK_HEX).unwrap();
        SigningKey::from(scalar)
    }

    /// Compressed 33-byte public key as Binary.
    fn get_pubkey_binary() -> Binary {
        let sk = get_signing_key();
        Binary::from(
            sk.verifying_key()
                .to_encoded_point(true)
                .as_bytes()
                .to_vec(),
        )
    }

    /// Bech32 address derived from the public key with prefix "euclid".
    fn get_cosmos_address() -> String {
        cosmos_address_from_pubkey(&get_pubkey_binary(), "euclid").unwrap()
    }

    /// Sign `MetaTransactionData` using the same algorithm that the contract verifies.
    /// Returns (signature_base64, pubkey_base64).
    fn sign_cosmos_meta_tx(data: &MetaTransactionData) -> (String, String) {
        let sk = get_signing_key();
        let pubkey = get_pubkey_binary();

        // Replicate the contract's signing path:
        // 1. data_binary = to_json_binary(data)
        let data_binary = to_json_binary(data).unwrap();
        // 2. msg_sign_data = msg_to_sign_data(data_binary, signer_address)
        let msg_sign_data = msg_to_sign_data(data_binary, data.signer_address.clone());
        // 3. msg_sign_data_str = to_json_string(msg_sign_data)
        let msg_sign_data_str = to_json_string(&msg_sign_data).unwrap();

        // 4. ECDSA-sign: sha256(msg_sign_data_str), matches verify_signature in relayer
        let message_digest = Sha256::new().chain(msg_sign_data_str.as_bytes());
        let (sig, _recovery_id) = sk
            .sign_digest_recoverable(message_digest)
            .expect("sign failed");

        let sig_b64 = Binary::from(sig.to_bytes().as_slice()).to_base64();
        let pubkey_b64 = pubkey.to_base64();
        (sig_b64, pubkey_b64)
    }

    /// Build a MetaTransactionData for the Cosmos signer and sign it.
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

    /// Register a mock WasmQuery handler that responds to router GetChain queries
    /// with the given chain_type.
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

    /// Base fixture: contract instantiated, no extra state.
    #[fixture]
    fn initialized() -> MockDeps {
        let mut deps = mock_dependencies();
        let sender = deps.api.addr_make("sender");
        let info = message_info(&sender, &[]);
        instantiate(deps.as_mut(), mock_env(), info, make_instantiate_msg()).unwrap();
        deps
    }

    /// Fixture with a Cosmos chain pre-seeded in the mock WasmQuerier.
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

    /// Fixture with an EVM chain pre-seeded in the mock WasmQuerier.
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

    // -----------------------------------------------------------------------
    // Instantiate tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_instantiate_stores_state_and_admin() {
        let mut deps = mock_dependencies();
        let sender = deps.api.addr_make("creator");
        let info = message_info(&sender, &[]);

        let res = instantiate(
            deps.as_mut(),
            mock_env(),
            info.clone(),
            make_instantiate_msg(),
        )
        .unwrap();

        // Attributes
        assert_eq!(res.attributes[0], attr("method", "instantiate"));
        assert_eq!(
            res.attributes[1],
            attr("router_contract", router_contract().as_str())
        );

        // STATE
        let state = STATE.load(&deps.storage).unwrap();
        assert_eq!(state.router_contract, router_contract());

        // ADMIN — all three roles default to sender
        let admin = ADMIN.load(&deps.storage).unwrap();
        assert_eq!(admin, EuclidAdmin::default(sender));
    }

    // -----------------------------------------------------------------------
    // Query: GetState
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_query_get_state(initialized: MockDeps) {
        let res = query(initialized.as_ref(), mock_env(), QueryMsg::GetState {}).unwrap();
        let state_resp: euclid::msgs::meta_transaction::msg::StateResponse =
            from_json(res).unwrap();

        assert_eq!(state_resp.router_contract, router_contract());
    }

    // -----------------------------------------------------------------------
    // Query: NonceRelayed — missing nonce returns error
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_query_nonce_relayed_missing_key_returns_error(initialized: MockDeps) {
        let res = query(
            initialized.as_ref(),
            mock_env(),
            QueryMsg::NonceRelayed {
                nonce: "nonexistent_nonce".to_string(),
            },
        );
        assert!(res.is_err(), "expected error for missing nonce");
    }

    // -----------------------------------------------------------------------
    // Query: NonceRelayed — present nonce returns block height
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_query_nonce_relayed_present_returns_height(mut initialized: MockDeps) {
        // The query uses (nonce.clone(), nonce.clone()) as key — i.e. (sender_key, nonce)
        // must both equal the query argument.
        let nonce_key = "mynonce".to_string();
        NONCES
            .save(
                initialized.as_mut().storage,
                (nonce_key.clone(), nonce_key.clone()),
                &Uint128::new(42),
            )
            .unwrap();

        let res = query(
            initialized.as_ref(),
            mock_env(),
            QueryMsg::NonceRelayed {
                nonce: nonce_key.clone(),
            },
        )
        .unwrap();
        let resp: euclid::msgs::meta_transaction::msg::NonceRelayedResponse =
            from_json(res).unwrap();
        assert_eq!(resp.height, Uint128::new(42));
    }

    // -----------------------------------------------------------------------
    // UpdateAdmin: access control (table-driven)
    // -----------------------------------------------------------------------

    #[rstest]
    #[case(
        "general_admin_updates_general_admin",
        "sender",
        AdminType::GeneralAdmin,
        "new_general",
        false
    )]
    #[case(
        "wrong_sender_cannot_update_general_admin",
        "attacker",
        AdminType::GeneralAdmin,
        "new_general",
        true
    )]
    #[case(
        "general_admin_updates_fee_admin",
        "sender",
        AdminType::FeeAdmin,
        "new_fee",
        false
    )]
    #[case(
        "wrong_sender_cannot_update_fee_admin",
        "attacker",
        AdminType::FeeAdmin,
        "new_fee",
        true
    )]
    #[case(
        "migration_admin_updates_migration_admin",
        "sender",
        AdminType::MigrationAdmin,
        "new_migration",
        false
    )]
    #[case(
        "wrong_sender_cannot_update_migration_admin",
        "attacker",
        AdminType::MigrationAdmin,
        "new_migration",
        true
    )]
    fn test_update_admin_access_control(
        mut initialized: MockDeps,
        #[case] name: &str,
        #[case] sender_name: &str,
        #[case] admin_type: AdminType,
        #[case] new_admin_name: &str,
        #[case] expect_error: bool,
    ) {
        let sender = initialized.api.addr_make(sender_name);
        let new_admin = initialized.api.addr_make(new_admin_name);
        let info = message_info(&sender, &[]);

        let msg = ExecuteMsg::UpdateAdmin(UpdateAdminMsg {
            new_admin: new_admin.to_string(),
            admin_type,
        });

        let res = execute(initialized.as_mut(), mock_env(), info, msg);

        if expect_error {
            assert!(res.is_err(), "{name}: expected error");
        } else {
            assert!(res.is_ok(), "{name}: expected success, got {:?}", res.err());
        }
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

        // Expect a WasmMsg::UpdateAdmin cosmos message
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

        // State persisted
        let stored_admin = ADMIN.load(&initialized.storage).unwrap();
        assert_eq!(stored_admin.migration_admin, new_migration);
    }

    // -----------------------------------------------------------------------
    // ExecuteMetaTransaction: timestamp expired
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_meta_transaction_expired_timestamp_rejected(mut with_cosmos_chain: MockDeps) {
        let broadcaster = with_cosmos_chain.api.addr_make("broadcaster");
        let info = message_info(&broadcaster, &[]);
        let mut env = mock_env();
        // Push block time well into the future so expiry is always in the past
        env.block.time = Timestamp::from_seconds(9_999_999);

        // Build a data where expiry < env.block.time (already expired)
        let signer_address = get_cosmos_address();
        let data = MetaTransactionData {
            signer_address: signer_address.clone(),
            signer_prefix: "euclid".to_string(),
            signer_chain_uid: ChainUid::create("testchain".to_string()).unwrap(),
            call_data: vec![],
            // expiry in the past relative to env.block.time
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
    // ExecuteMetaTransaction: nonce replay is rejected
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_meta_transaction_nonce_replay_rejected(mut with_cosmos_chain: MockDeps) {
        let env = mock_env();

        // First, execute a valid transaction to record the nonce
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

        // Replay the same nonce — must fail
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
            // All-zero 64-byte signature (invalid for any message)
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

        // Use real EVM pubkey but a zero signature
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
            // 65 zero bytes as hex (invalid signature)
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

        // Should emit one WasmMsg::Execute for the call_data target
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

        // Attributes
        let attr_keys: Vec<&str> = res.attributes.iter().map(|a| a.key.as_str()).collect();
        assert!(
            attr_keys.contains(&"meta_sender_key"),
            "missing meta_sender_key"
        );
        assert!(
            attr_keys.contains(&"meta_broadcaster"),
            "missing meta_broadcaster"
        );

        // Nonce must be saved in storage
        let sender_key = format!("testchain:{}", get_cosmos_address());
        let height = NONCES
            .load(
                &with_cosmos_chain.storage,
                (sender_key, "unique_nonce_happy".to_string()),
            )
            .unwrap();
        assert_eq!(height, Uint128::from(env.block.height));
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

        assert_eq!(stored_height, Uint128::from(env.block.height));
    }

    // -----------------------------------------------------------------------
    // ExecuteMetaTransaction: nonce uniqueness across senders
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_same_nonce_different_senders_are_independent(mut with_cosmos_chain: MockDeps) {
        let env = mock_env();
        let nonce = "shared_nonce".to_string();

        // Seed a nonce for a *different* sender key
        let other_sender_key = "testchain:other_address".to_string();
        NONCES
            .save(
                with_cosmos_chain.as_mut().storage,
                (other_sender_key, nonce.clone()),
                &Uint128::new(5),
            )
            .unwrap();

        // Our real signer should still be able to use the same nonce value
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

        // Build data claiming a wrong signer_address, sign it with the real key.
        // The derived address will differ from signer_address, so the check fails.
        let wrong_address = "euclid1wrongaddressxxxxxxxxxxxxxxxxxxxxxxx".to_string();
        let data = MetaTransactionData {
            signer_address: wrong_address.clone(),
            signer_prefix: "euclid".to_string(),
            signer_chain_uid: ChainUid::create("testchain".to_string()).unwrap(),
            call_data: vec![],
            expiry: env.block.time.seconds() + 3600,
            nonce: "nonce_mismatch".to_string(),
        };
        // Sign with real key so signature is valid, but the address won't match
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

        // Verify all 3 nonces are stored
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
        // From test_verify_evm_signature in packages/relayer/src/verify.rs
        // The signature covers: add_eth_prefix(to_json_string(&data))
        // We use the pre-computed vector from that test file.
        //
        // data JSON (before eth-prefix) must match the serialization of the
        // MetaTransactionData below. The EVM path does NOT wrap in msg_to_sign_data;
        // it signs: add_eth_prefix(to_json_string(&data)).
        //
        // We reconstruct the exact MetaTransactionData that was used to produce
        // this known signature from the relayer verify test.

        let broadcaster = with_evm_chain.api.addr_make("broadcaster");
        let info = message_info(&broadcaster, &[]);
        let env = mock_env();

        // EVM test vector #2 from test_verify_evm_signature:
        // signer_address = 0x887e4aac216674d2c432798f851c1ea5d505b2e1
        // The data JSON that was signed is the exact serialization of the
        // MetaTransactionData struct below (as verified in the relayer tests).
        let evm_address = "0x887e4aac216674d2c432798f851c1ea5d505b2e1".to_string();
        let evm_pubkey_hex = "0437c6e8362883ef2497eed6adefa91e8d11783a1f4d535334d6e9d3040bbbd3cba65033a064647d202020e7741595630c67062bc1cc0f585659e2469231b3112f";
        let evm_sig_hex = "6c943497a66306ef5024728a42c8c5352a76a125e8495de16339ec51874f924a03a580a974e3f7a80239504e6982d443ffeb84b71cec30506b2c16d91872879b1b";

        // Reconstruct the MetaTransactionData whose JSON matches the signed message.
        // The signed JSON was:
        // {"signer_address":"0x887e...","signer_prefix":"0x","signer_chain_uid":"somnia",
        //  "call_data":[{"target":"euclid1yv...","call_data":"{\"execute_swap_request\"...}"}],
        //  "expiry":1765897937,"nonce":"1765897877"}
        let target_addr =
            Addr::unchecked("euclid1yvgh8xeju5dyr0zxlkvq09htvhjj20fncp5g58np4u25g8rkpgjsy5hngy");
        let call_data_str = r#"{"execute_swap_request":{"amount_in":"1000000000000000000","asset_in":{"token":"stt","token_type":{"voucher":{}}},"asset_out":"mon","cross_chain_addresses":[],"min_amount_out":"2299751846672827","partner_fee":null,"swaps":[{"token_in":"stt","token_out":"weuclid"},{"token_in":"weuclid","token_out":"euclid"},{"token_in":"euclid","token_out":"mon"}]}}"#;

        // Override the querier to respond to "somnia" chain_uid
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

        // Verify that to_json_string(&data) == the expected JSON from the relayer test
        let serialized = to_json_string(&data).unwrap();
        let expected_json = r#"{"signer_address":"0x887e4aac216674d2c432798f851c1ea5d505b2e1","signer_prefix":"0x","signer_chain_uid":"somnia","call_data":[{"target":"euclid1yvgh8xeju5dyr0zxlkvq09htvhjj20fncp5g58np4u25g8rkpgjsy5hngy","call_data":"{\"execute_swap_request\":{\"amount_in\":\"1000000000000000000\",\"asset_in\":{\"token\":\"stt\",\"token_type\":{\"voucher\":{}}},\"asset_out\":\"mon\",\"cross_chain_addresses\":[],\"min_amount_out\":\"2299751846672827\",\"partner_fee\":null,\"swaps\":[{\"token_in\":\"stt\",\"token_out\":\"weuclid\"},{\"token_in\":\"weuclid\",\"token_out\":\"euclid\"},{\"token_in\":\"euclid\",\"token_out\":\"mon\"}]}}"}],"expiry":1765897937,"nonce":"1765897877"}"#;
        assert_eq!(
            serialized, expected_json,
            "JSON serialization must match the signed message"
        );

        // The expiry must be >= env.block.time; mock_env uses block height 12345, time ~1571797419
        // 1765897937 > 1571797419, so this is still valid.
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

        // Nonce persisted
        let height = NONCES
            .load(
                &with_evm_chain.storage,
                (sender_key, "1765897877".to_string()),
            )
            .unwrap();
        assert_eq!(height, Uint128::from(env.block.height));
    }
}
