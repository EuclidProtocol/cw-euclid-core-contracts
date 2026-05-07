#![cfg(not(target_arch = "wasm32"))]

#[cfg(test)]
use rstest_reuse;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod helpers;

#[cfg(test)]
mod tests_reusable;
