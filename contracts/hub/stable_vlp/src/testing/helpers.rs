use cosmwasm_std::{
    testing::{message_info, mock_env, MockQuerier},
    Addr, Response, Uint128, Uint256, Uint64,
};
use euclid::{
    admin::EuclidAdmin,
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    fee::Fee,
    msgs::vlp::{
        base::{VlpAddLiquidityMsg, VlpRegisterPoolMsg},
        stable::msg::{ExecuteMsg, InstantiateMsg},
    },
    token::{Pair, PairWithAmount, Token, TokenWithAmount},
};

use crate::contract::{execute, instantiate};

pub type MockDeps = cosmwasm_std::OwnedDeps<
    cosmwasm_std::MemoryStorage,
    cosmwasm_std::testing::MockApi,
    MockQuerier,
>;

pub fn token1() -> Token {
    Token::create("token1".to_string()).unwrap()
}

pub fn token2() -> Token {
    Token::create("token2".to_string()).unwrap()
}

pub fn default_pair() -> Pair {
    Pair {
        token_1: token1(),
        token_2: token2(),
    }
}

pub fn default_fee() -> Fee {
    Fee::new(
        1,
        1,
        CrossChainUser::new(
            ChainUid::create("1".to_string()).unwrap(),
            "addr".to_string(),
        ),
    )
}

pub fn chain1() -> ChainUid {
    ChainUid::create("1".to_string()).unwrap()
}

pub fn chain2() -> ChainUid {
    ChainUid::create("2".to_string()).unwrap()
}

pub fn sender_on_chain1() -> CrossChainUser {
    CrossChainUser::new(chain1(), "sender_address".to_string())
}

pub fn sender_on_chain2() -> CrossChainUser {
    CrossChainUser::new(chain2(), "sender_address".to_string())
}

pub fn init(deps: &mut MockDeps) -> Response {
    let router = deps.api.addr_make("router");
    let admin = EuclidAdmin::default(deps.api.addr_make("admin"));
    let msg = InstantiateMsg {
        router: Addr::unchecked("router"),
        virtual_balance_contract: Addr::unchecked("virtual_balance_contract"),
        pair: default_pair(),
        fee: default_fee(),
        execute: None,
        admin,
        amp_factor: Some(Uint64::from(1000u64)),
    };
    let info = message_info(&router, &[]);
    instantiate(deps.as_mut(), mock_env(), info, msg).unwrap()
}

pub fn register_pool(deps: &mut MockDeps, chain_uid: ChainUid, tx_id: &str) {
    let router = deps.api.addr_make("router");
    let info = message_info(&router, &[]);
    let msg = ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
        sender: CrossChainUser::new(chain_uid, "sender".to_string()),
        pair: default_pair(),
        tx_id: tx_id.to_string(),
    });
    execute(deps.as_mut(), mock_env(), info, msg).unwrap();
}

pub fn seed_liquidity(deps: &mut MockDeps, reserve: u128) {
    register_pool(deps, chain1(), "reg-tx");

    let router = deps.api.addr_make("router");
    let info = message_info(&router, &[]);
    let liquidity = PairWithAmount::new(
        TokenWithAmount {
            token: token1(),
            amount: Uint256::from(reserve),
        },
        TokenWithAmount {
            token: token2(),
            amount: Uint256::from(reserve),
        },
    )
    .unwrap();
    let msg = ExecuteMsg::AddLiquidity(VlpAddLiquidityMsg {
        sender: sender_on_chain1(),
        tx_id: "liq-tx".to_string(),
        liquidity,
        slippage_tolerance_bps: 5000,
    });
    execute(deps.as_mut(), mock_env(), info, msg).unwrap();
}
