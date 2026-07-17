use serde::{de::DeserializeOwned, Serialize};

use crate::error::EncodingError;
use crate::traits::{JsonDecode, JsonEncode};

// Blanket over everything serde-capable: identical bytes to the
// `to_json_binary(self)` calls the contracts make today.
impl<T: Serialize> JsonEncode for T {
    fn to_json_bytes(&self) -> Result<Vec<u8>, EncodingError> {
        cosmwasm_std::to_json_vec(self).map_err(|e| EncodingError::JsonEncode {
            type_name: core::any::type_name::<T>(),
            reason: e.to_string(),
        })
    }
}

impl<T: DeserializeOwned> JsonDecode for T {
    fn from_json_bytes(bytes: &[u8]) -> Result<Self, EncodingError> {
        cosmwasm_std::from_json(bytes).map_err(|e| EncodingError::JsonDecode {
            type_name: core::any::type_name::<T>(),
            reason: e.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde::Deserialize;

    use super::*;

    /// Local stand-in for a wire message: any serde-capable struct exercises
    /// the blanket `JsonEncode`/`JsonDecode` impls identically to a real
    /// domain type, since both blankets are generic over `T: Serialize` /
    /// `T: DeserializeOwned` with no further bound.
    #[derive(Debug, PartialEq, Eq, Clone, serde::Serialize, Deserialize)]
    struct TestMsg {
        name: String,
        value: u64,
    }

    fn sample() -> TestMsg {
        TestMsg {
            name: "hello".to_string(),
            value: 42,
        }
    }

    #[test]
    fn json_roundtrip_preserves_value() {
        let msg = sample();
        let bytes = msg.to_json_bytes().unwrap();
        let decoded = TestMsg::from_json_bytes(&bytes).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn json_decode_rejects_malformed_json() {
        // Truncated mid-object: unterminated string, no closing braces.
        let malformed = br#"{"name":"unterminated"#;
        match TestMsg::from_json_bytes(malformed) {
            Err(EncodingError::JsonDecode { type_name, .. }) => {
                assert_eq!(type_name, core::any::type_name::<TestMsg>());
            }
            other => panic!("wrong error variant: {other:?}"),
        }
    }

    #[test]
    fn json_encode_rejects_map_with_non_string_keys() {
        // JSON object keys must be strings; a `bool` key has no valid JSON
        // representation, so `to_json_bytes` must surface the underlying
        // serde error as `EncodingError::JsonEncode`, not panic.
        let mut map: BTreeMap<bool, i32> = BTreeMap::new();
        map.insert(true, 1);
        match map.to_json_bytes() {
            Err(EncodingError::JsonEncode { type_name, .. }) => {
                assert_eq!(type_name, core::any::type_name::<BTreeMap<bool, i32>>());
            }
            other => panic!("wrong error variant: {other:?}"),
        }
    }
}
