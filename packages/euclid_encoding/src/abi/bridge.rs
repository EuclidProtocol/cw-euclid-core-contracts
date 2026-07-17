use alloy_sol_types::private::U256 as SolU256;
use cosmwasm_std::{Uint128, Uint256, Uint64};

use crate::error::EncodingError;

pub fn uint256_to_sol(v: &Uint256) -> SolU256 {
    SolU256::from_be_bytes(v.to_be_bytes())
}

pub fn uint256_from_sol(v: SolU256) -> Uint256 {
    Uint256::from_be_bytes(v.to_be_bytes::<32>())
}

// Uint128 rides as `uint128` (not widened to uint256): router/ack payloads
// use it for position_id, liquidity_delta, amount_0/1 requested.
pub fn uint128_to_sol(v: &Uint128) -> u128 {
    v.u128()
}

pub fn uint128_from_sol(v: u128) -> Uint128 {
    Uint128::new(v)
}

pub fn uint64_to_sol(v: &Uint64) -> u64 {
    v.u64()
}

pub fn uint64_from_sol(v: u64) -> Uint64 {
    Uint64::new(v)
}

/// Rebuilds a validated string newtype (Token, ChainUid) from its wire string
/// through its own serde Deserialize impl. This is the only way to construct
/// these types without triggering domain validation (their constructors are
/// private), and it makes ABI decode semantics identical to JSON decode
/// semantics: neither validates, both roundtrip exactly.
pub fn newtype_from_string<T: serde::de::DeserializeOwned>(
    type_name: &'static str,
    s: String,
) -> Result<T, EncodingError> {
    let json = cosmwasm_std::to_json_vec(&s).map_err(|e| EncodingError::AbiDecode {
        type_name,
        reason: e.to_string(),
    })?;
    cosmwasm_std::from_json(&json).map_err(|e| EncodingError::AbiDecode {
        type_name,
        reason: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Local stand-in for a validated string newtype like `euclid::token::Token`
    /// or `euclid::chain::ChainUid`: a private-constructor wrapper around
    /// `String` whose `Deserialize` impl is a plain passthrough (no
    /// validation). Exercises `newtype_from_string`'s contract, deserialize
    /// never validates, without pulling in `euclid`.
    #[derive(Debug, PartialEq, Eq, Clone, serde::Serialize, serde::Deserialize)]
    struct TestNewtype(String);

    impl TestNewtype {
        fn create(s: String) -> Result<Self, &'static str> {
            if s.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            {
                Ok(TestNewtype(s))
            } else {
                Err("invalid characters")
            }
        }
    }

    impl std::fmt::Display for TestNewtype {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    #[test]
    fn uint256_bridge_roundtrips_max() {
        let max = Uint256::MAX;
        let sol = uint256_to_sol(&max);
        assert_eq!(uint256_from_sol(sol), max);
    }

    #[test]
    fn uint256_bridge_roundtrips_zero() {
        let zero = Uint256::zero();
        let sol = uint256_to_sol(&zero);
        assert_eq!(uint256_from_sol(sol), zero);
    }

    #[test]
    fn uint128_bridge_roundtrips_max() {
        let max = Uint128::MAX;
        assert_eq!(uint128_from_sol(uint128_to_sol(&max)), max);
    }

    #[test]
    fn uint64_bridge_roundtrips_max() {
        let max = Uint64::MAX;
        assert_eq!(uint64_from_sol(uint64_to_sol(&max)), max);
    }

    #[test]
    fn newtype_from_string_does_not_validate() {
        // TestNewtype::create would reject spaces and uppercase-with-invalid
        // characters; newtype_from_string must accept it anyway (§7.7: ABI
        // decode never validates, exactly like JSON decode).
        let raw = "Not Valid! Token".to_string();
        assert!(TestNewtype::create(raw.clone()).is_err());

        let rebuilt: TestNewtype = newtype_from_string("TestNewtype", raw.clone()).unwrap();
        assert_eq!(rebuilt.to_string(), raw);
    }

    #[test]
    fn newtype_from_string_second_case_does_not_validate() {
        let raw = "Not-Valid-Chain".to_string();
        assert!(TestNewtype::create(raw.clone()).is_err());

        let rebuilt: TestNewtype = newtype_from_string("TestNewtype", raw.clone()).unwrap();
        assert_eq!(rebuilt.to_string(), raw);
    }
}
