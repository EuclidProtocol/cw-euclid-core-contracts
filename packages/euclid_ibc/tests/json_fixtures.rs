//! Golden JSON fixtures (plan §9.2): pins the codec's JSON side to today's
//! exact wire behavior in two directions.
//!
//! 1. For representative samples across every message family, the codec's
//!    JSON bytes (`to_json_bytes`, i.e. the `JsonEncode` impl) must be
//!    byte-identical to `cosmwasm_std::to_json_binary` on the same value.
//!    This is the load-bearing parity: a codec that diverges from cw serde
//!    would surface here as a mismatch. A mismatch is an implementation bug,
//!    never a fixture update.
//! 2. Hand-written JSON literals, captured verbatim from the real serde output
//!    of the domain values, decode to the exact wire values today's contracts
//!    would produce: externally tagged enum keys, the ack envelope's
//!    `{"ok":...}`/`{"error":"..."}` shape, the `{"ok":[49]}` sentinel (checked
//!    against `AcknowledgementMsg::Ok(b"1")`'s own JSON bytes directly),
//!    quoted `Uint256` decimal strings, and `NextSwapPair.pool_key`'s two
//!    legal absence shapes (omitted key vs. explicit `null`, §7.9).

mod common;

use cosmwasm_std::{to_json_binary, Uint256};
use euclid::chain::ChainUid;
use euclid::cross_chain_user::CrossChainUser;
use euclid::msgs::factory::msg::RegisterFactoryResponse;
use euclid::msgs::vlp::base::{PoolKey, PoolType};
use euclid::swap::NextSwapPair;
use euclid::token::{Pair, Token};
use euclid_encoding::{JsonDecode, JsonEncode};
use euclid_ibc::wire::envelope::factory::FactoryReceiveMsg;
use euclid_ibc::wire::envelope::router::RouterReceiveMsg;
use euclid_ibc::wire::envelope::AcknowledgementMsg;
use euclid_ibc::wire::msgs::RegisterFactoryAckMsg;
use serde::Serialize;

use common::factory_samples;
use common::router_samples;
use common::types_samples::{chain_uid, token};

/// The wire mirror's codec JSON bytes must equal `to_json_binary` on the
/// domain value it was converted from. Any divergence is a wire/domain JSON
/// parity break in the mirror definition.
fn assert_wire_json_matches_domain<W, D>(label: &str, wire: &W, domain: &D)
where
    W: JsonEncode,
    D: Serialize,
{
    let codec_bytes = wire.to_json_bytes().expect("wire codec json encode");
    let domain_bytes = to_json_binary(domain)
        .expect("domain to_json_binary")
        .to_vec();
    assert_eq!(
        codec_bytes, domain_bytes,
        "wire JSON must equal domain to_json_binary bytes for {label}"
    );
}

#[test]
fn router_family_json_bytes_match_domain() {
    for (label, msg) in router_samples::all_samples() {
        assert_wire_json_matches_domain(label, &msg, &msg);
    }
}

#[test]
fn factory_family_json_bytes_match_domain() {
    for msg in factory_samples::all_samples() {
        assert_wire_json_matches_domain("factory-sample", &msg, &msg);
    }
}

#[test]
fn ack_family_json_bytes_match_domain() {
    for sample in common::ack_samples::register_factory_response_samples() {
        let domain_ok: AcknowledgementMsg<RegisterFactoryResponse> =
            AcknowledgementMsg::Ok(sample.clone());
        let wire_ok: AcknowledgementMsg<RegisterFactoryAckMsg> =
            AcknowledgementMsg::Ok(sample.into());
        assert_wire_json_matches_domain("register_factory_ok", &wire_ok, &domain_ok);

        let domain_err: AcknowledgementMsg<RegisterFactoryResponse> =
            AcknowledgementMsg::Error("boom".to_string());
        let wire_err: AcknowledgementMsg<RegisterFactoryAckMsg> =
            AcknowledgementMsg::Error("boom".to_string());
        assert_wire_json_matches_domain("register_factory_err", &wire_err, &domain_err);
    }

    for sample in common::ack_samples::add_liquidity_response_samples() {
        let domain_ok: AcknowledgementMsg<euclid::liquidity::AddLiquidityResponse> =
            AcknowledgementMsg::Ok(sample.clone());
        let wire_ok: AcknowledgementMsg<euclid_ibc::wire::msgs::AddLiquidityAckMsg> =
            AcknowledgementMsg::Ok(sample.into());
        assert_wire_json_matches_domain("add_liquidity_ok", &wire_ok, &domain_ok);
    }

    // The sentinel has no wire mirror; its JSON is pinned directly.
    for sample in common::ack_samples::sentinel_samples() {
        let ok: AcknowledgementMsg<Vec<u8>> = AcknowledgementMsg::Ok(sample);
        let codec = ok.to_json_bytes().expect("codec json");
        let cosmwasm = to_json_binary(&ok).expect("to_json_binary").to_vec();
        assert_eq!(codec, cosmwasm);
    }
}

