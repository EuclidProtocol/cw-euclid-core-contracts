use cosmwasm_std::{
    ensure, from_json, DepsMut, Env, MessageInfo, Response, Timestamp, Uint256, WasmMsg,
};
use euclid::{admin, chain::ChainUid, error::ContractError};
use relayer::{
    msgs::{MetaTransaction, UpdateAdminMsg, UpdateStateMsg},
    verify::verify_signature,
    MetaTransactionData, Validator,
};

use crate::state::{ADMIN, NONCES, STATE, VALIDATORS};

pub fn execute_update_state(
    deps: &mut DepsMut,
    info: &MessageInfo,
    msg: UpdateStateMsg,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(info)?;
    let mut state = STATE.load(deps.storage)?;
    let admin = ADMIN.load(deps.storage)?;
    ensure!(
        info.sender == admin.general_admin,
        ContractError::Unauthorized {}
    );
    let mut response = Response::new();
    if let Some(message_signer) = msg.message_signer {
        state.message_signer = message_signer.clone();
        response = response
            .add_attribute(
                "message_signer_pubkey_old_value",
                state.message_signer.pubkey.to_string(),
            )
            .add_attribute(
                "message_signer_pubkey_new_value",
                message_signer.pubkey.to_string(),
            )
            .add_attribute(
                "message_signer_address_old_value",
                state.message_signer.address.to_string(),
            )
            .add_attribute(
                "message_signer_address_new_value",
                message_signer.address.to_string(),
            );
    }

    if let Some(signature_threshold) = msg.signature_threshold {
        state.signature_threshold = signature_threshold;
        response = response
            .add_attribute(
                "signature_threshold_old_value",
                state.signature_threshold.to_string(),
            )
            .add_attribute(
                "signature_threshold_new_value",
                signature_threshold.to_string(),
            );
    }
    STATE.save(deps.storage, &state)?;
    Ok(response)
}

pub fn execute_update_admin(
    deps: &mut DepsMut,
    env: Env,
    info: &MessageInfo,
    msg: UpdateAdminMsg,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(info)?;
    let current_admin = ADMIN.load(deps.storage)?;
    let (updated_admins, response) = admin::update_admin(
        &current_admin,
        deps,
        &env,
        &info.sender,
        msg.new_admin.clone(),
        msg.admin_type,
    )?;

    ADMIN.save(deps.storage, &updated_admins)?;
    Ok(response
        .add_attribute("old_admin", current_admin.to_string())
        .add_attribute("new_admin", msg.new_admin.to_string()))
}

pub fn execute_meta_transaction(
    deps: &mut DepsMut,
    env: &Env,
    info: &MessageInfo,
    msg: MetaTransaction,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(info)?;
    let state = STATE.load(deps.storage)?;
    let meta_transaction: MetaTransactionData = from_json(msg.data.clone())?;
    // Ensure the nonce is not used
    ensure!(
        !NONCES.has(deps.storage, meta_transaction.nonce.clone()),
        ContractError::new(format!("Nonce already used: {}", meta_transaction.nonce).as_str())
    );
    // Save the nonce
    NONCES.save(
        deps.storage,
        meta_transaction.nonce.clone(),
        &Uint256::from(env.block.height),
    )?;

    // Ensure the timestamp is not exceeded
    ensure!(
        env.block.time <= Timestamp::from_seconds(msg.expiry),
        ContractError::new("Timestamp limit exceeded")
    );

    let verified = verify_signature(
        deps.as_ref(),
        &expiry_call_data(&msg.data, msg.expiry, msg.chain_uid.as_str()),
        &msg.admin_signature,
        &state.message_signer.pubkey,
    )?;

    ensure!(verified, ContractError::new("Invalid admin signature"));
    let validators = VALIDATORS
        .load(deps.storage, msg.chain_uid.clone())
        .map_err(|_| ContractError::new("Validators not found for chain"))?;
    let mut visited = vec![false; validators.len()];
    let mut valid_signatures = 0;
    for signature in msg.validator_signatures {
        let validator_index = validators
            .iter()
            .position(|v| v.pubkey == signature.pubkey)
            .ok_or(ContractError::new("Validator not found"))?;
        if visited[validator_index] {
            continue;
        }
        let verified = verify_signature(
            deps.as_ref(),
            &expiry_call_data(&msg.data, signature.expiry, msg.chain_uid.as_str()),
            &signature.signature,
            &signature.pubkey,
        )?;
        if !verified {
            continue;
        }
        valid_signatures += 1;
        visited[validator_index] = true;
    }

    ensure!(
        valid_signatures >= state.signature_threshold,
        ContractError::new(
            format!(
                "Threshold not met: expected {}, got {}",
                state.signature_threshold, valid_signatures
            )
            .as_str()
        )
    );

    let relay_msg = WasmMsg::Execute {
        contract_addr: meta_transaction.target.to_string(),
        msg: meta_transaction.call_data,
        funds: vec![],
    };

    Ok(Response::new()
        .add_message(relay_msg)
        .add_attribute("relayer_nonce", meta_transaction.nonce)
        .add_attribute("relayer_target", meta_transaction.target)
        .add_attribute("relayer_sender", info.sender.to_string()))
}

