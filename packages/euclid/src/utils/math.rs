use cosmwasm_std::{
    ConversionOverflowError, Decimal256, Fraction, StdError, StdResult, Uint128, Uint256,
};

/// Trait extension for Decimal256 to work with token precisions more accurately.
pub trait Decimal256Ext {
    fn to_uint256(&self) -> Uint256;

    fn to_uint128_with_precision(&self, precision: impl Into<u32>) -> StdResult<Uint128>;

    fn to_uint256_with_precision(&self, precision: impl Into<u32>) -> StdResult<Uint256>;

    fn checked_from_integer(i: impl Into<Uint256>) -> StdResult<Decimal256>;

    fn checked_multiply_ratio(
        &self,
        numerator: Decimal256,
        denominator: Decimal256,
    ) -> StdResult<Decimal256>;

    fn with_precision(
        value: impl Into<Uint256>,
        precision: impl Into<u32>,
    ) -> StdResult<Decimal256>;
}
impl Decimal256Ext for Decimal256 {
    fn to_uint256(&self) -> Uint256 {
        self.numerator() / self.denominator()
    }

    fn to_uint128_with_precision(&self, precision: impl Into<u32>) -> StdResult<Uint128> {
        let value = self.atomics();
        let precision = precision.into();

        value
            .checked_div(10u128.pow(self.decimal_places() - precision).into())?
            .try_into()
            .map_err(|o: ConversionOverflowError| {
                StdError::msg(format!("Error converting {}", o.target_type))
            })
    }

    fn to_uint256_with_precision(&self, precision: impl Into<u32>) -> StdResult<Uint256> {
        let value = self.atomics();
        let precision = precision.into();

        value
            .checked_div(10u128.pow(self.decimal_places() - precision).into())
            .map_err(|_| StdError::msg("DivideByZeroError"))
    }

    fn checked_from_integer(i: impl Into<Uint256>) -> StdResult<Decimal256> {
        Decimal256::checked_from_ratio(i.into(), 1u8)
            .map_err(|e| StdError::msg(e.to_string()))
    }

    fn checked_multiply_ratio(
        &self,
        numerator: Decimal256,
        denominator: Decimal256,
    ) -> StdResult<Decimal256> {
        Ok(Decimal256::new(
            self.atomics()
                .checked_multiply_ratio(numerator.atomics(), denominator.atomics())
                .map_err(|_| StdError::msg("CheckedMultiplyRatioError"))?,
        ))
    }

    fn with_precision(
        value: impl Into<Uint256>,
        precision: impl Into<u32>,
    ) -> StdResult<Decimal256> {
        Decimal256::from_atomics(value, precision.into())
            .map_err(|_| StdError::msg("Decimal256 range exceeded"))
    }
}
