//! §9.3 (strict ABI decode): a dynamic-field offset that points outside the
//! buffer must be REJECTED on decode.
//!
//! Status: this guard is ALREADY enforced by alloy's non-validating
//! `abi_decode_params` (it bounds-checks offsets and returns a "buffer
//! overrun" error), so this test PASSES on current code. It is kept as a
//! regression pin: the offset bounds check must survive the switch to
//! `abi_decode_params_validate`.
//!
//! Expected error contract (already-existing variant):
//!   EncodingError::AbiDecode { type_name: "StrMsg", reason: <alloy buffer-overrun text> }

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

/// Layout of `abi_encode_params((string,))`:
///   word0 [bytes 0..32] = offset to the string tail (canonically 0x20)
/// Overwrite that offset with a value far past the end of the buffer.
#[test]
fn out_of_range_offset_rejected() {
    let msg = StrMsg {
        s: "hello".to_string(),
    };
    let mut bytes = msg.to_abi_bytes().unwrap();

    // MUTATION @ word0 (bytes 0..32): zero it, then set byte 24 = 0x10 so the
    // offset reads ~0x10 << 56, pointing far beyond the ~96-byte buffer.
    for b in bytes.iter_mut().take(32) {
        *b = 0;
    }
    bytes[24] = 0x10;

    match StrMsg::from_abi_bytes(&bytes) {
        Err(EncodingError::AbiDecode { type_name, .. }) => {
            assert_eq!(type_name, "StrMsg");
        }
        other => panic!("expected AbiDecode error for out-of-range offset, got {other:?}"),
    }
}

/// Positive guard: a valid, in-range offset roundtrips.
#[test]
fn in_range_offset_roundtrips() {
    let msg = StrMsg {
        s: "hello".to_string(),
    };
    let bytes = msg.to_abi_bytes().unwrap();
    assert_eq!(StrMsg::from_abi_bytes(&bytes).unwrap(), msg);
}
