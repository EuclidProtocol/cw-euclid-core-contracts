//! §9.3 (strict ABI decode): an option whose discriminant is `false` (None)
//! must carry an EMPTY / ZERO value slot. A `(false, <non-empty/nonzero>)`
//! pair is a smuggling channel and must be REJECTED on decode.
//!
//! This applies to BOTH option shapes the crate defines in `abi::option`:
//!
//!  * Composite (dynamic) options `OptDynSol = (Bool some, SolBytes inner)`:
//!    `None` is `(false, 0x"")`; `(false, <non-empty bytes>)` must error.
//!  * Primitive options `OptPrimSol<T> = (Bool some, T value)`:
//!    `None` is `(false, T::default())`; `(false, <nonzero value>)` must error.
//!
//! Current behavior (RED): the decode side ignores the value slot when
//! `some == false` and returns `Ok(None)`, silently discarding the payload.
//!
//! Expected error contracts:
//!  * Composite: `EncodingError::NonEmptyPayload { type_name, len }` (existing
//!    variant, same one `tagged::ensure_empty` produces). The check belongs in
//!    `abi::option::opt_dyn_from_sol`, which this test drives through
//!    `from_abi_bytes`. `len` is the ignored inner-bytes length (32 here).
//!  * Primitive: a PROPOSED variant, e.g.
//!    `EncodingError::NonCanonicalOption { type_name, .. }` (does not yet
//!    exist; see return notes). Because it cannot be named without touching
//!    `src/`, the primitive test asserts `is_err()`; the fixer tightens it.

use alloy_sol_types::sol_data::Uint as SolUint;
use alloy_sol_types::SolType;

use euclid_encoding::abi::option::{
    opt_dyn_from_sol, opt_dyn_to_sol, opt_prim_from_sol, OptDynSol, OptPrimSol,
};
use euclid_encoding::{AbiDecode, AbiEncode, AbiMap, EncodingError};

// ---------------------------------------------------------------------------
// Composite (dynamic) option: (bool some, bytes inner)
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
struct Inner {
    n: u64,
}

impl AbiMap for Inner {
    type Sol = (SolUint<64>,);

    fn type_name() -> &'static str {
        "Inner"
    }

    fn to_sol(&self) -> Result<<Self::Sol as SolType>::RustType, EncodingError> {
        Ok((self.n,))
    }

    fn from_sol(sol: <Self::Sol as SolType>::RustType) -> Result<Self, EncodingError> {
        Ok(Self { n: sol.0 })
    }
}

#[derive(Debug, PartialEq, Eq)]
struct DynOptMsg {
    v: Option<Inner>,
}

impl AbiMap for DynOptMsg {
    type Sol = (OptDynSol,);

    fn type_name() -> &'static str {
        "DynOptMsg"
    }

    fn to_sol(&self) -> Result<<Self::Sol as SolType>::RustType, EncodingError> {
        Ok((opt_dyn_to_sol(&self.v)?,))
    }

    fn from_sol(sol: <Self::Sol as SolType>::RustType) -> Result<Self, EncodingError> {
        Ok(Self {
            v: opt_dyn_from_sol(sol.0)?,
        })
    }
}

/// Encode `Some(Inner{9})`, then flip the discriminant to `false` while
/// leaving the (now-orphaned) 32-byte inner payload in place.
///
/// Layout of `abi_encode_params(((bool, bytes),))`:
///   word0 [bytes 0..32]   = offset to the option tuple (0x20)
///   -- option tuple block starts at byte 32 --
///   word1 [bytes 32..64]  = `some` discriminant (1)   <-- flip low byte @ 63
///   word2 [bytes 64..96]  = offset to inner bytes (relative, 0x40)
///   word3 [bytes 96..128] = inner bytes length (0x20 = 32)
///   word4 [bytes 128..160]= inner bytes data (encodes Inner{9})
#[test]
fn none_with_nonempty_composite_payload_rejected() {
    let msg = DynOptMsg {
        v: Some(Inner { n: 9 }),
    };
    let mut bytes = msg.to_abi_bytes().unwrap();

    // Discriminant lives at the start of the tuple block; word0 tells us where
    // that block is (0x20). Assert the expected offset, then flip `some`->0.
    assert_eq!(bytes[31], 0x20, "unexpected option-tuple offset");
    // MUTATION @ byte 63 (low byte of the `some` discriminant word).
    bytes[63] = 0x00;

    match DynOptMsg::from_abi_bytes(&bytes) {
        Err(EncodingError::NonEmptyPayload { .. }) => {}
        other => panic!(
            "expected NonEmptyPayload for (false, non-empty inner bytes), got {other:?} \
             (current decode returns Ok(None), discarding the smuggled payload)"
        ),
    }
}

