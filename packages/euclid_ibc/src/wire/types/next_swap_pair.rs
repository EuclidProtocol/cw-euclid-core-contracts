use alloy_sol_types::private::Bytes;
use alloy_sol_types::sol_data::{Bool, String as SolString};
use alloy_sol_types::SolType;
use euclid::msgs::vlp::base::PoolKey;
use euclid::swap::NextSwapPair;
use euclid_encoding::abi::bridge::newtype_from_string;
use euclid_encoding::abi::option::{opt_prim_from_sol, OptDynSol, OptPrimSol};
use euclid_encoding::abi::params::decode_params_canonical;
use euclid_encoding::abi::tagged::ensure_empty;
use euclid_encoding::EncodingError;

use super::pool_key::{pool_key_from_sol, pool_key_to_sol, PoolKeySol};

pub type NextSwapPairSol = (SolString, SolString, OptDynSol, OptPrimSol<Bool>);

// `Option<PoolKey>` is a composite option: `PoolKey` is a domain struct, not
// a bare SolType primitive, so it rides as (bool some, bytes inner) with
// `inner` the nested ABI params encoding, exactly like
// `euclid_encoding::abi::option::opt_dyn_to_sol`/`opt_dyn_from_sol` but
// wired through the sibling `pool_key_to_sol`/`pool_key_from_sol` free
// functions instead of the (now-removed-here) `AbiMap` impl.
fn pool_key_opt_to_sol(v: &Option<PoolKey>) -> Result<(bool, Bytes), EncodingError> {
    match v {
        Some(inner) => {
            let encoded = <PoolKeySol as SolType>::abi_encode_params(&pool_key_to_sol(inner)?);
            Ok((true, encoded.into()))
        }
        None => Ok((false, Bytes::new())),
    }
}

fn pool_key_opt_from_sol(sol: (bool, Bytes)) -> Result<Option<PoolKey>, EncodingError> {
    let (some, data) = sol;
    if some {
        let decoded = decode_params_canonical::<PoolKeySol>("PoolKey", &data)?;
        Ok(Some(pool_key_from_sol(decoded)?))
    } else {
        ensure_empty("PoolKey", &data)?;
        Ok(None)
    }
}

pub fn next_swap_pair_to_sol(
    v: &NextSwapPair,
) -> Result<<NextSwapPairSol as SolType>::RustType, EncodingError> {
    Ok((
        v.token_in.to_string(),
        v.token_out.to_string(),
        pool_key_opt_to_sol(&v.pool_key)?,
        (v.test_fail.is_some(), v.test_fail.unwrap_or_default()),
    ))
}

pub fn next_swap_pair_from_sol(
    sol: <NextSwapPairSol as SolType>::RustType,
) -> Result<NextSwapPair, EncodingError> {
    let (token_in, token_out, pool_key, test_fail) = sol;
    Ok(NextSwapPair {
        token_in: newtype_from_string("Token", token_in)?,
        token_out: newtype_from_string("Token", token_out)?,
        pool_key: pool_key_opt_from_sol(pool_key)?,
        test_fail: opt_prim_from_sol("NextSwapPair", test_fail)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use euclid::msgs::vlp::base::PoolType;
    use euclid::token::{Pair, Token};

    fn token(denom: &str) -> Token {
        Token::create(denom.to_string()).unwrap()
    }

    fn roundtrip(v: NextSwapPair) {
        let sol = next_swap_pair_to_sol(&v).unwrap();
        let back = next_swap_pair_from_sol(sol).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn next_swap_pair_sol_roundtrip_with_pool_key() {
        roundtrip(NextSwapPair {
            token_in: token("aaa"),
            token_out: token("bbb"),
            pool_key: Some(PoolKey {
                pair: Pair {
                    token_1: token("aaa"),
                    token_2: token("bbb"),
                },
                pool_type: PoolType::Concentrated {
                    fee_tier_bps: 30,
                    tick_spacing: 60,
                },
            }),
            test_fail: Some(true),
        });
    }

    #[test]
    fn next_swap_pair_sol_roundtrip_without_pool_key() {
        roundtrip(NextSwapPair {
            token_in: token("aaa"),
            token_out: token("bbb"),
            pool_key: None,
            test_fail: None,
        });
    }
}
