//! §9.3: malformed JSON input, in three distinct ways, must all surface as
//! `EncodingError::JsonDecode` (never panic): truncated/unparseable JSON
//! syntax, a syntactically valid object whose externally-tagged enum key
//! doesn't match any known variant, and a `Uint256` value that isn't a quoted
//! string (§7.1: `Uint256`/`Uint128` ride the wire as decimal strings, never
//! bare JSON numbers). Decode targets are the wire mirror types.

use cosmwasm_std::Uint256;
use euclid_encoding::{EncodingError, JsonDecode};
use euclid_ibc::wire::envelope::factory::FactoryReceiveMsg;
use euclid_ibc::wire::envelope::router::RouterReceiveMsg;
use euclid_ibc::wire::envelope::AcknowledgementMsg;
use euclid_ibc::wire::msgs::RegisterFactoryAckMsg;

fn assert_json_decode_err<T: JsonDecode + std::fmt::Debug>(label: &str, bytes: &[u8]) {
    match T::from_json_bytes(bytes) {
        Err(EncodingError::JsonDecode { .. }) => {}
        Err(other) => panic!("{label}: wrong error variant: {other:?}"),
        Ok(decoded) => panic!("{label}: unexpectedly decoded Ok: {decoded:?}"),
    }
}

#[test]
fn truncated_json_is_rejected() {
    // Cut mid-object: an unterminated string and no closing braces.
    let truncated = br#"{"register_denom":{"sender":{"chain_uid":"cosmos","addre"#;
    assert_json_decode_err::<RouterReceiveMsg>("router truncated", truncated);

    let truncated_factory = br#"{"register_factory":{"chain_uid":"#;
    assert_json_decode_err::<FactoryReceiveMsg>("factory truncated", truncated_factory);

    let truncated_ack = br#"{"ok":{"factory_address":"a","chain_id":"#;
    assert_json_decode_err::<AcknowledgementMsg<RegisterFactoryAckMsg>>(
        "ack truncated",
        truncated_ack,
    );
}

#[test]
fn wrong_enum_key_is_rejected() {
    // Syntactically well-formed JSON, but the tag doesn't name any variant of
    // the externally-tagged enum.
    let wrong_key = br#"{"not_a_real_variant":{}}"#;
    assert_json_decode_err::<RouterReceiveMsg>("router wrong key", wrong_key);

    let wrong_key_factory = br#"{"not_a_real_variant":{}}"#;
    assert_json_decode_err::<FactoryReceiveMsg>("factory wrong key", wrong_key_factory);

    // The ack envelope only recognizes "ok" and "error".
    let wrong_key_ack = br#"{"maybe":{}}"#;
    assert_json_decode_err::<AcknowledgementMsg<RegisterFactoryAckMsg>>(
        "ack wrong key",
        wrong_key_ack,
    );
}

#[test]
fn non_string_uint256_is_rejected() {
    // §7.1: Uint256 must be a quoted decimal string; a bare JSON number is not
    // a legal wire value even though it is valid JSON syntax.
    let bare_number = br#"123456"#;
    assert_json_decode_err::<Uint256>("bare-number uint256", bare_number);

    // Also not legal: a JSON object or array in place of the string.
    let wrong_shape = br#"{"amount":123456}"#;
    assert_json_decode_err::<Uint256>("object-shaped uint256", wrong_shape);
}