/// §7.9/§9.2: externally tagged enum keys, exactly as `cw_serde`'s default
/// (snake_case, externally tagged) serde representation produces them today.
/// This literal was captured verbatim from `to_json_string` on the domain
/// `router_samples::register_denom_msg()`, and must decode into the wire
/// mirror and convert back to the same domain value.
#[test]
fn router_register_denom_literal_decodes_to_sample() {
    let literal = br#"{"register_denom":{"sender":{"chain_uid":"cosmos","address":"cosmos1abcdef"},"tx_id":"tx-register","token":{"token":"abc","token_type":{"native":{"denom":"uatom","decimals":6}}}}}"#;
    let decoded: RouterReceiveMsg =
        RouterReceiveMsg::from_json_bytes(literal).expect("decode literal");
    assert_eq!(decoded, router_samples::register_denom_msg());
}

/// Same externally tagged shape on the factory side, tag key
/// `"register_factory"`, captured verbatim from
/// `factory_samples::register_factory_samples()[0]`.
#[test]
fn factory_register_factory_literal_decodes_to_sample() {
    let literal = br#"{"register_factory":{"chain_uid":"chain0","chain_type":{"native":{"factory_address":"native1factory","factory_chain_id":"native"}},"tx_id":"tx-register-0"}}"#;
    let decoded: FactoryReceiveMsg =
        FactoryReceiveMsg::from_json_bytes(literal).expect("decode literal");
    assert_eq!(
        decoded,
        factory_samples::register_factory_samples()
            .into_iter()
            .next()
            .unwrap()
    );
}

/// The ack envelope's externally tagged `{"ok": ...}` shape, decoding into the
/// wire mirror payload.
#[test]
fn ack_ok_literal_decodes() {
    let literal = br#"{"ok":{"factory_address":"a","chain_id":"b"}}"#;
    let decoded: AcknowledgementMsg<RegisterFactoryAckMsg> =
        AcknowledgementMsg::from_json_bytes(literal).expect("decode literal");
    assert_eq!(
        decoded,
        AcknowledgementMsg::Ok(RegisterFactoryAckMsg {
            factory_address: "a".to_string(),
            chain_id: "b".to_string(),
        })
    );
}

/// The ack envelope's externally tagged `{"error": "..."}` shape.
#[test]
fn ack_error_literal_decodes() {
    let literal = br#"{"error":"boom"}"#;
    let decoded: AcknowledgementMsg<RegisterFactoryAckMsg> =
        AcknowledgementMsg::from_json_bytes(literal).expect("decode literal");
    assert_eq!(decoded, AcknowledgementMsg::Error("boom".to_string()));
}

/// §7.11: the sentinel success ack (`Ok(b"1")`) literal must decode to
/// `Ok(vec![b'1'])` and must equal `AcknowledgementMsg::Ok(b"1")`'s actual
/// bytes.
#[test]
fn sentinel_literal_matches_ok_ack_bytes() {
    let literal = br#"{"ok":[49]}"#;
    let decoded: AcknowledgementMsg<Vec<u8>> =
        AcknowledgementMsg::from_json_bytes(literal).expect("decode literal");
    assert_eq!(decoded, AcknowledgementMsg::Ok(vec![b'1']));

    let expected = to_json_binary(&AcknowledgementMsg::Ok(b"1")).unwrap();
    assert_eq!(literal.to_vec(), expected.to_vec());
}

