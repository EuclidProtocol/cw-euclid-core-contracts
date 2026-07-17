// Sample builders are shared across several integration-test binaries; any one
// binary uses only a subset, so the rest look "unused" to that binary. This is
// the standard shared-`tests/common` idiom for silencing that cross-binary
// dead-code lint under `-D warnings`.
#![allow(dead_code)]

pub mod ack_samples;
pub mod factory_samples;
pub mod router_samples;
pub mod types_samples;
