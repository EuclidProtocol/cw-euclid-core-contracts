use crate::encoding::Encoding;
use crate::error::EncodingError;

/// Json (0): the payload IS the JSON text, returned as a UTF-8 String.
/// Abi (1): canonical 0x-prefixed lowercase hex of the payload bytes.
pub fn to_transport_string(bytes: &[u8], encoding: Encoding) -> Result<String, EncodingError> {
    match encoding {
        Encoding::Json => {
            String::from_utf8(bytes.to_vec()).map_err(|e| EncodingError::InvalidRepresentation {
                expected: "json text",
                reason: e.to_string(),
            })
        }
        Encoding::Abi => Ok(format!("0x{}", hex::encode(bytes))),
    }
}

/// Inverse. Strict: Json requires nothing beyond UTF-8 (the bytes are the text);
/// Abi requires the 0x prefix and even length, but accepts hex digits in any case.
pub fn from_transport_string(s: &str, encoding: Encoding) -> Result<Vec<u8>, EncodingError> {
    match encoding {
        Encoding::Json => Ok(s.as_bytes().to_vec()),
        Encoding::Abi => {
            let hex_part = s
                .strip_prefix("0x")
                .ok_or(EncodingError::InvalidRepresentation {
                    expected: "0x hex",
                    reason: "missing 0x prefix".to_string(),
                })?;
            hex::decode(hex_part).map_err(|e| EncodingError::InvalidRepresentation {
                expected: "0x hex",
                reason: e.to_string(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_roundtrip_is_text_identity() {
        let bytes = br#"{"a":1}"#;
        let s = to_transport_string(bytes, Encoding::Json).unwrap();
        assert_eq!(s, r#"{"a":1}"#);
        assert_eq!(
            from_transport_string(&s, Encoding::Json).unwrap(),
            bytes.to_vec()
        );
    }

    #[test]
    fn abi_roundtrip_is_lowercase_0x_hex() {
        let bytes = [0xdeu8, 0xad, 0xbe, 0xef];
        let s = to_transport_string(&bytes, Encoding::Abi).unwrap();
        assert_eq!(s, "0xdeadbeef");
        assert_eq!(
            from_transport_string(&s, Encoding::Abi).unwrap(),
            bytes.to_vec()
        );
    }

    #[test]
    fn abi_decode_tolerates_uppercase_hex() {
        assert_eq!(
            from_transport_string("0xDEADbeEF", Encoding::Abi).unwrap(),
            vec![0xde, 0xad, 0xbe, 0xef]
        );
    }

    #[test]
    fn abi_decode_rejects_missing_prefix_and_odd_length() {
        assert!(matches!(
            from_transport_string("deadbeef", Encoding::Abi),
            Err(EncodingError::InvalidRepresentation {
                expected: "0x hex",
                ..
            })
        ));
        assert!(matches!(
            from_transport_string("0xdeadbee", Encoding::Abi),
            Err(EncodingError::InvalidRepresentation {
                expected: "0x hex",
                ..
            })
        ));
        assert!(matches!(
            from_transport_string("0xzz", Encoding::Abi),
            Err(EncodingError::InvalidRepresentation {
                expected: "0x hex",
                ..
            })
        ));
    }

    #[test]
    fn json_encode_rejects_non_utf8_bytes() {
        assert!(matches!(
            to_transport_string(&[0xff, 0xfe], Encoding::Json),
            Err(EncodingError::InvalidRepresentation {
                expected: "json text",
                ..
            })
        ));
    }

    #[test]
    fn empty_payload_forms() {
        assert_eq!(to_transport_string(&[], Encoding::Abi).unwrap(), "0x");
        assert_eq!(
            from_transport_string("0x", Encoding::Abi).unwrap(),
            Vec::<u8>::new()
        );
    }
}
