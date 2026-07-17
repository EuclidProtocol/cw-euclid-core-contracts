//! Sample builders for the 17 shared domain types (plan §6.1). Every builder
//! returns every enum variant and both `Some`/`None` for every optional
//! field, with nontrivial values (max `Uint256`, empty and multi-element
//! vecs) so the roundtrip suite in `roundtrip_types.rs` exercises the full
//! wire-shape surface, not just the happy path.

use cosmwasm_std::{Uint256, Uint64};
use euclid::chain::ChainUid;
use euclid::cross_chain_user::CrossChainUser;
use euclid::limit::Limit;
use euclid::msgs::router::execute::{
    RegisterFactoryChainCosmos, RegisterFactoryChainEvm, RegisterFactoryChainNative,
    RegisterFactoryChainTvm, RegisterFactoryChainType,
};
use euclid::msgs::vlp::base::{PoolConfig, PoolKey, PoolType};
use euclid::recipient::Recipient;
use euclid::swap::NextSwapPair;
use euclid::token::{
    Pair, PairWithAmount, PairWithDenomAndAmount, Token, TokenType, TokenWithAmount,
    TokenWithDenom, TokenWithDenomAndAmount,
};

pub fn token(id: &str) -> Token {
    Token::create(id.to_string()).unwrap()
}

pub fn token_samples() -> Vec<Token> {
    vec![token("abc"), token("uatom.ibc"), token("A")]
}

pub fn chain_uid(uid: &str) -> ChainUid {
    ChainUid::create(uid.to_string()).unwrap()
}

pub fn chain_uid_samples() -> Vec<ChainUid> {
    vec![chain_uid("cosmos"), chain_uid("vsl"), chain_uid("tron")]
}

pub fn cross_chain_user_samples() -> Vec<CrossChainUser> {
    vec![
        CrossChainUser::new(chain_uid("cosmos"), "cosmos1abcdef".to_string()),
        CrossChainUser::new(chain_uid("evm"), "0xabc123def456".to_string()),
        // Empty address is representable on the wire even though
        // `CrossChainUser::validate` would reject it (validation happens at
        // the contract boundary, not in the codec — §7.7).
        CrossChainUser::new(chain_uid("vsl"), String::new()),
    ]
}

pub fn pair_samples() -> Vec<Pair> {
    vec![
        Pair::new(token("abc"), token("def")).unwrap(),
        Pair::new(token("A"), token("b")).unwrap(),
    ]
}

pub fn token_type_samples() -> Vec<TokenType> {
    vec![
        TokenType::Native {
            denom: "uatom".to_string(),
            decimals: Some(6),
        },
        TokenType::Native {
            denom: "ibc/abc123".to_string(),
            decimals: None,
        },
        TokenType::Smart {
            contract_address: "cosmos1contract".to_string(),
            decimals: Some(18),
        },
        TokenType::Smart {
            contract_address: "0xdeadbeef".to_string(),
            decimals: None,
        },
        TokenType::Voucher {},
    ]
}

pub fn token_with_denom_samples() -> Vec<TokenWithDenom> {
    token_type_samples()
        .into_iter()
        .map(|token_type| TokenWithDenom {
            token: token("abc"),
            token_type,
        })
        .collect()
}

pub fn token_with_amount_samples() -> Vec<TokenWithAmount> {
    vec![
        TokenWithAmount {
            token: token("abc"),
            amount: Uint256::zero(),
        },
        TokenWithAmount {
            token: token("def"),
            amount: Uint256::MAX,
        },
        TokenWithAmount {
            token: token("ghi"),
            amount: Uint256::from(12345u128),
        },
    ]
}

pub fn token_with_denom_and_amount_samples() -> Vec<TokenWithDenomAndAmount> {
    token_type_samples()
        .into_iter()
        .map(|token_type| TokenWithDenomAndAmount {
            token: token("abc"),
            amount: Uint256::MAX,
            token_type,
        })
        .collect()
}

pub fn pair_with_amount_samples() -> Vec<PairWithAmount> {
    vec![
        PairWithAmount::new(
            TokenWithAmount {
                token: token("abc"),
                amount: Uint256::zero(),
            },
            TokenWithAmount {
                token: token("def"),
                amount: Uint256::MAX,
            },
        )
        .unwrap(),
        PairWithAmount::new(
            TokenWithAmount {
                token: token("a"),
                amount: Uint256::from(1u128),
            },
            TokenWithAmount {
                token: token("b"),
                amount: Uint256::from(2u128),
            },
        )
        .unwrap(),
    ]
}

