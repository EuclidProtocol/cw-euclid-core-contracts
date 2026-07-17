/// Errors are data, not strings: negative tests match on variants.
#[derive(Debug, PartialEq, thiserror::Error)]
pub enum EncodingError {
    #[error("invalid encoding tag: {tag}")]
    InvalidEncodingTag { tag: u8 },

    #[error("json encode failed for {type_name}: {reason}")]
    JsonEncode {
        type_name: &'static str,
        reason: String,
    },

    #[error("json decode failed for {type_name}: {reason}")]
    JsonDecode {
        type_name: &'static str,
        reason: String,
    },

    #[error("abi decode failed for {type_name}: {reason}")]
    AbiDecode {
        type_name: &'static str,
        reason: String,
    },

    #[error("unknown discriminant {discriminant} for {type_name}")]
    UnknownDiscriminant {
        type_name: &'static str,
        discriminant: u8,
    },

    #[error("expected empty payload for {type_name}, got {len} bytes")]
    NonEmptyPayload { type_name: &'static str, len: usize },

    #[error(
        "non-canonical abi encoding for {type_name}: re-encoding the decoded value does not \
         reproduce the input bytes"
    )]
    NonCanonicalEncoding { type_name: &'static str },

    #[error("non-canonical option for {type_name}: some=false but the value slot is non-default")]
    NonCanonicalOption { type_name: &'static str },

    #[error("invalid {expected} representation: {reason}")]
    InvalidRepresentation {
        expected: &'static str,
        reason: String,
    },
}
