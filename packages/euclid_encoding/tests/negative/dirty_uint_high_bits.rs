//! §9.3 (strict ABI decode): a fixed-width uint word whose bits above the
//! type width are set ("dirty high bits") must be REJECTED on decode.
//!
//! Current behavior (RED): `from_abi_bytes` routes through alloy's
//! `abi_decode_params`, which silently masks the high bits and returns `Ok`.
//! Desired behavior (GREEN): the crate switches the decode entry to
//! `abi_decode_params_validate` (mod.rs:51), which rejects the word and the
//! error is mapped to `EncodingError::AbiDecode { type_name, reason }`.
//!
//! Expected error contract (already-existing variant):
//!   EncodingError::AbiDecode { type_name: "UintStrMsg", reason: <alloy TypeCheckFail text> }
//!
//! `euclid_encoding` sits below `euclid_ibc`, so the real wire messages that
//! carry `SolUint<64>` (e.g. `AddLiquiditySendMsg::slippage_tolerance_bps`)
//! cannot be imported here without a dependency cycle. This local `AbiMap`
//! type reproduces the exact `(SolUint<64>, dynamic)` head layout those
//! messages use, and exercises the same public `from_abi_bytes` path.

use alloy_sol_types::sol_data::{String as SolString, Uint as SolUint};
use alloy_sol_types::SolType;

use euclid_encoding::{AbiDecode, AbiEncode, AbiMap, EncodingError};

#[derive(Debug, PartialEq, Eq)]
struct UintStrMsg {
    n: u64,
    s: String,
}

impl AbiMap for UintStrMsg {
    type Sol = (SolUint<64>, SolString);

    fn type_name() -> &'static str {
        "UintStrMsg"
    }

    fn to_sol(&self) -> Result<<Self::Sol as SolType>::RustType, EncodingError> {
        Ok((self.n, self.s.clone()))
    }

    fn from_sol(sol: <Self::Sol as SolType>::RustType) -> Result<Self, EncodingError> {
        Ok(Self { n: sol.0, s: sol.1 })
    }
}

/// Head layout of `abi_encode_params((uint64, string))`:
///   word0 [bytes 0..32]  = the uint64, right-aligned (valid data in 24..32)
///   word1 [bytes 32..64] = offset to the string tail
/// so byte 0 is the most-significant byte of the uint64 slot: a canonical
/// `u64` must leave it (and all of bytes 0..24) zero.
#[test]
fn dirty_uint_high_bits_rejected() {
    let msg = UintStrMsg {
        n: 100,
        s: "hi".to_string(),
    };
    let mut bytes = msg.to_abi_bytes().unwrap();

    // MUTATION @ byte 0 (word0, top byte of the uint64 slot): set a bit far
    // above the 64-bit width. alloy's non-validating decode masks it away.
    bytes[0] = 0x01;

    match UintStrMsg::from_abi_bytes(&bytes) {
        Err(EncodingError::AbiDecode { type_name, .. }) => {
            assert_eq!(type_name, "UintStrMsg");
        }
        other => panic!(
            "expected AbiDecode error for dirty uint high bits, got {other:?} \
             (current lax decode silently masks the high bits and returns Ok)"
        ),
    }
}

/// Positive guard: a canonical maximal `u64` (all 64 low bits set, high bits
/// clear) roundtrips cleanly. This must keep passing after the fix.
#[test]
fn canonical_uint_roundtrips() {
    let msg = UintStrMsg {
        n: u64::MAX,
        s: "hi".to_string(),
    };
    let bytes = msg.to_abi_bytes().unwrap();
    assert_eq!(UintStrMsg::from_abi_bytes(&bytes).unwrap(), msg);
}