pub fn pair_with_denom_and_amount_samples() -> Vec<PairWithDenomAndAmount> {
    vec![PairWithDenomAndAmount {
        token_1: TokenWithDenomAndAmount {
            token: token("abc"),
            amount: Uint256::MAX,
            token_type: TokenType::Native {
                denom: "uatom".to_string(),
                decimals: Some(6),
            },
        },
        token_2: TokenWithDenomAndAmount {
            token: token("def"),
            amount: Uint256::zero(),
            token_type: TokenType::Voucher {},
        },
    }]
}

pub fn limit_samples() -> Vec<Limit> {
    vec![
        Limit::LessThanOrEqual(Uint256::MAX),
        Limit::Equal(Uint256::from(42u128)),
        Limit::GreaterThanOrEqual(Uint256::zero()),
        Limit::Dynamic(Uint256::zero()),
    ]
}

pub fn recipient_samples() -> Vec<Recipient> {
    vec![
        Recipient {
            recipient: CrossChainUser::new(chain_uid("cosmos"), "cosmos1abc".to_string()),
            amount: Limit::Equal(Uint256::MAX),
            denom: TokenType::Native {
                denom: "uatom".to_string(),
                decimals: Some(6),
            },
            forwarding_message: Some("do-something".to_string()),
            unsafe_refund_as_voucher: Some(true),
        },
        Recipient {
            recipient: CrossChainUser::new(chain_uid("vsl"), "vsl1abc".to_string()),
            amount: Limit::Dynamic(Uint256::zero()),
            denom: TokenType::Voucher {},
            forwarding_message: None,
            unsafe_refund_as_voucher: None,
        },
    ]
}

pub fn next_swap_pair_samples() -> Vec<NextSwapPair> {
    let pool_key = pool_key_samples().into_iter().next().unwrap();
    vec![
        NextSwapPair {
            token_in: token("abc"),
            token_out: token("def"),
            pool_key: Some(pool_key),
            test_fail: Some(false),
        },
        NextSwapPair {
            token_in: token("ghi"),
            token_out: token("jkl"),
            pool_key: None,
            test_fail: None,
        },
    ]
}

pub fn pool_type_samples() -> Vec<PoolType> {
    vec![
        PoolType::ConstantProduct {},
        PoolType::Stable {},
        PoolType::Concentrated {
            fee_tier_bps: 30,
            tick_spacing: 60,
        },
    ]
}

pub fn pool_key_samples() -> Vec<PoolKey> {
    pool_type_samples()
        .into_iter()
        .map(|pool_type| PoolKey {
            pair: Pair::new(token("abc"), token("def")).unwrap(),
            pool_type,
        })
        .collect()
}

pub fn pool_config_samples() -> Vec<PoolConfig> {
    vec![
        PoolConfig::Stable {
            amp_factor: Some(Uint64::new(85)),
        },
        PoolConfig::Stable { amp_factor: None },
        PoolConfig::ConstantProduct {},
        PoolConfig::Concentrated {
            fee_tier_bps: 100,
            tick_spacing: 10,
        },
    ]
}

pub fn register_factory_chain_samples() -> Vec<RegisterFactoryChainType> {
    vec![
        RegisterFactoryChainType::Native(RegisterFactoryChainNative {
            factory_address: "native1factory".to_string(),
            factory_chain_id: "native".to_string(),
        }),
        RegisterFactoryChainType::Cosmos(RegisterFactoryChainCosmos {
            factory_address: "cosmos1factory".to_string(),
            factory_chain_id: "cosmoshub-4".to_string(),
        }),
        RegisterFactoryChainType::Evm(RegisterFactoryChainEvm {
            factory_address: "0xfactory".to_string(),
            factory_chain_id: "1".to_string(),
        }),
        RegisterFactoryChainType::Tvm(RegisterFactoryChainTvm {
            factory_address: "Tfactory".to_string(),
            factory_chain_id: "728126428".to_string(),
        }),
    ]
}
