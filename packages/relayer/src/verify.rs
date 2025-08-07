use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Binary, Coin, Deps, Uint128};
use euclid::error::ContractError;
use sha2::{Digest, Sha256};

// https://docs.cosmos.network/main/build/architecture/adr-036-arbitrary-signature
#[cw_serde]
pub struct MsgSignData {
    pub account_number: Uint128,
    pub chain_id: String,
    pub fee: MsgSignDataFee,
    pub memo: String,
    pub msgs: Vec<MsgSignDataMsg>,
    pub sequence: Uint128,
}

impl MsgSignData {
    pub fn new(msgs: Vec<MsgSignDataMsg>) -> Self {
        Self {
            account_number: Uint128::zero(),
            chain_id: "".to_string(),
            fee: MsgSignDataFee::new(),
            memo: "".to_string(),
            msgs,
            sequence: Uint128::zero(),
        }
    }
}

#[cw_serde]
pub struct MsgSignDataFee {
    pub amount: Vec<Coin>,
    pub gas: Uint128,
}

impl Default for MsgSignDataFee {
    fn default() -> Self {
        Self::new()
    }
}

impl MsgSignDataFee {
    pub fn new() -> Self {
        Self {
            amount: vec![],
            gas: Uint128::zero(),
        }
    }
}

#[cw_serde]
pub struct MsgSignDataMsg {
    pub r#type: String,
    pub value: MsgSignDataValue,
}

impl MsgSignDataMsg {
    pub fn new(value: MsgSignDataValue) -> Self {
        Self {
            r#type: "sign/MsgSignData".to_string(),
            value,
        }
    }
}

#[cw_serde]
pub struct MsgSignDataValue {
    pub data: Binary,
    pub signer: String,
}

impl MsgSignDataValue {
    pub fn new(data: Binary, signer: String) -> Self {
        Self { data, signer }
    }
}

pub fn msg_to_sign_data(msg: Binary, signer: String) -> MsgSignData {
    let msg_sign_data_msg = MsgSignDataMsg::new(MsgSignDataValue::new(msg, signer));
    MsgSignData::new(vec![msg_sign_data_msg])
}

pub fn get_k256_pubkey(pubkey: &Binary) -> Result<k256::ecdsa::VerifyingKey, ContractError> {
    let pubkey_bytes = pubkey.as_slice();
    k256::ecdsa::VerifyingKey::from_sec1_bytes(pubkey_bytes)
        .map_err(|e| ContractError::new(&format!("Invalid public key: {}", e)))
}

