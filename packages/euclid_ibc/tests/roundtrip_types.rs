//! Roundtrip coverage for every shared type in plan §6.1 (17 types total).
//! Each test loops over a sample vector from `common::types_samples` covering
//! every enum variant and both `Some`/`None` for every optional field, and
//! asserts two independent roundtrips:
//!
//! 1. JSON, through the domain type's serde (the wire and domain JSON shapes
//!    are identical by design), via the public `JsonEncode`/`JsonDecode` API.
//! 2. The ABI mapping, through the crate's `wire::types::*_to_sol` /
//!    `*_from_sol` free functions (these replace the old `<Foo as AbiMap>`
//!    impls; the shared domain types no longer carry `AbiMap` themselves).
//!
//! The `register_factory_chain` case exercises all four
//! `RegisterFactoryChainType` arms (Native/Cosmos/Tvm/Evm), which nothing else
//! in the suite drives.

mod common;

use std::fmt::Debug;

use euclid_encoding::{JsonDecode, JsonEncode};

fn assert_json_roundtrip<T>(sample: &T)
where
    T: JsonEncode + JsonDecode + PartialEq + Debug,
{
    let bytes = sample.to_json_bytes().expect("json encode should not fail");
    let decoded = T::from_json_bytes(&bytes).expect("json decode should not fail");
    assert_eq!(&decoded, sample, "json roundtrip mismatch for {sample:?}");
}

/// One roundtrip test per shared type: JSON via the domain serde, ABI via the
/// wire `*_to_sol` / `*_from_sol` free functions.
macro_rules! type_roundtrip_test {
    ($name:ident, $samples:expr, $to_sol:path, $from_sol:path) => {
        #[test]
        fn $name() {
            let samples = $samples;
            assert!(!samples.is_empty(), "sample list must not be empty");
            for sample in samples {
                assert_json_roundtrip(&sample);

                let sol = $to_sol(&sample).expect("to_sol should not fail");
                let back = $from_sol(sol).expect("from_sol should not fail");
                assert_eq!(back, sample, "abi map roundtrip mismatch for {sample:?}");
            }
        }
    };
}

use euclid_ibc::wire::types::{
    chain_uid, cross_chain_user, limit, next_swap_pair, pair, pair_with_amount,
    pair_with_denom_and_amount, pool_config, pool_key, pool_type, recipient,
    register_factory_chain, token, token_type, token_with_amount, token_with_denom,
    token_with_denom_and_amount,
};

type_roundtrip_test!(
    token_roundtrips,
    common::types_samples::token_samples(),
    token::token_to_sol,
    token::token_from_sol
);

type_roundtrip_test!(
    chain_uid_roundtrips,
    common::types_samples::chain_uid_samples(),
    chain_uid::chain_uid_to_sol,
    chain_uid::chain_uid_from_sol
);

type_roundtrip_test!(
    cross_chain_user_roundtrips,
    common::types_samples::cross_chain_user_samples(),
    cross_chain_user::cross_chain_user_to_sol,
    cross_chain_user::cross_chain_user_from_sol
);

type_roundtrip_test!(
    pair_roundtrips,
    common::types_samples::pair_samples(),
    pair::pair_to_sol,
    pair::pair_from_sol
);

type_roundtrip_test!(
    token_type_roundtrips_all_variants,
    common::types_samples::token_type_samples(),
    token_type::token_type_to_sol,
    token_type::token_type_from_sol
);

type_roundtrip_test!(
    token_with_denom_roundtrips,
    common::types_samples::token_with_denom_samples(),
    token_with_denom::token_with_denom_to_sol,
    token_with_denom::token_with_denom_from_sol
);

type_roundtrip_test!(
    token_with_amount_roundtrips,
    common::types_samples::token_with_amount_samples(),
    token_with_amount::token_with_amount_to_sol,
    token_with_amount::token_with_amount_from_sol
);

type_roundtrip_test!(
    token_with_denom_and_amount_roundtrips,
    common::types_samples::token_with_denom_and_amount_samples(),
    token_with_denom_and_amount::token_with_denom_and_amount_to_sol,
    token_with_denom_and_amount::token_with_denom_and_amount_from_sol
);

type_roundtrip_test!(
    pair_with_amount_roundtrips,
    common::types_samples::pair_with_amount_samples(),
    pair_with_amount::pair_with_amount_to_sol,
    pair_with_amount::pair_with_amount_from_sol
);

type_roundtrip_test!(
    pair_with_denom_and_amount_roundtrips,
    common::types_samples::pair_with_denom_and_amount_samples(),
    pair_with_denom_and_amount::pair_with_denom_and_amount_to_sol,
    pair_with_denom_and_amount::pair_with_denom_and_amount_from_sol
);

type_roundtrip_test!(
    limit_roundtrips_all_variants,
    common::types_samples::limit_samples(),
    limit::limit_to_sol,
    limit::limit_from_sol
);

type_roundtrip_test!(
    recipient_roundtrips,
    common::types_samples::recipient_samples(),
    recipient::recipient_to_sol,
    recipient::recipient_from_sol
);

type_roundtrip_test!(
    next_swap_pair_roundtrips_both_pool_key_shapes,
    common::types_samples::next_swap_pair_samples(),
    next_swap_pair::next_swap_pair_to_sol,
    next_swap_pair::next_swap_pair_from_sol
);

type_roundtrip_test!(
    pool_type_roundtrips_all_variants,
    common::types_samples::pool_type_samples(),
    pool_type::pool_type_to_sol,
    pool_type::pool_type_from_sol
);

type_roundtrip_test!(
    pool_key_roundtrips,
    common::types_samples::pool_key_samples(),
    pool_key::pool_key_to_sol,
    pool_key::pool_key_from_sol
);

type_roundtrip_test!(
    pool_config_roundtrips_all_variants,
    common::types_samples::pool_config_samples(),
    pool_config::pool_config_to_sol,
    pool_config::pool_config_from_sol
);

type_roundtrip_test!(
    register_factory_chain_roundtrips_all_variants,
    common::types_samples::register_factory_chain_samples(),
    register_factory_chain::register_factory_chain_type_to_sol,
    register_factory_chain::register_factory_chain_type_from_sol
);

/// §9.4-style cross-encoding sanity, scoped to a representative shared type:
/// the JSON serde bytes and the ABI-mapped bytes actually differ (guards
/// against an impl silently delegating to the wrong path).
#[test]
fn json_and_abi_encodings_are_distinct_for_nonempty_types() {
    use alloy_sol_types::SolType;
    use euclid_ibc::wire::types::cross_chain_user::{cross_chain_user_to_sol, CrossChainUserSol};

    let sample = common::types_samples::cross_chain_user_samples()
        .into_iter()
        .next()
        .unwrap();
    let json_bytes = sample.to_json_bytes().unwrap();
    let sol = cross_chain_user_to_sol(&sample).unwrap();
    let abi_bytes = <CrossChainUserSol as SolType>::abi_encode_params(&sol);
    assert_ne!(json_bytes, abi_bytes);
}
