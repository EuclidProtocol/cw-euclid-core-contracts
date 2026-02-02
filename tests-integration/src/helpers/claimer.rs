use cosmwasm_std::{to_json_binary, to_json_string, Binary};
use cw_orch::mock::cw_multi_test::App;
use euclid::msgs::claimer::msg::{ClaimVoucherData, SignedTransaction};
use k256::{
    ecdsa::SigningKey,
    elliptic_curve::{rand_core, NonZeroScalar},
};
use relayer::verify::{MsgSignData, MsgSignDataMsg, MsgSignDataValue};
use sha2::{digest::Update, Digest, Sha256};

pub fn get_claimer_key() -> (SigningKey, Binary) {
    let new_private_key = NonZeroScalar::random(&mut rand_core::OsRng);

    let signer_key = SigningKey::from(new_private_key);
    let pubkey = signer_key
        .verifying_key()
        .to_encoded_point(false)
        .as_bytes()
        .to_vec();

    let pubkey_binary = Binary::from(pubkey);
    (signer_key, pubkey_binary)
}

pub fn sign_claim_messsage(
    signer_key: SigningKey,
    claim_msg: ClaimVoucherData,
    app: &App,
) -> SignedTransaction {
    let msg = MsgSignDataMsg::new(MsgSignDataValue::new(
        to_json_binary(&claim_msg).unwrap(),
        format!("claimer_{}", app.block_info().chain_id),
    ));
    let msg = MsgSignData::new(vec![msg]);
    let msg = to_json_string(&msg).unwrap();
    let message_digest = Sha256::new().chain(msg.as_bytes());

    let signature = signer_key
        .sign_digest_recoverable(message_digest)
        .unwrap()
        .0;
    SignedTransaction {
        data: msg,
        signature: Binary::from(signature.to_vec()),
    }
}