pub fn verify_signature(
    deps: Deps,
    message: &str,
    signature: &Binary,
    pubkey: &Binary,
) -> Result<bool, ContractError> {
    let message_hash: [u8; 32] = Sha256::digest(message).into();
    let signature_bytes = signature.as_slice();
    let pubkey_bytes = pubkey.as_slice();
    // If pubkey is compressed (33 bytes), uncompress it
    let pubkey_bytes = if pubkey_bytes.len() == 33 {
        let verifying_key = get_k256_pubkey(pubkey)?;
        verifying_key.to_encoded_point(false).as_bytes().to_vec()
    } else {
        pubkey_bytes.to_vec()
    };
    deps.api
        .secp256k1_verify(&message_hash, signature_bytes, &pubkey_bytes)
        .map_err(|err| ContractError::new(&err.to_string()))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    use cosmwasm_std::{testing::mock_dependencies, Binary};
    use k256::{ecdsa::SigningKey, elliptic_curve::NonZeroScalar};
    use sha2::{digest::Update, Digest, Sha256};

    fn get_signer_key() -> (SigningKey, Binary) {
        let pk = "2268A9118C1681EC6A649F01886995DE55E90C7E71B0BC5E409C551B92FF7369";
        let scalar = NonZeroScalar::from_str(pk).unwrap();

        let secret_key = SigningKey::from(scalar);

        let pub_key = Binary::from(
            secret_key
                .verifying_key()
                .to_encoded_point(false)
                .as_bytes()
                .to_vec(),
        );

        (secret_key, pub_key)
    }

    fn get_signer_key_from_priv_key(priv_key: &str) -> (SigningKey, Binary) {
        let scalar = NonZeroScalar::from_str(priv_key).unwrap();

        let secret_key = SigningKey::from(scalar);

        let pub_key = Binary::from(
            secret_key
                .verifying_key()
                .to_encoded_point(false)
                .as_bytes()
                .to_vec(),
        );

        (secret_key, pub_key)
    }

    fn sign_messsage(msg: &str) -> (Binary, Binary) {
        let message_digest = Sha256::new().chain(msg.as_bytes());

        let (secret_key, public_key_bytes) = get_signer_key();
        let signature = secret_key
            .sign_digest_recoverable(message_digest)
            .unwrap()
            .0;
        (Binary::from(signature.to_vec()), public_key_bytes)
    }

    fn sign_messsage_from_priv_key(msg: &str, priv_key: &str) -> (Binary, Binary) {
        let message_digest = Sha256::new().chain(msg.as_bytes());
        let (secret_key, public_key_bytes) = get_signer_key_from_priv_key(priv_key);
        let signature = secret_key
            .sign_digest_recoverable(message_digest)
            .unwrap()
            .0;
        (Binary::from(signature.to_vec()), public_key_bytes)
    }

    #[test]
    fn test_verify_signature() {
        let msg: String = "Hello World!".to_string();

        let (signature, public_key_bytes) = sign_messsage(&msg);

        // verifying
        let deps = mock_dependencies();

        assert!(verify_signature(deps.as_ref(), &msg, &signature, &public_key_bytes).unwrap());
    }
    #[test]
    fn test_verify_signature_external() {
        let msg_str = "{\"account_number\":\"0\",\"chain_id\":\"\",\"fee\":{\"amount\":[],\"gas\":\"0\"},\"memo\":\"\",\"msgs\":[{\"type\":\"sign/MsgSignData\",\"value\":{\"data\":\"eyJjYWxsX2RhdGEiOiJleUoxY0dSaGRHVmZjM1JoZEdVaU9udDlmUT09IiwiZXhwaXJ5Ijo3MjAwLCJub25jZSI6IjEiLCJ0YXJnZXQiOiJyb3V0ZXIifQ==\"}}],\"sequence\":\"0\"}";
        let signature = "F8RW1nYpcKNQU71UdSoIB0SUShTNqbXQyCc6CPAvqTgVSVHZbDIDnyg5vZYFQfI8kGr1No8sQkh7b31/GEvxAg==";
        let (_secret_key, pub_key) = get_signer_key();
        let signature = Binary::from_base64(signature).unwrap();

        let deps = mock_dependencies();

        assert!(verify_signature(deps.as_ref(), msg_str, &signature, &pub_key,).unwrap());
    }

    #[test]
    fn test_verify_signature_external_claim() {
        let msg_str = r#"{"chain_id":"","account_number":"0","sequence":"0","fee":{"amount":[],"gas":"0"},"msgs":[{"type":"sign/MsgSignData","value":{"data":"eyJjbGFpbV9pZCI6NywicmVjaXBpZW50Ijp7ImFkZHJlc3MiOiJpbmoxY2txYWF1cDZxbHhncjR6ZDB4MHFjZGxyM3ZjbTg0dnd2bnRydHYiLCJjaGFpbl91aWQiOiJpbmplY3RpdmUifSwicmVsZWFzZV9mdW5kcyI6ZmFsc2V9","signer":"inj1ckqaaup6qlxgr4zd0x0qcdlr3vcm84vwvntrtv"}}],"memo":""}"#;
        let pub_key_base64 = "AyX++cbmJAz14kYZO8HYVFTamX047aBqOFDS4XpFOHs9";
        let pub_key = Binary::from_base64(pub_key_base64).unwrap();
        let pub_key = get_k256_pubkey(&pub_key).unwrap().to_encoded_point(false);
        let pub_key = Binary::from(pub_key.as_bytes().to_vec());
        // let priv_key = "29dc810d1b8d9994278131236cd8cd4fe8d8d8ff274d72c952e4396910cbe21d";
        let priv_key = "29dc810d1b8d9994278131236cd8cd4fe8d8d8ff274d72c952e4396910cbe21d";
        let (_secret_key, pub_key_from_priv_key) = get_signer_key_from_priv_key(priv_key);

        assert_eq!(
            pub_key, pub_key_from_priv_key,
            "pub_key should be the same {} != {}",
            pub_key, pub_key_from_priv_key
        );

        let signature = "VQxfVo0bviysKz5QSEZqnmzcmEZt/Rgta4qpbEKx5wEcuO5E63b8RwLZmQ+KXSYPKTgkTGn4MX+nXdBWrH79TQ==";
        let signature = Binary::from_base64(signature).unwrap();

        let custom_signed_msg = sign_messsage_from_priv_key(msg_str, priv_key).0;
        assert_eq!(
            signature, custom_signed_msg,
            "signature should be the same {} != {}",
            signature, custom_signed_msg
        );

        let deps = mock_dependencies();
        assert!(verify_signature(deps.as_ref(), msg_str, &signature, &pub_key).unwrap());
    }
}
