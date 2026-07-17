//! Negative-test suite (plan §9.3): one file per failure class, compiled as a
//! single `tests/negative` integration-test binary (the `tests/<name>/main.rs`
//! auto-discovery pattern, mirroring `src/bin/<name>/main.rs` for binaries).
//! `common` is the same shared sample-builder tree the other integration
//! tests use; it lives one directory up (`tests/common`), so it is pulled in
//! by explicit path rather than by the default `tests/negative/common.rs`
//! lookup.

#[path = "../common/mod.rs"]
mod common;

mod json_malformed;
mod nonempty_unit_payload;
mod truncated;
mod wrong_discriminant;
