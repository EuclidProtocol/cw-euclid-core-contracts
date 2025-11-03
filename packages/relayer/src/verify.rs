use bech32::{encode, ToBase32, Variant};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Binary, Coin, Deps, HexBinary, Uint128};
use euclid::error::ContractError;
use k256::{elliptic_curve::sec1::ToEncodedPoint, PublicKey};
use ripemd::Ripemd160;
use sha2::{Digest, Sha256};
use sha3::Keccak256;

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

pub fn verify_keccak256_signature(
    deps: Deps,
    message: &str,
    signature: &HexBinary,
    pubkey: &HexBinary,
) -> Result<bool, ContractError> {
    let message_hash: [u8; 32] = Keccak256::digest(message).into();
    let signature_bytes = signature.as_slice()[0..64].to_vec();
    let pubkey_bytes = pubkey.as_slice();
    deps.api
        .secp256k1_verify(&message_hash, &signature_bytes, pubkey_bytes)
        .map_err(|err| ContractError::new(&err.to_string()))
}

// Normalize pubkey: accept 65 (0x04+xy) or 64 (xy).
fn normalize_evm_pubkey(pk: &[u8]) -> Result<&[u8], String> {
    match pk.len() {
        65 if pk[0] == 0x04 => Ok(&pk[1..65]), // drop 0x04
        64 => Ok(pk),
        33 => Err("33-byte compressed pubkey: decompression needed (not implemented)".into()),
        _ => Err(format!("unexpected pubkey length: {}", pk.len())),
    }
}

// Normalize pubkey: accept 65 (0x04+xy) or 64 (xy).
fn normalize_cosmos_pubkey(pk: &[u8]) -> Result<Vec<u8>, String> {
    match pk.len() {
        33 => Ok(pk.to_vec()),
        64 => {
            // prepend 0x04 to make valid uncompressed point
            let mut tmp = vec![0x04];
            tmp.extend_from_slice(pk);
            let pk = PublicKey::from_sec1_bytes(&tmp)
                .map_err(|e| format!("Invalid 64-byte pubkey: {}", e))?;
            let compressed_pk = pk.to_encoded_point(true).as_bytes().to_vec();
            Ok(compressed_pk)
        }
        65 => {
            let pk = PublicKey::from_sec1_bytes(pk)
                .map_err(|e| format!("Invalid 65-byte pubkey: {}", e))?;
            let compressed_pk = pk.to_encoded_point(true).as_bytes().to_vec();
            Ok(compressed_pk)
        }
        _ => Err(format!("unexpected pubkey length: {}", pk.len())),
    }
}

// Ethereum address: keccak256(x||y) -> last 20 bytes, return lower-hex (0x prefixed)
pub fn eth_address_from_pubkey(pubkey: &[u8]) -> Result<String, String> {
    let pk = normalize_evm_pubkey(pubkey)?;
    // keccak256 of the 64 bytes (x||y)
    let mut hasher = Keccak256::new();
    hasher.update(pk);
    let hash = hasher.finalize();
    let addr = &hash[12..]; // last 20 bytes
    Ok(format!("0x{}", HexBinary::from(addr).to_hex()))
}

pub fn cosmos_address_from_pubkey(pubkey: &[u8], prefix: &str) -> Result<String, String> {
    let pk = normalize_cosmos_pubkey(pubkey)?;
    // Some SDKs expect the compressed pubkey or a protobuf/pubkey wrapper.
    // Here we compute raw address bytes the simple way:
    let sha = Sha256::digest(&pk);
    let rip = Ripemd160::digest(sha);

    // bech32 encode
    let bech = encode(prefix, rip.to_base32(), Variant::Bech32)
        .map_err(|e| format!("bech32 encode failed: {}", e))?;
    Ok(bech)
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

    fn sign_messsage(msg: &str) -> (Binary, Binary) {
        let message_digest = Sha256::new().chain(msg.as_bytes());

        let (secret_key, public_key_bytes) = get_signer_key();
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

        let verified = verify_signature(deps.as_ref(), msg_str, &signature, &pub_key).unwrap();
        assert!(verified);
    }

    #[test]
    fn test_verify_evm_signature() {
        let prefix = "\x19Ethereum Signed Message:\n";
        // let prefix = HexBinary::from(prefix.as_bytes()).to_hex();
        let msg_str = "Hello, world!";
        let msg_length = msg_str.len().to_string();
        // let msg_length = HexBinary::from(msg_length.as_bytes()).to_hex();
        // let msg_str = HexBinary::from(msg_str.as_bytes()).to_hex();

        let combined_msg = format!("{}{}{}", prefix, msg_length, msg_str);
        println!("combined_msg: {:?}", combined_msg);

        let signature = "68b37e2f523d414669be3e57f2c8232d26e78e9a7c7ff2e2690b15b72d21b4f740babfa1e165920ee6f2a490de47ceb91760a72f2e2ec7fc4169f2d154198a1e1b";
        let signature = HexBinary::from_hex(signature).unwrap();

        println!("signature length: {:?}", signature.len());

        let pub_key = "044089a9fb9f67cdac85610900f61d69e2adc7e5da37036585955ca85d0ea148202a1e1750d26b825efb5c3e9aff92c6faf37bb1865c9b9612b064e89c6806e408";
        let pub_key = HexBinary::from_hex(pub_key).unwrap();
        let deps = mock_dependencies();

        let verified =
            verify_keccak256_signature(deps.as_ref(), &combined_msg, &signature, &pub_key).unwrap();
        assert!(verified);
    }

    #[test]
    fn test_cosmos_address_from_pubkey() {
        let (_secret_key, pub_key) = get_signer_key();
        let address = cosmos_address_from_pubkey(&pub_key, "euclid").unwrap();
        assert_eq!(address, "euclid1kn4rjnaa6yz9dh53ucep4n3ghghe3fss7u79t6");
    }

    #[test]
    fn test_cosmos_address_from_pubkey_external() {
        let pub_key = "A0WsDoywwf2uMav0l6fW0vS0TOMYjmBC5qszc7seHX17";
        let pub_key = Binary::from_base64(pub_key).unwrap();
        let address = cosmos_address_from_pubkey(&pub_key, "euclid").unwrap();
        assert_eq!(address, "euclid1uqt6umzd4z25zx8djq6yz9t88vujfkg76sq299");
    }

    #[test]
    fn test_eth_address_from_pubkey() {
        let pub_key = "044089a9fb9f67cdac85610900f61d69e2adc7e5da37036585955ca85d0ea148202a1e1750d26b825efb5c3e9aff92c6faf37bb1865c9b9612b064e89c6806e408";
        let pub_key = HexBinary::from_hex(pub_key).unwrap();
        let address = eth_address_from_pubkey(&pub_key).unwrap();
        assert_eq!(address, "0x20c863d309b5e56cd7502301b89b9223829fa7b5");
    }
}
