//! §9.3 (strict ABI decode): tail offsets must be canonical. alloy's
//! validating decoder accepts gapped (or overlapping) offsets, so a dynamic
//! element can be shifted off the canonical word grid; any word inside it
//! (here a `bool == 2`) then dodges every positional check while still
//! decoding `Ok`. The full canonical-equality check in
//! `decode_params_canonical` rejects this: re-encoding the decoded value
//! removes the gap and normalizes the bool, so the bytes cannot match.
//!
//! The shape mirrors the real reachable surface: `Recipient` (with its
//! `unsafe_refund_as_voucher` bool) and `NextSwapPair` (with its `test_fail`
//! bool) both ride in `Array`s on wire messages.
//!
//! Expected error contract:
//!   EncodingError::NonCanonicalEncoding { type_name: "ArrMsg" }

use alloy_sol_types::sol_data::{Array, Bool, String as SolString};
use alloy_sol_types::SolType;

use euclid_encoding::{AbiDecode, AbiEncode, AbiMap, EncodingError};

#[derive(Debug, PartialEq, Eq)]
struct ArrMsg {
    items: Vec<(String, bool)>,
}

impl AbiMap for ArrMsg {
    type Sol = (Array<(SolString, Bool)>,);

    fn type_name() -> &'static str {
        "ArrMsg"
    }

    fn to_sol(&self) -> Result<<Self::Sol as SolType>::RustType, EncodingError> {
        Ok((self.items.clone(),))
    }

    fn from_sol(sol: <Self::Sol as SolType>::RustType) -> Result<Self, EncodingError> {
        Ok(Self { items: sol.0 })
    }
}

fn one_element() -> ArrMsg {
    ArrMsg {
        items: vec![("hi".to_string(), true)],
    }
}

/// Canonical layout of `abi_encode_params((Array<(string, bool)>,))` for one
/// element ("hi", true):
///   word0 [bytes 0..32]    = offset to the array (0x20)
///   word1 [bytes 32..64]   = array length (1)
///   word2 [bytes 64..96]   = offset to element 0, relative to byte 64 (0x20)
///   -- element tuple block starts at byte 96 --
///   word3 [bytes 96..128]  = offset to the string, relative to byte 96 (0x40)
///   word4 [bytes 128..160] = the bool (canonical byte at 159)
///   word5 [bytes 160..192] = string length (2)
///   word6 [bytes 192..224] = "hi" data
///
/// Mutation: inflate the element offset by 3 words (0x20 -> 0x80), splice a
/// 3-word zero gap in front of the element block, and set the (shifted) bool
/// byte to 2. alloy's validating decoder follows the inflated offset and
/// accepts the buffer; a positional word-grid scan sees only zero words where
/// the bool used to be.
#[test]
fn gapped_element_offset_with_bool_two_rejected() {
    let canonical = one_element().to_abi_bytes().unwrap();
    assert_eq!(canonical.len(), 224, "unexpected canonical layout");
    assert_eq!(canonical[95], 0x20, "unexpected element-0 offset");
    assert_eq!(canonical[159], 0x01, "unexpected bool position");

    let mut bytes = canonical[..96].to_vec();
    bytes[95] = 0x80; // element-0 offset: 0x20 + 0x60 gap
    bytes.extend_from_slice(&[0u8; 96]); // 3-word zero-filled gap
    bytes.extend_from_slice(&canonical[96..]); // element block, shifted
    bytes[159 + 96] = 0x02; // the bool, now off the canonical grid

    match ArrMsg::from_abi_bytes(&bytes) {
        Err(EncodingError::NonCanonicalEncoding { type_name }) => {
            assert_eq!(type_name, "ArrMsg");
        }
        other => panic!(
            "expected NonCanonicalEncoding for a gap-inflated element offset \
             hiding bool == 2, got {other:?}"
        ),
    }
}

/// Positive guard: the canonical encoding of the same value decodes fine.
#[test]
fn canonical_array_roundtrips() {
    let msg = one_element();
    let bytes = msg.to_abi_bytes().unwrap();
    assert_eq!(ArrMsg::from_abi_bytes(&bytes).unwrap(), msg);
}
