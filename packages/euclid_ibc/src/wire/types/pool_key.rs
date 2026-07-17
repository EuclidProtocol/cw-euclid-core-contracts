use alloy_sol_types::SolType;
use euclid::msgs::vlp::base::PoolKey;
use euclid_encoding::EncodingError;

use super::pair::{pair_from_sol, pair_to_sol, PairSol};
use super::pool_type::{pool_type_from_sol, pool_type_to_sol, PoolTypeSol};

pub type PoolKeySol = (PairSol, PoolTypeSol);

pub fn pool_key_to_sol(v: &PoolKey) -> Result<<PoolKeySol as SolType>::RustType, EncodingError> {
    Ok((pair_to_sol(&v.pair)?, pool_type_to_sol(&v.pool_type)?))
}

pub fn pool_key_from_sol(sol: <PoolKeySol as SolType>::RustType) -> Result<PoolKey, EncodingError> {
    let (pair, pool_type) = sol;
    Ok(PoolKey {
        pair: pair_from_sol(pair)?,
        pool_type: pool_type_from_sol(pool_type)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use euclid::msgs::vlp::base::PoolType;
    use euclid::token::{Pair, Token};

    fn pair() -> Pair {
        Pair {
            token_1: Token::create("aaa".to_string()).unwrap(),
            token_2: Token::create("bbb".to_string()).unwrap(),
        }
    }

    fn roundtrip(v: PoolKey) {
        let sol = pool_key_to_sol(&v).unwrap();
        let back = pool_key_from_sol(sol).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn pool_key_sol_roundtrip_constant_product() {
        roundtrip(PoolKey {
            pair: pair(),
            pool_type: PoolType::ConstantProduct {},
        });
    }

    #[test]
    fn pool_key_sol_roundtrip_concentrated() {
        roundtrip(PoolKey {
            pair: pair(),
            pool_type: PoolType::Concentrated {
                fee_tier_bps: 30,
                tick_spacing: 60,
            },
        });
    }
}
