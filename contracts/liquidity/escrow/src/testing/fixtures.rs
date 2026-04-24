#[cfg(test)]
use rstest::fixture;

#[cfg(test)]
use crate::testing::helpers::{initialized_deps, with_deposit_deps, MockDeps};

/// Fixture: contract instantiated with one allowed native denom, no deposits.
#[cfg(test)]
#[fixture]
pub fn initialized() -> MockDeps {
    initialized_deps()
}

/// Fixture: contract instantiated with 1_000 ueucl deposited.
#[cfg(test)]
#[fixture]
pub fn with_deposit() -> MockDeps {
    with_deposit_deps()
}