/// §7.1: `Uint256` rides the wire as a quoted decimal string, never a bare
/// JSON number.
#[test]
fn uint256_literal_is_a_quoted_decimal_string() {
    let literal = br#""123456""#;
    let decoded = Uint256::from_json_bytes(literal).expect("decode quoted uint256");
    assert_eq!(decoded, Uint256::from(123_456u128));

    // Sanity: the encoder side round-trips through the same quoted form.
    let value = Uint256::from(123_456u128);
    let encoded = value.to_json_bytes().unwrap();
    assert_eq!(encoded, literal);

    // A bare (unquoted) number is not a legal Uint256 JSON value.
    let bare = br#"123456"#;
    assert!(Uint256::from_json_bytes(bare).is_err());
}

fn expected_next_swap_pair_without_pool_key() -> NextSwapPair {
    NextSwapPair {
        token_in: token("abc"),
        token_out: token("def"),
        pool_key: None,
        test_fail: Some(true),
    }
}

/// §7.9: `NextSwapPair.pool_key` is `#[serde(default, skip_serializing_if =
/// "Option::is_none")]`. On the encode side this makes the key vanish from the
/// JSON object when `None`. On the decode side both the omitted-key shape and
/// an explicit `"pool_key":null` must decode to the identical `None` value.
/// This is pure domain serde behavior (the wire mirror shares it by design).
#[test]
fn next_swap_pair_pool_key_omitted_and_explicit_null_decode_to_same_value() {
    let omitted = br#"{"token_in":"abc","token_out":"def","test_fail":true}"#;
    let decoded_omitted: NextSwapPair =
        NextSwapPair::from_json_bytes(omitted).expect("decode omitted pool_key");

    let explicit_null = br#"{"token_in":"abc","token_out":"def","pool_key":null,"test_fail":true}"#;
    let decoded_explicit: NextSwapPair =
        NextSwapPair::from_json_bytes(explicit_null).expect("decode explicit null pool_key");

    assert_eq!(decoded_omitted, decoded_explicit);
    assert_eq!(decoded_omitted, expected_next_swap_pair_without_pool_key());
}

/// The mirror image: when `pool_key` is `Some`, the encoder emits the key and
/// the literal with the key present decodes back to the same `Some` value.
#[test]
fn next_swap_pair_pool_key_present_literal_decodes() {
    let pool_key = PoolKey {
        pair: Pair::new(Token::create("abc".to_string()).unwrap(), token("def")).unwrap(),
        pool_type: PoolType::ConstantProduct {},
    };
    let value = NextSwapPair {
        token_in: token("abc"),
        token_out: token("def"),
        pool_key: Some(pool_key.clone()),
        test_fail: Some(false),
    };
    let encoded = value.to_json_bytes().unwrap();
    // The encoder must include the key when `Some` (only `None` is omitted).
    assert!(String::from_utf8_lossy(&encoded).contains("\"pool_key\""));

    let decoded: NextSwapPair = NextSwapPair::from_json_bytes(&encoded).unwrap();
    assert_eq!(decoded, value);
    assert_eq!(decoded.pool_key, Some(pool_key));
}

/// Sanity anchor for the `CrossChainUser`/`ChainUid` literal shapes used
/// above: `ChainUid` is a transparent string newtype and `CrossChainUser` is
/// a plain two-field object.
#[test]
fn cross_chain_user_literal_decodes() {
    let literal = br#"{"chain_uid":"cosmos","address":"cosmos1abcdef"}"#;
    let decoded: CrossChainUser = CrossChainUser::from_json_bytes(literal).unwrap();
    assert_eq!(
        decoded,
        CrossChainUser::new(chain_uid("cosmos"), "cosmos1abcdef".to_string())
    );

    let chain_uid_literal = br#""cosmos""#;
    let decoded_chain_uid: ChainUid = ChainUid::from_json_bytes(chain_uid_literal).unwrap();
    assert_eq!(decoded_chain_uid, chain_uid("cosmos"));
}
