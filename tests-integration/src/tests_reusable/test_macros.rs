#![allow(unused_imports)]

use rstest::rstest;
use rstest_reuse::template;

use super::constants::{FACTORY_CHAIN_ID_EVM, FACTORY_CHAIN_ID_IBC, FACTORY_CHAIN_ID_LOCAL};
use super::factory_register::FactorySetupMode;

#[template]
#[rstest]
pub fn decimal_pair(
    #[values(0, 6, 8, 18, 24)] decimals_a: u32,
    #[values(0, 6, 8, 18, 24)] decimals_b: u32,
) {
}

#[template]
#[rstest]
pub fn decimal_pair_full(
    #[values(
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24
    )]
    decimals_a: u32,
    #[values(
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24
    )]
    decimals_b: u32,
) {
}

#[template]
#[rstest]
pub fn single_decimal(#[values(0, 6, 8, 18, 24)] decimals: u32) {}

#[template]
#[rstest]
pub fn single_decimal_full(
    #[values(
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24
    )]
    decimals: u32,
) {
}

#[template]
#[rstest]
pub fn factory_modes(
    #[values(FactorySetupMode::Native, FactorySetupMode::Ibc, FactorySetupMode::Evm)]
    mode: FactorySetupMode,
) {
}
