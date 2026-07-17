//! The single strict decode entry for ABI params bytes.
//!
//! alloy's `abi_decode_params_validate` range-checks every decoded token
//! (dirty integer high bits, invalid UTF-8, out-of-range offsets) but still
//! accepts non-canonical ENCODINGS: bool words above 1, and gapped or
//! overlapping tail offsets that shift dynamic elements off the canonical word
//! grid. Any such laundered layout is a smuggling channel.
//!
//! ABI encoding of a decoded value is unique and canonical, so re-encoding the
//! decoded value and requiring byte equality with the input rejects every
//! non-canonical form at once; canonical inputs always re-encode
//! byte-identical. Every decode of attacker-controlled bytes (top-level
//! messages and nested tagged/option payload slices alike) must route through
//! this function.

use alloy_sol_types::abi::token::TokenSeq;
use alloy_sol_types::SolType;

use crate::error::EncodingError;

pub fn decode_params_canonical<S>(
    type_name: &'static str,
    bytes: &[u8],
) -> Result<S::RustType, EncodingError>
where
    S: SolType,
    for<'a> S::Token<'a>: TokenSeq<'a>,
{
    let sol = S::abi_decode_params_validate(bytes).map_err(|e| EncodingError::AbiDecode {
        type_name,
        reason: e.to_string(),
    })?;
    if S::abi_encode_params(&sol) != bytes {
        return Err(EncodingError::NonCanonicalEncoding { type_name });
    }
    Ok(sol)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_sol_types::sol_data::{Bool, String as SolString};

    type PairSol = (Bool, SolString);

    fn canonical() -> Vec<u8> {
        <PairSol as SolType>::abi_encode_params(&(true, "hi".to_string()))
    }

    #[test]
    fn canonical_input_decodes() {
        let sol = decode_params_canonical::<PairSol>("Pair", &canonical()).unwrap();
        assert_eq!(sol, (true, "hi".to_string()));
    }

    #[test]
    fn non_canonical_bool_word_rejected() {
        let mut bytes = canonical();
        // Low byte of the bool word: 2 detokenizes to `true` but re-encodes as 1.
        bytes[31] = 0x02;
        assert_eq!(
            decode_params_canonical::<PairSol>("Pair", &bytes).unwrap_err(),
            EncodingError::NonCanonicalEncoding { type_name: "Pair" }
        );
    }

    #[test]
    fn gapped_tail_offset_rejected() {
        let mut bytes = canonical();
        // Head word 1 is the string offset (0x40). Inflate it by one word and
        // append a zero-word gap the decoder tolerates but re-encode removes.
        bytes[63] = 0x60;
        let mut gapped = bytes[..64].to_vec();
        gapped.extend_from_slice(&[0u8; 32]);
        gapped.extend_from_slice(&bytes[64..]);
        assert_eq!(
            decode_params_canonical::<PairSol>("Pair", &gapped).unwrap_err(),
            EncodingError::NonCanonicalEncoding { type_name: "Pair" }
        );
    }
}
