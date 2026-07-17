//! §9.3 (strict ABI decode): a `bool` word must be canonical, i.e. exactly
//! `0` or `1`. Any other value (e.g. `2`) must be REJECTED on decode.
//!
//! alloy's validating decoder (`abi_decode_params_validate`) does NOT reject
//! this: it accepts `bool == 2` as `true` (empirically confirmed on alloy
//! 1.6.0). The crate therefore enforces full canonical-form equality in
//! `decode_params_canonical`: the decoded value is re-encoded and must
//! reproduce the input bytes exactly. Because the check is whole-buffer byte
//! equality rather than a positional word scan, it also covers bool words
//! shifted off the canonical word grid by gapped or overlapping tail offsets
//! (e.g. inside `Array` elements), so it genuinely protects every bool on the
//! wire: bare bools like `NextSwapPair::test_fail`'s value (which rides in a
//! `Vec` on the swap message) and every option `some` discriminant
//! (see `abi::option`).
//!
//! Expected error contract:
//!   EncodingError::NonCanonicalEncoding { type_name: "BoolStrMsg" }

use alloy_sol_types::sol_data::{Bool, String as SolString};
use alloy_sol_types::SolType;

use euclid_encoding::{AbiDecode, AbiEncode, AbiMap, EncodingError};

#[derive(Debug, PartialEq, Eq)]
struct BoolStrMsg {
    flag: bool,
    s: String,
}

impl AbiMap for BoolStrMsg {
    type Sol = (Bool, SolString);

    fn type_name() -> &'static str {
        "BoolStrMsg"
    }

    fn to_sol(&self) -> Result<<Self::Sol as SolType>::RustType, EncodingError> {
        Ok((self.flag, self.s.clone()))
    }

    fn from_sol(sol: <Self::Sol as SolType>::RustType) -> Result<Self, EncodingError> {
        Ok(Self {
            flag: sol.0,
            s: sol.1,
        })
    }
}

/// Head layout of `abi_encode_params((bool, string))`:
///   word0 [bytes 0..32]  = the bool, right-aligned (canonical byte at 31)
///   word1 [bytes 32..64] = offset to the string tail
#[test]
fn non_canonical_bool_two_rejected() {
    let msg = BoolStrMsg {
        flag: false,
        s: "hi".to_string(),
    };
    let mut bytes = msg.to_abi_bytes().unwrap();

    // MUTATION @ byte 31 (word0, low byte of the bool slot): 0x02 is neither
    // 0 nor 1. Lax decode reads it as `true`; a strict decoder must reject it.
    bytes[31] = 0x02;

    match BoolStrMsg::from_abi_bytes(&bytes) {
        Err(EncodingError::NonCanonicalEncoding { type_name }) => {
            assert_eq!(type_name, "BoolStrMsg");
        }
        other => panic!(
            "expected NonCanonicalEncoding error for bool == 2, got {other:?} \
             (a lax decode would coerce byte 2 to `true`)"
        ),
    }
}

/// Positive guard: canonical `false` (0) decodes to `false`.
#[test]
fn canonical_bool_false_roundtrips() {
    let msg = BoolStrMsg {
        flag: false,
        s: "hi".to_string(),
    };
    let bytes = msg.to_abi_bytes().unwrap();
    assert_eq!(BoolStrMsg::from_abi_bytes(&bytes).unwrap(), msg);
}

/// Positive guard: canonical `true` (1) decodes to `true`.
#[test]
fn canonical_bool_true_roundtrips() {
    let msg = BoolStrMsg {
        flag: true,
        s: "hi".to_string(),
    };
    let bytes = msg.to_abi_bytes().unwrap();
    assert_eq!(BoolStrMsg::from_abi_bytes(&bytes).unwrap(), msg);
}
