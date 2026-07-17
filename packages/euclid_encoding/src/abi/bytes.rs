use alloy_sol_types::private::Bytes;
use alloy_sol_types::sol_data::Bytes as SolBytes;

use crate::abi::AbiMap;
use crate::error::EncodingError;

/// The sentinel success ack payload (`AcknowledgementMsg::Ok(b"1")`, see
/// `euclid_ibc::wire::envelope::ack`), modeled as `AcknowledgementMsg<Vec<u8>>`
/// so the JSON side stays byte-identical to `{"ok":[49]}` (§7.11) while the
/// ABI side gets a plain `bytes` shape.
impl AbiMap for Vec<u8> {
    type Sol = (SolBytes,);

    fn type_name() -> &'static str {
        "Vec<u8>"
    }

    fn to_sol(&self) -> Result<(Bytes,), EncodingError> {
        Ok((self.clone().into(),))
    }

    fn from_sol((bytes,): (Bytes,)) -> Result<Self, EncodingError> {
        Ok(bytes.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use crate::abi::{AbiDecode, AbiEncode};

    #[test]
    fn roundtrips_sentinel_byte() {
        let v = vec![b'1'];
        let encoded = v.to_abi_bytes().unwrap();
        assert_eq!(Vec::<u8>::from_abi_bytes(&encoded).unwrap(), v);
    }

    #[test]
    fn roundtrips_empty() {
        let v: Vec<u8> = Vec::new();
        let encoded = v.to_abi_bytes().unwrap();
        assert_eq!(Vec::<u8>::from_abi_bytes(&encoded).unwrap(), v);
    }
}
