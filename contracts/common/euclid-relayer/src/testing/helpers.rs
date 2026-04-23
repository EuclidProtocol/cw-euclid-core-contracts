use crate::contract::instantiate;
use cosmwasm_std::testing::{message_info, mock_env, MockQuerier};
use cosmwasm_std::{to_json_binary, Binary, Response};
use euclid::chain::ChainUid;
use k256::ecdsa::SigningKey;
use k256::elliptic_curve::NonZeroScalar;
use relayer::msgs::{
    InstantiateMsg, MetaTransaction, MetaTransactionData, Validator, ValidatorSignature,
};
use sha2::{Digest, Sha256};
use std::str::FromStr;

pub type MockDeps = cosmwasm_std::OwnedDeps<
    cosmwasm_std::testing::MockStorage,
    cosmwasm_std::testing::MockApi,
    MockQuerier,
>;

// -------------------------------------------------------------------
// Fixed test-key (same as in packages/relayer/src/verify.rs tests)
// -------------------------------------------------------------------

const TEST_SIGNING_KEY_HEX: &str =
    "2268A9118C1681EC6A649F01886995DE55E90C7E71B0BC5E409C551B92FF7369";

pub fn get_signer_key() -> (SigningKey, Binary) {
    let scalar = NonZeroScalar::from_str(TEST_SIGNING_KEY_HEX).unwrap();
    let secret_key = SigningKey::from(scalar);
    let pub_key = Binary::from(
        secret_key
            .verifying_key()
            .to_encoded_point(true)
            .as_bytes()
            .to_vec(),
    );
    (secret_key, pub_key)
}

/// Sign `msg` with the test signing key and return (signature, pubkey).
pub fn sign_message(msg: &str) -> (Binary, Binary) {
    let digest = Sha256::new().chain_update(msg.as_bytes());
    let (secret_key, pub_key) = get_signer_key();
    let (sig, _) = secret_key.sign_digest_recoverable(digest).unwrap();
    (Binary::from(sig.to_bytes().as_slice()), pub_key)
}

/// Produce the string that the relayer hashes/signs:
///   `{data},{expiry},{chain_uid}`
pub fn expiry_call_data(data: &str, expiry: u64, chain_uid: &str) -> String {
    format!(
        "{data},{expiry},{chain_uid}",
        data = data,
        expiry = expiry,
        chain_uid = chain_uid
    )
}

// -------------------------------------------------------------------
// Address constants
// -------------------------------------------------------------------

pub const TEST_CHAIN_UID: &str = "testchain";

// -------------------------------------------------------------------
// init helper
// -------------------------------------------------------------------

pub fn init(deps: &mut MockDeps) -> Response {
    let (_, pub_key) = get_signer_key();
    let msg = InstantiateMsg {
        message_signer: Validator {
            pubkey: pub_key,
            address: "signer_address".to_string(),
        },
        signature_threshold: 1,
    };
    let sender = deps.api.addr_make("sender");
    let info = message_info(&sender, &[]);
    instantiate(deps.as_mut(), mock_env(), info, msg).unwrap()
}

pub fn test_chain_uid() -> ChainUid {
    ChainUid::create(TEST_CHAIN_UID.to_string()).unwrap()
}

/// Build a validator + its signing helper from a private-key hex string.
pub fn make_validator(signing_key_hex: &str) -> (Validator, SigningKey) {
    let scalar = NonZeroScalar::from_str(signing_key_hex).unwrap();
    let sk = SigningKey::from(scalar);
    let pub_key = Binary::from(
        sk.verifying_key()
            .to_encoded_point(true)
            .as_bytes()
            .to_vec(),
    );
    let validator = Validator {
        pubkey: pub_key,
        address: format!("validator_{}", signing_key_hex.get(0..8).unwrap_or("x")),
    };
    (validator, sk)
}

/// Sign the relayer expiry call data with a validator's signing key.
pub fn sign_validator_message(
    sk: &SigningKey,
    data: &str,
    expiry: u64,
    chain_uid: &str,
) -> ValidatorSignature {
    let payload = expiry_call_data(data, expiry, chain_uid);
    let digest = Sha256::new().chain_update(payload.as_bytes());
    let (sig, _) = sk.sign_digest_recoverable(digest).unwrap();
    let pub_key = Binary::from(
        sk.verifying_key()
            .to_encoded_point(true)
            .as_bytes()
            .to_vec(),
    );
    ValidatorSignature {
        pubkey: pub_key,
        signature: Binary::from(sig.to_bytes().as_slice()),
        expiry,
    }
}

/// Build a complete valid `MetaTransaction` that will pass all checks.
pub fn make_valid_meta_transaction(
    deps: &mut MockDeps,
    nonce: &str,
    target: &str,
    validator_sk: &SigningKey,
    expiry: u64,
    chain_uid_str: &str,
) -> MetaTransaction {
    let meta_data = MetaTransactionData {
        target: cosmwasm_std::Addr::unchecked(target),
        call_data: to_json_binary(&"dummy").unwrap(),
        nonce: nonce.to_string(),
    };
    let data_str = cosmwasm_std::to_json_string(&meta_data).unwrap();

    // Sign with the admin signer key
    let admin_payload = expiry_call_data(&data_str, expiry, chain_uid_str);
    let (admin_sig, _) = sign_message(&admin_payload);

    // Register the validator for this chain first
    let chain_uid = ChainUid::create(chain_uid_str.to_string()).unwrap();
    let validator = Validator {
        pubkey: Binary::from(
            validator_sk
                .verifying_key()
                .to_encoded_point(true)
                .as_bytes()
                .to_vec(),
        ),
        address: "test_validator".to_string(),
    };
    crate::state::VALIDATORS
        .save(deps.as_mut().storage, chain_uid, &vec![validator])
        .unwrap();

    let validator_sig = sign_validator_message(validator_sk, &data_str, expiry, chain_uid_str);

    MetaTransaction {
        data: data_str,
        expiry,
        admin_signature: admin_sig,
        validator_signatures: vec![validator_sig],
        chain_uid: ChainUid::create(chain_uid_str.to_string()).unwrap(),
    }
}
