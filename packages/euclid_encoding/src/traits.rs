use crate::encoding::Encoding;
use crate::error::EncodingError;

pub trait JsonEncode {
    fn to_json_bytes(&self) -> Result<Vec<u8>, EncodingError>;
}

pub trait JsonDecode: Sized {
    fn from_json_bytes(bytes: &[u8]) -> Result<Self, EncodingError>;
}

pub trait AbiEncode {
    fn to_abi_bytes(&self) -> Result<Vec<u8>, EncodingError>;
}

pub trait AbiDecode: Sized {
    fn from_abi_bytes(bytes: &[u8]) -> Result<Self, EncodingError>;
}

/// Single dispatch point (mirrors the POC's `encode`/`decode` free functions).
pub fn encode<T: JsonEncode + AbiEncode>(
    msg: &T,
    encoding: Encoding,
) -> Result<Vec<u8>, EncodingError> {
    match encoding {
        Encoding::Json => msg.to_json_bytes(),
        Encoding::Abi => msg.to_abi_bytes(),
    }
}

pub fn decode<T: JsonDecode + AbiDecode>(
    bytes: &[u8],
    encoding: Encoding,
) -> Result<T, EncodingError> {
    match encoding {
        Encoding::Json => T::from_json_bytes(bytes),
        Encoding::Abi => T::from_abi_bytes(bytes),
    }
}

#[cfg(test)]
mod tests {
    use alloy_sol_types::sol_data::{String as SolString, Uint as SolUint};
    use alloy_sol_types::SolType;
    use serde::{Deserialize, Serialize};

    use super::*;
    use crate::abi::AbiMap;

    /// Local stand-in for a wire message implementing both codecs: `AbiMap`
    /// (via the blanket `AbiEncode`/`AbiDecode` impls in `abi::mod`) and
    /// serde (via the blanket `JsonEncode`/`JsonDecode` impls in `json.rs`).
    /// Exercises the `encode`/`decode` dispatch fns exactly like a real
    /// `RouterCrossChainExecuteMsg` would, without pulling in `euclid_ibc`.
    #[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
    struct TestMsg {
        name: String,
        value: u64,
    }

    impl AbiMap for TestMsg {
        type Sol = (SolString, SolUint<64>);

        fn type_name() -> &'static str {
            "TestMsg"
        }

        fn to_sol(&self) -> Result<<Self::Sol as SolType>::RustType, EncodingError> {
            Ok((self.name.clone(), self.value))
        }

        fn from_sol(sol: <Self::Sol as SolType>::RustType) -> Result<Self, EncodingError> {
            let (name, value) = sol;
            Ok(TestMsg { name, value })
        }
    }

    fn sample() -> TestMsg {
        TestMsg {
            name: "hello".to_string(),
            value: 42,
        }
    }

    #[test]
    fn encode_json_tag_dispatches_to_json_codec() {
        let msg = sample();
        assert_eq!(
            encode(&msg, Encoding::Json).unwrap(),
            msg.to_json_bytes().unwrap()
        );
    }

    #[test]
    fn encode_abi_tag_dispatches_to_abi_codec() {
        let msg = sample();
        assert_eq!(
            encode(&msg, Encoding::Abi).unwrap(),
            msg.to_abi_bytes().unwrap()
        );
    }

    #[test]
    fn decode_json_tag_dispatches_to_json_codec() {
        let msg = sample();
        let bytes = msg.to_json_bytes().unwrap();
        let decoded: TestMsg = decode(&bytes, Encoding::Json).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn decode_abi_tag_dispatches_to_abi_codec() {
        let msg = sample();
        let bytes = msg.to_abi_bytes().unwrap();
        let decoded: TestMsg = decode(&bytes, Encoding::Abi).unwrap();
        assert_eq!(decoded, msg);
    }
}
