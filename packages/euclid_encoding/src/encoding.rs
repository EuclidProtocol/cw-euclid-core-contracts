use crate::error::EncodingError;

/// Wire encoding of a packet payload. The numeric value is the on-wire tag
/// (the `encoding` uint8 in the future SendPacketEncoded event).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Encoding {
    Json = 0,
    Abi = 1,
}

impl Encoding {
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    /// Strict: any tag other than 0 or 1 is an error (no reserved passthrough).
    pub fn from_u8(value: u8) -> Result<Self, EncodingError> {
        match value {
            0 => Ok(Encoding::Json),
            1 => Ok(Encoding::Abi),
            other => Err(EncodingError::InvalidEncodingTag { tag: other }),
        }
    }

    /// Lowercase label for CosmWasm event attributes ("json" / "abi").
    pub const fn as_str(self) -> &'static str {
        match self {
            Encoding::Json => "json",
            Encoding::Abi => "abi",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(0, Ok(Encoding::Json))]
    #[case(1, Ok(Encoding::Abi))]
    #[case(2, Err(EncodingError::InvalidEncodingTag { tag: 2 }))]
    #[case(255, Err(EncodingError::InvalidEncodingTag { tag: 255 }))]
    fn from_u8_table(#[case] tag: u8, #[case] expected: Result<Encoding, EncodingError>) {
        assert_eq!(Encoding::from_u8(tag), expected);
    }

    #[rstest]
    #[case(Encoding::Json, 0, "json")]
    #[case(Encoding::Abi, 1, "abi")]
    fn as_u8_and_as_str(
        #[case] encoding: Encoding,
        #[case] expected_u8: u8,
        #[case] expected_str: &str,
    ) {
        assert_eq!(encoding.as_u8(), expected_u8);
        assert_eq!(encoding.as_str(), expected_str);
        assert_eq!(Encoding::from_u8(encoding.as_u8()), Ok(encoding));
    }
}
