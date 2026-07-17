//! §9.3 (strict ABI decode): a `string` payload that is not valid UTF-8 must
//! be REJECTED on decode, not silently repaired.
//!
//! Current behavior (RED): `from_abi_bytes` routes through alloy's
//! `abi_decode_params`, which decodes the bytes lossily (invalid sequences
//! become the U+FFFD replacement character) and returns `Ok`. Desired
//! behavior (GREEN): the crate switches the decode entry to
//! `abi_decode_params_validate` (mod.rs:51), which rejects non-UTF-8 bytes and
//! the error is mapped to `EncodingError::AbiDecode { type_name, reason }`.
//!
//! Expected error contract (already-existing variant):
//!   EncodingError::AbiDecode { type_name: "StrMsg", reason: <alloy TypeCheckFail text> }

use alloy_sol_types::sol_data::String as SolString;
use alloy_sol_types::SolType;

use euclid_encoding::{AbiDecode, AbiEncode, AbiMap, EncodingError};

#[derive(Debug, PartialEq, Eq)]
struct StrMsg {
    s: String,
}

impl AbiMap for StrMsg {
    type Sol = (SolString,);

    fn type_name() -> &'static str {
        "StrMsg"
    }

    fn to_sol(&self) -> Result<<Self::Sol as SolType>::RustType, EncodingError> {
        Ok((self.s.clone(),))
    }

    fn from_sol(sol: <Self::Sol as SolType>::RustType) -> Result<Self, EncodingError> {
        Ok(Self { s: sol.0 })
    }
}

/// Layout of `abi_encode_params((string,))` for "hello":
///   word0 [bytes 0..32]  = offset to the string (0x20)
///   word1 [bytes 32..64] = length (5)
///   word2 [bytes 64..96] = "hello" data, left-aligned (byte 64 = 'h')
#[test]
fn invalid_utf8_string_rejected() {
    let msg = StrMsg {
        s: "hello".to_string(),
    };
    let mut bytes = msg.to_abi_bytes().unwrap();

    // MUTATION @ byte 64 (first data byte of the string): 0xFF is never a
    // valid UTF-8 leading byte. Lax decode replaces it with U+FFFD.
    bytes[64] = 0xff;

    match StrMsg::from_abi_bytes(&bytes) {
        Err(EncodingError::AbiDecode { type_name, .. }) => {
            assert_eq!(type_name, "StrMsg");
        }
        other => panic!(
            "expected AbiDecode error for invalid UTF-8, got {other:?} \
             (current lax decode substitutes U+FFFD and returns Ok)"
        ),
    }
}

/// Positive guard: valid UTF-8, including multi-byte code points, roundtrips.
#[test]
fn valid_utf8_string_roundtrips() {
    let msg = StrMsg {
        // ASCII + a multi-byte code point (é = 0xC3 0xA9).
        s: "h\u{00e9}llo".to_string(),
    };
    let bytes = msg.to_abi_bytes().unwrap();
    assert_eq!(StrMsg::from_abi_bytes(&bytes).unwrap(), msg);
}