fn expiry_call_data(data: &str, expiry: u64, chain_uid: &str) -> String {
    let expiry_call_data = format!(
        "{data},{expiry},{chain_uid}",
        data = data,
        expiry = expiry,
        chain_uid = chain_uid
    );
    expiry_call_data
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{ADMIN, NONCES, STATE, VALIDATORS};
    use crate::testing::helpers::{
        expiry_call_data as helper_expiry_call_data, get_signer_key, init,
        make_valid_meta_transaction, make_validator, sign_message, test_chain_uid,
    };
    use cosmwasm_std::testing::{message_info, mock_dependencies, mock_env};
    use cosmwasm_std::{attr, to_json_binary, Addr, Timestamp};
    use euclid::admin::{AdminType, EuclidAdmin};
    use euclid::chain::ChainUid;
    use euclid::error::ContractError;
    use relayer::msgs::{MetaTransactionData, UpdateAdminMsg, UpdateStateMsg, Validator};
    use rstest::rstest;

    // -----------------------------------------------------------------------
    // execute_update_state
    // -----------------------------------------------------------------------

    #[test]
    fn test_update_state_happy_path() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let sender = deps.api.addr_make("sender");
        let info = message_info(&sender, &[]);

        let (_, new_pub_key) = get_signer_key();
        let new_signer = Validator {
            pubkey: new_pub_key.clone(),
            address: "new_address".to_string(),
        };
        let msg = UpdateStateMsg {
            message_signer: Some(new_signer),
            signature_threshold: Some(2),
        };

        let res = execute_update_state(&mut deps.as_mut(), &info, msg).unwrap();

        let state = STATE.load(&deps.storage).unwrap();
        assert_eq!(state.message_signer.pubkey, new_pub_key);
        assert_eq!(state.signature_threshold, 2);

        // Response has the old and new attribute keys
        let keys: Vec<&str> = res.attributes.iter().map(|a| a.key.as_str()).collect();
        assert!(keys.contains(&"message_signer_pubkey_old_value"));
        assert!(keys.contains(&"message_signer_pubkey_new_value"));
        assert!(keys.contains(&"signature_threshold_old_value"));
        assert!(keys.contains(&"signature_threshold_new_value"));
    }

    #[test]
    fn test_update_state_partial_update_signer_only() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let sender = deps.api.addr_make("sender");
        let info = message_info(&sender, &[]);

        let (_, new_pub_key) = get_signer_key();
        let msg = UpdateStateMsg {
            message_signer: Some(Validator {
                pubkey: new_pub_key.clone(),
                address: "addr".to_string(),
            }),
            signature_threshold: None,
        };
        execute_update_state(&mut deps.as_mut(), &info, msg).unwrap();

        let state = STATE.load(&deps.storage).unwrap();
        assert_eq!(state.message_signer.pubkey, new_pub_key);
        assert_eq!(state.signature_threshold, 1); // unchanged
    }

    #[test]
    fn test_update_state_partial_update_threshold_only() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let sender = deps.api.addr_make("sender");
        let info = message_info(&sender, &[]);

        let msg = UpdateStateMsg {
            message_signer: None,
            signature_threshold: Some(5),
        };
        execute_update_state(&mut deps.as_mut(), &info, msg).unwrap();

        let state = STATE.load(&deps.storage).unwrap();
        assert_eq!(state.signature_threshold, 5);
    }

    #[test]
    fn test_update_state_unauthorized() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let not_admin = deps.api.addr_make("not_admin");
        let info = message_info(&not_admin, &[]);

        let msg = UpdateStateMsg {
            message_signer: None,
            signature_threshold: Some(2),
        };
        let err = execute_update_state(&mut deps.as_mut(), &info, msg).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    // -----------------------------------------------------------------------
    // execute_update_admin
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::general_admin(AdminType::GeneralAdmin)]
    #[case::fee_admin(AdminType::FeeAdmin)]
    #[case::migration_admin(AdminType::MigrationAdmin)]
    fn test_update_admin_happy_path(#[case] admin_type: AdminType) {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let env = mock_env();

        let sender = deps.api.addr_make("sender");
        let info = message_info(&sender, &[]);
        let new_admin = deps.api.addr_make("new_admin");

        let msg = UpdateAdminMsg {
            new_admin: new_admin.to_string(),
            admin_type: admin_type.clone(),
        };

        let res = execute_update_admin(&mut deps.as_mut(), env, &info, msg).unwrap();

        let keys: Vec<&str> = res.attributes.iter().map(|a| a.key.as_str()).collect();
        assert!(keys.contains(&"old_admin"));
        assert!(keys.contains(&"new_admin"));

        let saved_admin = ADMIN.load(&deps.storage).unwrap();
        match admin_type {
            AdminType::GeneralAdmin => assert_eq!(saved_admin.general_admin, new_admin),
            AdminType::FeeAdmin => assert_eq!(saved_admin.fee_admin, new_admin),
            AdminType::MigrationAdmin => assert_eq!(saved_admin.migration_admin, new_admin),
        }
    }

    #[test]
    fn test_update_admin_unauthorized() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let env = mock_env();

        let not_admin = deps.api.addr_make("not_admin");
        let info = message_info(&not_admin, &[]);

        let msg = UpdateAdminMsg {
            new_admin: not_admin.to_string(),
            admin_type: AdminType::GeneralAdmin,
        };
        let err = execute_update_admin(&mut deps.as_mut(), env, &info, msg).unwrap_err();
        // EuclidAdmin::verify_update_access returns UnauthorizedWithMsg, not Unauthorized
        assert!(matches!(err, ContractError::UnauthorizedWithMsg { .. }));
    }

    // -----------------------------------------------------------------------
    // execute_add_validator / execute_remove_validator
    // -----------------------------------------------------------------------

    #[test]
    fn test_add_validator_happy_path() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let sender = deps.api.addr_make("sender");
        let info = message_info(&sender, &[]);
        let chain_uid = test_chain_uid();

        let (validator, _) =
            make_validator("2268A9118C1681EC6A649F01886995DE55E90C7E71B0BC5E409C551B92FF7369");
        let validator_address = validator.address.clone();

        let res = execute_add_validator(
            &mut deps.as_mut(),
            &info,
            validator.clone(),
            chain_uid.clone(),
        )
        .unwrap();

        assert_eq!(
            res.attributes[0],
            attr("validator_added", &validator_address)
        );

        let saved = VALIDATORS.load(&deps.storage, chain_uid).unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0], validator);
    }

    #[test]
    fn test_add_validator_unauthorized() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let not_admin = deps.api.addr_make("not_admin");
        let info = message_info(&not_admin, &[]);
        let chain_uid = test_chain_uid();
        let (validator, _) =
            make_validator("2268A9118C1681EC6A649F01886995DE55E90C7E71B0BC5E409C551B92FF7369");
        let err =
            execute_add_validator(&mut deps.as_mut(), &info, validator, chain_uid).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    #[test]
    fn test_add_validator_duplicate_rejected() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let sender = deps.api.addr_make("sender");
        let info = message_info(&sender, &[]);
        let chain_uid = test_chain_uid();
        let (validator, _) =
            make_validator("2268A9118C1681EC6A649F01886995DE55E90C7E71B0BC5E409C551B92FF7369");

        execute_add_validator(
            &mut deps.as_mut(),
            &info,
            validator.clone(),
            chain_uid.clone(),
        )
        .unwrap();
        let err =
            execute_add_validator(&mut deps.as_mut(), &info, validator, chain_uid).unwrap_err();
        assert!(matches!(err, ContractError::Generic { .. }));
    }

    #[test]
    fn test_add_multiple_validators_different_chains() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let sender = deps.api.addr_make("sender");
        let info = message_info(&sender, &[]);

        let (v1, _) =
            make_validator("2268A9118C1681EC6A649F01886995DE55E90C7E71B0BC5E409C551B92FF7369");
        let chain_a = ChainUid::create("chaina".to_string()).unwrap();
        let chain_b = ChainUid::create("chainb".to_string()).unwrap();

        execute_add_validator(&mut deps.as_mut(), &info, v1.clone(), chain_a.clone()).unwrap();
        execute_add_validator(&mut deps.as_mut(), &info, v1.clone(), chain_b.clone()).unwrap();

        assert_eq!(VALIDATORS.load(&deps.storage, chain_a).unwrap().len(), 1);
        assert_eq!(VALIDATORS.load(&deps.storage, chain_b).unwrap().len(), 1);
    }

    #[test]
    fn test_remove_validator_happy_path() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let sender = deps.api.addr_make("sender");
        let info = message_info(&sender, &[]);
        let chain_uid = test_chain_uid();

        let (validator, _) =
            make_validator("2268A9118C1681EC6A649F01886995DE55E90C7E71B0BC5E409C551B92FF7369");
        execute_add_validator(
            &mut deps.as_mut(),
            &info,
            validator.clone(),
            chain_uid.clone(),
        )
        .unwrap();

        let res = execute_remove_validator(&mut deps.as_mut(), &info, validator, chain_uid.clone())
            .unwrap();

        assert_eq!(res.attributes[0].key, "validator_removed");

        let saved = VALIDATORS.load(&deps.storage, chain_uid).unwrap();
        assert!(saved.is_empty());
    }

    #[test]
    fn test_remove_validator_unauthorized() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let not_admin = deps.api.addr_make("not_admin");
        let info = message_info(&not_admin, &[]);
        let chain_uid = test_chain_uid();
        let (validator, _) =
            make_validator("2268A9118C1681EC6A649F01886995DE55E90C7E71B0BC5E409C551B92FF7369");
        let err =
            execute_remove_validator(&mut deps.as_mut(), &info, validator, chain_uid).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    #[test]
    fn test_remove_validator_not_found() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let sender = deps.api.addr_make("sender");
        let info = message_info(&sender, &[]);
        let chain_uid = test_chain_uid();

        let (validator, _) =
            make_validator("2268A9118C1681EC6A649F01886995DE55E90C7E71B0BC5E409C551B92FF7369");
        let err =
            execute_remove_validator(&mut deps.as_mut(), &info, validator, chain_uid).unwrap_err();
        assert!(matches!(err, ContractError::Generic { .. }));
    }

    // -----------------------------------------------------------------------
    // execute_meta_transaction — error paths that don't need real sigs
    // -----------------------------------------------------------------------

    #[test]
    fn test_meta_transaction_nonce_already_used() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        // Pre-seed a used nonce
        NONCES
            .save(
                deps.as_mut().storage,
                "used-nonce".to_string(),
                &cosmwasm_std::Uint256::from(1u64),
            )
            .unwrap();

        let meta_data = MetaTransactionData {
            target: Addr::unchecked("some_target"),
            call_data: to_json_binary(&"dummy").unwrap(),
            nonce: "used-nonce".to_string(),
        };
        let data_str = cosmwasm_std::to_json_string(&meta_data).unwrap();

        let msg = relayer::msgs::MetaTransaction {
            data: data_str,
            expiry: u64::MAX,
            admin_signature: cosmwasm_std::Binary::from(vec![0u8; 64]),
            validator_signatures: vec![],
            chain_uid: test_chain_uid(),
        };

        let sender = deps.api.addr_make("sender");
        let info = message_info(&sender, &[]);
        let err =
            execute_meta_transaction(&mut deps.as_mut(), &mock_env(), &info, msg).unwrap_err();
        assert!(matches!(err, ContractError::Generic { .. }));
    }

    #[test]
    fn test_meta_transaction_expired_timestamp() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let meta_data = MetaTransactionData {
            target: Addr::unchecked("some_target"),
            call_data: to_json_binary(&"dummy").unwrap(),
            nonce: "fresh-nonce".to_string(),
        };
        let data_str = cosmwasm_std::to_json_string(&meta_data).unwrap();

        // expiry = 0 means any block time > 0 will exceed it
        let msg = relayer::msgs::MetaTransaction {
            data: data_str,
            expiry: 0,
            admin_signature: cosmwasm_std::Binary::from(vec![0u8; 64]),
            validator_signatures: vec![],
            chain_uid: test_chain_uid(),
        };

        let sender = deps.api.addr_make("sender");
        let info = message_info(&sender, &[]);
        let env = mock_env(); // mock_env has block.time = 1_571_797_419
        let err = execute_meta_transaction(&mut deps.as_mut(), &env, &info, msg).unwrap_err();
        assert!(matches!(err, ContractError::Generic { .. }));
    }

    // -----------------------------------------------------------------------
    // execute_meta_transaction — happy path with real secp256k1 signatures
    // -----------------------------------------------------------------------

    #[test]
    fn test_meta_transaction_happy_path() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        // Use the same signing key as the admin signer (set in init)
        let (_, validator_sk) = {
            use k256::ecdsa::SigningKey;
            use k256::elliptic_curve::NonZeroScalar;
            use std::str::FromStr;
            let scalar = NonZeroScalar::from_str(
                "2268A9118C1681EC6A649F01886995DE55E90C7E71B0BC5E409C551B92FF7369",
            )
            .unwrap();
            let sk = SigningKey::from(scalar);
            ((), sk)
        };

        let chain_uid_str = "testchain";
        // Use a far-future expiry (year ~2100)
        let expiry = 4_102_444_800u64;

        let msg = make_valid_meta_transaction(
            &mut deps,
            "nonce-1",
            "some_target",
            &validator_sk,
            expiry,
            chain_uid_str,
        );

        let sender = deps.api.addr_make("sender");
        let info = message_info(&sender, &[]);

        let mut env = mock_env();
        env.block.time = Timestamp::from_seconds(1_000_000); // well before expiry

        let res = execute_meta_transaction(&mut deps.as_mut(), &env, &info, msg).unwrap();

        assert_eq!(res.messages.len(), 1);
        assert_eq!(
            res.attributes
                .iter()
                .find(|a| a.key == "relayer_nonce")
                .unwrap()
                .value,
            "nonce-1"
        );
        assert_eq!(
            res.attributes
                .iter()
                .find(|a| a.key == "relayer_target")
                .unwrap()
                .value,
            "some_target"
        );
        assert_eq!(
            res.attributes
                .iter()
                .find(|a| a.key == "relayer_sender")
                .unwrap()
                .value,
            sender.to_string()
        );

        // Nonce is now recorded
        assert!(NONCES.has(&deps.storage, "nonce-1".to_string()));
    }

    #[test]
    fn test_meta_transaction_nonce_saved_after_relay() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let (_, validator_sk) = {
            use k256::ecdsa::SigningKey;
            use k256::elliptic_curve::NonZeroScalar;
            use std::str::FromStr;
            let scalar = NonZeroScalar::from_str(
                "2268A9118C1681EC6A649F01886995DE55E90C7E71B0BC5E409C551B92FF7369",
            )
            .unwrap();
            ((), SigningKey::from(scalar))
        };

        let expiry = 4_102_444_800u64;
        let msg = make_valid_meta_transaction(
            &mut deps,
            "unique-nonce",
            "target",
            &validator_sk,
            expiry,
            "testchain",
        );

        let sender = deps.api.addr_make("sender");
        let info = message_info(&sender, &[]);
        let mut env = mock_env();
        env.block.time = Timestamp::from_seconds(1_000_000);

        execute_meta_transaction(&mut deps.as_mut(), &env, &info, msg).unwrap();

        assert!(NONCES.has(&deps.storage, "unique-nonce".to_string()));
        let block_height = NONCES
            .load(&deps.storage, "unique-nonce".to_string())
            .unwrap();
        assert_eq!(block_height, cosmwasm_std::Uint256::from(env.block.height));
    }

    #[test]
    fn test_meta_transaction_threshold_not_met() {
        let mut deps = mock_dependencies();
        // Set threshold to 2
        let (_, pub_key) = get_signer_key();
        let state = relayer::msgs::State {
            message_signer: Validator {
                pubkey: pub_key,
                address: "signer".to_string(),
            },
            signature_threshold: 2,
        };
        let sender = deps.api.addr_make("sender");
        STATE.save(deps.as_mut().storage, &state).unwrap();
        ADMIN
            .save(deps.as_mut().storage, &EuclidAdmin::default(sender.clone()))
            .unwrap();

        let (_, validator_sk) = {
            use k256::ecdsa::SigningKey;
            use k256::elliptic_curve::NonZeroScalar;
            use std::str::FromStr;
            let scalar = NonZeroScalar::from_str(
                "2268A9118C1681EC6A649F01886995DE55E90C7E71B0BC5E409C551B92FF7369",
            )
            .unwrap();
            ((), SigningKey::from(scalar))
        };

        // make_valid_meta_transaction provides only 1 validator sig
        let expiry = 4_102_444_800u64;
        let msg = make_valid_meta_transaction(
            &mut deps,
            "nonce-threshold",
            "target",
            &validator_sk,
            expiry,
            "testchain",
        );

        let info = message_info(&sender, &[]);
        let mut env = mock_env();
        env.block.time = Timestamp::from_seconds(1_000_000);

        let err = execute_meta_transaction(&mut deps.as_mut(), &env, &info, msg).unwrap_err();
        assert!(matches!(err, ContractError::Generic { .. }));
    }

    #[test]
    fn test_meta_transaction_no_validators_for_chain() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let meta_data = MetaTransactionData {
            target: Addr::unchecked("target"),
            call_data: to_json_binary(&"dummy").unwrap(),
            nonce: "nonce-no-validators".to_string(),
        };
        let data_str = cosmwasm_std::to_json_string(&meta_data).unwrap();

        // Sign admin payload so we can get past admin sig check
        let admin_payload = helper_expiry_call_data(&data_str, 4_102_444_800, "unknownchain");
        let (admin_sig, _) = sign_message(&admin_payload);

        let msg = relayer::msgs::MetaTransaction {
            data: data_str,
            expiry: 4_102_444_800,
            admin_signature: admin_sig,
            validator_signatures: vec![],
            chain_uid: ChainUid::create("unknownchain".to_string()).unwrap(),
        };

        let sender = deps.api.addr_make("sender");
        let info = message_info(&sender, &[]);
        let mut env = mock_env();
        env.block.time = Timestamp::from_seconds(1_000_000);

        let err = execute_meta_transaction(&mut deps.as_mut(), &env, &info, msg).unwrap_err();
        assert!(matches!(err, ContractError::Generic { .. }));
    }
}

