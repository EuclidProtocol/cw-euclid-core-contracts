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

pub fn verify_signature(
    deps: Deps,
    message: &str,
    signature: &Binary,
    pubkey: &Binary,
) -> Result<bool, ContractError> {
    let message_hash: [u8; 32] = Sha256::digest(message).into();
    let signature_bytes = signature.as_slice();
    let pubkey_bytes = pubkey.as_slice();
    deps.api
        .secp256k1_verify(&message_hash, signature_bytes, pubkey_bytes)
        .map_err(|err| ContractError::new(&err.to_string()))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    use cosmwasm_std::{testing::mock_dependencies, to_json_binary, to_json_string, Binary};
    use k256::{
        ecdsa::{signature::SignerMut, SigningKey},
        elliptic_curve::NonZeroScalar,
    };
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

    fn sign_messsage(msg: &str) -> (Binary, Binary) {
        let message_digest = Sha256::new().chain(msg.as_bytes());

        let (secret_key, public_key_bytes) = get_signer_key();
        let signature = secret_key
            .sign_digest_recoverable(message_digest)
            .unwrap()
            .0;
        (
            Binary::from(signature.to_vec()),
            Binary::from(public_key_bytes),
        )
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

        assert!(verify_signature(deps.as_ref(), &msg_str, &signature, &pub_key,).unwrap());
    }
}
