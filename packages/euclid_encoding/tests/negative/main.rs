//! Negative-test suite (plan §9.3): one file per failure class, compiled as a
//! single `tests/negative` integration-test binary (the `tests/<name>/main.rs`
//! auto-discovery pattern, mirroring `src/bin/<name>/main.rs` for binaries).

mod dirty_uint_high_bits;
mod invalid_encoding_tag;
mod invalid_utf8_string;
mod non_canonical_bool;
mod non_canonical_offsets;
mod option_none_nonempty_payload;
mod out_of_range_offset;