pub fn execute_add_validator(
    deps: &mut DepsMut,
    info: &MessageInfo,
    validator: Validator,
    chain_uid: ChainUid,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(info)?;
    let admin = ADMIN.load(deps.storage)?;
    ensure!(
        info.sender == admin.general_admin,
        ContractError::Unauthorized {}
    );
    let mut validators = VALIDATORS
        .load(deps.storage, chain_uid.clone())
        .unwrap_or_default();
    ensure!(
        !validators.contains(&validator),
        ContractError::new("Validator already exists")
    );
    validators.push(validator.clone());
    VALIDATORS.save(deps.storage, chain_uid, &validators)?;
    Ok(Response::new().add_attribute("validator_added", validator.address.to_string()))
}

pub fn execute_remove_validator(
    deps: &mut DepsMut,
    info: &MessageInfo,
    validator: Validator,
    chain_uid: ChainUid,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(info)?;
    let admin = ADMIN.load(deps.storage)?;
    ensure!(
        info.sender == admin.general_admin,
        ContractError::Unauthorized {}
    );
    let mut validators = VALIDATORS
        .load(deps.storage, chain_uid.clone())
        .unwrap_or_default();
    let index = validators
        .iter()
        .position(|v| v.address == validator.address);
    if let Some(index) = index {
        validators.remove(index);
    } else {
        return Err(ContractError::new("Validator does not exist"));
    }
    VALIDATORS.save(deps.storage, chain_uid, &validators)?;
    Ok(Response::new().add_attribute("validator_removed", validator.address.clone()))
}
