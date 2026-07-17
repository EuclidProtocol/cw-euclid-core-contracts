pub mod bridge;
pub mod option;
pub mod params;
pub mod tagged;

mod bytes;

use alloy_sol_types::abi::token::TokenSeq;
use alloy_sol_types::SolType;

use crate::error::EncodingError;
use crate::traits::{AbiDecode, AbiEncode};

/// Bridges a domain type to its canonical flat ABI tuple.
///
/// INVARIANT (from the POC, its most important rule): encoding always goes
/// through `abi_encode_params` / `abi_decode_params` on `Self::Sol`, the flat
/// tuple form matching Solidity `abi.encode(field1, field2, ...)`. Never a
/// struct wrapper, which would prepend an offset word and desync the VMs.
///
/// The `TokenSeq` bound forces `Self::Sol` to be a tuple type (that's the
/// only shape `abi_encode_params`/`abi_decode_params` accept); single-field
/// types must use a 1-tuple, e.g. `(SolString,)`, never a bare `SolString`.
pub trait AbiMap: Sized
where
    for<'a> <Self::Sol as SolType>::Token<'a>: TokenSeq<'a>,
{
    /// The alloy SolType tuple describing this type's wire shape.
    type Sol: SolType;

    fn type_name() -> &'static str;
    fn to_sol(&self) -> Result<<Self::Sol as SolType>::RustType, EncodingError>;
    fn from_sol(sol: <Self::Sol as SolType>::RustType) -> Result<Self, EncodingError>;
}

impl<T> AbiEncode for T
where
    T: AbiMap,
    for<'a> <T::Sol as SolType>::Token<'a>: TokenSeq<'a>,
{
    fn to_abi_bytes(&self) -> Result<Vec<u8>, EncodingError> {
        Ok(<T::Sol as SolType>::abi_encode_params(&self.to_sol()?))
    }
}

impl<T> AbiDecode for T
where
    T: AbiMap,
    for<'a> <T::Sol as SolType>::Token<'a>: TokenSeq<'a>,
{
    fn from_abi_bytes(bytes: &[u8]) -> Result<Self, EncodingError> {
        T::from_sol(params::decode_params_canonical::<T::Sol>(
            T::type_name(),
            bytes,
        )?)
    }
}
