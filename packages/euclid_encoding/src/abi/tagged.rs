use alloy_sol_types::private::Bytes;
use alloy_sol_types::sol_data::{Bytes as SolBytes, Uint as SolUint};

use crate::error::EncodingError;

/// Every Rust enum crosses the wire as (uint8 tag, bytes payload), where
/// payload is the ABI params encoding of the active variant's fields and
/// empty bytes for unit variants. Tag values are declaration order from 0
/// and are frozen per the plan's tag tables; reordering a Rust enum without
/// updating the tag table is a wire break.
pub type TaggedSol = (SolUint<8>, SolBytes);

pub fn tagged(tag: u8, payload: Vec<u8>) -> (u8, Bytes) {
    (tag, payload.into())
}

/// Unit variants carry empty bytes. Raw alloy tuple decode tolerates
/// trailing bytes, so this is the only guard against garbage smuggled into
/// an otherwise-ignored payload slot.
pub fn ensure_empty(type_name: &'static str, data: &[u8]) -> Result<(), EncodingError> {
    if data.is_empty() {
        Ok(())
    } else {
        Err(EncodingError::NonEmptyPayload {
            type_name,
            len: data.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_sol_types::SolType;

    #[test]
    fn tagged_roundtrips_tag_and_payload() {
        let (tag, bytes) = tagged(7, vec![1, 2, 3]);
        assert_eq!(tag, 7);
        assert_eq!(bytes.as_ref(), &[1, 2, 3]);
    }

    #[test]
    fn ensure_empty_accepts_empty_and_rejects_nonempty() {
        assert!(ensure_empty("Unit", &[]).is_ok());
        let err = ensure_empty("Unit", &[9]).unwrap_err();
        assert_eq!(
            err,
            EncodingError::NonEmptyPayload {
                type_name: "Unit",
                len: 1
            }
        );
    }

    #[test]
    fn empty_tuple_sol_type_encodes_to_empty_bytes() {
        // Verified assumption from the plan: () implements SolType and its
        // abi_encode_params is empty bytes (mirrors Solidity abi.encode()).
        let encoded = <() as SolType>::abi_encode_params(&());
        assert!(encoded.is_empty());

        // Raw alloy tuple decode tolerates trailing bytes for a zero-field
        // tuple too (there is nothing to fail to decode). This is exactly
        // why `ensure_empty` above is the only real guard against garbage
        // smuggled into a unit variant's ignored payload slot.
        let decoded = <() as SolType>::abi_decode_params(&[1, 2, 3]);
        assert!(decoded.is_ok());
    }
}
