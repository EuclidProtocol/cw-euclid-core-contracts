use alloy_sol_types::private::Bytes;
use alloy_sol_types::sol_data::{Bool, Bytes as SolBytes};

use crate::error::EncodingError;
use crate::traits::{AbiDecode, AbiEncode};

/// Primitive options ride as (bool some, T value); None encodes (false, T::default()).
/// Used for: Option<u32>, Option<u64>, Option<i64>, Option<bool>, Option<Uint128>,
/// Option<String>. The bool alone is authoritative on decode; the value slot of a
/// (false, _) pair is ignored and must be written as the default.
///
/// Composite options (structs/enums with domain invariants: Option<PoolKey>,
/// Option<CrossChainUser>) ride as (bool some, bytes inner) where inner is the
/// nested ABI params encoding when some, and empty bytes when none. This avoids
/// fabricating default composite values (an all-empty PoolKey is not a value the
/// domain can represent).
pub type OptPrimSol<T> = (Bool, T);
pub type OptDynSol = (Bool, SolBytes);

pub fn opt_dyn_to_sol<T>(v: &Option<T>) -> Result<(bool, Bytes), EncodingError>
where
    T: AbiEncode,
{
    match v {
        Some(inner) => Ok((true, inner.to_abi_bytes()?.into())),
        None => Ok((false, Bytes::new())),
    }
}

pub fn opt_dyn_from_sol<T>(sol: (bool, Bytes)) -> Result<Option<T>, EncodingError>
where
    T: AbiDecode,
{
    let (some, data) = sol;
    if some {
        Ok(Some(T::from_abi_bytes(&data)?))
    } else if data.is_empty() {
        Ok(None)
    } else {
        // A `None` discriminant with a non-empty inner payload is a smuggling
        // channel: the value would be silently dropped on decode. Reject it,
        // reusing the same guard `tagged::ensure_empty` produces. The composite
        // element type is only bound by `AbiDecode` (no name), so the payload
        // slot is named generically.
        Err(EncodingError::NonEmptyPayload {
            type_name: "Option",
            len: data.len(),
        })
    }
}

/// Strict decode for primitive options `(bool some, T value)`.
///
/// `None` is canonically `(false, T::default())`. A `(false, <non-default>)`
/// pair smuggles a value that the loose `some.then_some(value)` pattern would
/// silently discard, so it is rejected here. Callers that widen the raw
/// `SolType` value into a domain type map over the returned `Option<T>`.
pub fn opt_prim_from_sol<T>(
    type_name: &'static str,
    sol: (bool, T),
) -> Result<Option<T>, EncodingError>
where
    T: Default + PartialEq,
{
    let (some, value) = sol;
    if some {
        Ok(Some(value))
    } else if value == T::default() {
        Ok(None)
    } else {
        Err(EncodingError::NonCanonicalOption { type_name })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_sol_types::sol_data::String as SolString;
    use alloy_sol_types::SolType;

    use crate::abi::AbiMap;

    /// Local composite stand-in for a real domain struct (e.g.
    /// `euclid::cross_chain_user::CrossChainUser`): any `AbiMap`-implementing
    /// struct exercises the same `(bool some, bytes inner)` dynamic-option
    /// wire shape this module implements; the fields themselves are
    /// arbitrary and carry no domain invariants.
    #[derive(Debug, PartialEq, Eq, Clone)]
    struct TestComposite {
        chain: String,
        address: String,
    }

    impl AbiMap for TestComposite {
        type Sol = (SolString, SolString);

        fn type_name() -> &'static str {
            "TestComposite"
        }

        fn to_sol(&self) -> Result<<Self::Sol as SolType>::RustType, EncodingError> {
            Ok((self.chain.clone(), self.address.clone()))
        }

        fn from_sol(sol: <Self::Sol as SolType>::RustType) -> Result<Self, EncodingError> {
            let (chain, address) = sol;
            Ok(TestComposite { chain, address })
        }
    }

    fn sample_composite() -> TestComposite {
        TestComposite {
            chain: "cosmos".to_string(),
            address: "cosmos1abc".to_string(),
        }
    }

    #[test]
    fn opt_dyn_none_roundtrip() {
        let none: Option<TestComposite> = None;
        let sol = opt_dyn_to_sol(&none).unwrap();
        assert!(!sol.0);
        assert!(sol.1.is_empty());
        let back: Option<TestComposite> = opt_dyn_from_sol(sol).unwrap();
        assert_eq!(back, None);
    }

    #[test]
    fn opt_dyn_some_roundtrip() {
        let some = Some(sample_composite());
        let sol = opt_dyn_to_sol(&some).unwrap();
        assert!(sol.0);
        assert!(!sol.1.is_empty());
        let back: Option<TestComposite> = opt_dyn_from_sol(sol).unwrap();
        assert_eq!(back, some);
    }
}