/// Positive guard: canonical `None` = `(false, empty bytes)` decodes to None.
#[test]
fn composite_none_roundtrips() {
    let msg = DynOptMsg { v: None };
    let bytes = msg.to_abi_bytes().unwrap();
    assert_eq!(DynOptMsg::from_abi_bytes(&bytes).unwrap(), msg);
}

/// Positive guard: `Some` roundtrips unchanged.
#[test]
fn composite_some_roundtrips() {
    let msg = DynOptMsg {
        v: Some(Inner { n: 9 }),
    };
    let bytes = msg.to_abi_bytes().unwrap();
    assert_eq!(DynOptMsg::from_abi_bytes(&bytes).unwrap(), msg);
}

// ---------------------------------------------------------------------------
// Primitive option: (bool some, uint128 value)
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
struct PrimOptMsg {
    v: Option<u128>,
}

impl AbiMap for PrimOptMsg {
    type Sol = (OptPrimSol<SolUint<128>>,);

    fn type_name() -> &'static str {
        "PrimOptMsg"
    }

    fn to_sol(&self) -> Result<<Self::Sol as SolType>::RustType, EncodingError> {
        // Mirrors the inline pattern real wire types use for primitive options.
        Ok(((self.v.is_some(), self.v.unwrap_or_default()),))
    }

    fn from_sol(sol: <Self::Sol as SolType>::RustType) -> Result<Self, EncodingError> {
        // Mirrors the strict helper real wire types route their inline
        // primitive-option decode through.
        Ok(Self {
            v: opt_prim_from_sol("PrimOptMsg", sol.0)?,
        })
    }
}

/// Encode `Some(7)`, then flip the discriminant to `false`, leaving the
/// nonzero value slot in place.
///
/// Layout of `abi_encode_params(((bool, uint128),))` (fully static, inline):
///   word0 [bytes 0..32]  = `some` discriminant (1)  <-- flip low byte @ 31
///   word1 [bytes 32..64] = the uint128 value (7)
#[test]
fn none_with_nonzero_primitive_payload_rejected() {
    let msg = PrimOptMsg { v: Some(7) };
    let mut bytes = msg.to_abi_bytes().unwrap();

    // MUTATION @ byte 31 (low byte of the `some` discriminant word).
    bytes[31] = 0x00;

    match PrimOptMsg::from_abi_bytes(&bytes) {
        Err(EncodingError::NonCanonicalOption { type_name }) => {
            assert_eq!(type_name, "PrimOptMsg");
        }
        other => panic!(
            "expected NonCanonicalOption for (false, nonzero value), got {other:?} \
             (a lax decode would return Ok(None), discarding the smuggled value)"
        ),
    }
}

/// Positive guard: canonical `None` = `(false, 0)` decodes to None.
#[test]
fn primitive_none_roundtrips() {
    let msg = PrimOptMsg { v: None };
    let bytes = msg.to_abi_bytes().unwrap();
    assert_eq!(PrimOptMsg::from_abi_bytes(&bytes).unwrap(), msg);
}

/// Positive guard: `Some` roundtrips unchanged.
#[test]
fn primitive_some_roundtrips() {
    let msg = PrimOptMsg { v: Some(7) };
    let bytes = msg.to_abi_bytes().unwrap();
    assert_eq!(PrimOptMsg::from_abi_bytes(&bytes).unwrap(), msg);
}
