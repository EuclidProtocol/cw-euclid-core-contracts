use crate::contract::instantiate;
use crate::state::{BALANCES, CHAIN_LP_TOKENS, STATE};
use cosmwasm_std::testing::{message_info, mock_env, MockQuerier};
use cosmwasm_std::{Addr, Response, Uint128};
use euclid::admin::EuclidAdmin;
use euclid::chain::ChainUid;
use euclid::cross_chain_user::CrossChainUser;
use euclid::fee::Fee;
use euclid::msgs::vlp::cp::msg::InstantiateMsg;
use euclid::token::{Pair, PairWithAmount, Token};
// -----------------------------------------------------------------------
// Type alias
// -----------------------------------------------------------------------

pub type MockDeps = cosmwasm_std::OwnedDeps<
    cosmwasm_std::MemoryStorage,
    cosmwasm_std::testing::MockApi,
    MockQuerier,
>;

// -----------------------------------------------------------------------
// Address constants
// -----------------------------------------------------------------------

pub const TEST_VIRTUAL_BALANCE: &str = "virtual_balance_contract";

// -----------------------------------------------------------------------
// Token helpers
// -----------------------------------------------------------------------

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

pub fn default_fee(deps: &MockDeps) -> Fee {
    Fee::new(
        1,
        1,
        CrossChainUser::new(
            ChainUid::create("1".to_string()).unwrap(),
            deps.api.addr_make("fee_recipient").to_string(),
        ),
    )
}

pub fn default_admin(deps: &MockDeps) -> EuclidAdmin {
    EuclidAdmin::default(deps.api.addr_make("admin"))
}

// -----------------------------------------------------------------------
// Standard init helper
// -----------------------------------------------------------------------

pub fn init(deps: &mut MockDeps) -> Response {
    let admin = default_admin(deps);
    let fee = default_fee(deps);
    let msg = InstantiateMsg {
        router: Addr::unchecked("router"),
        virtual_balance_contract: Addr::unchecked(TEST_VIRTUAL_BALANCE),
        pair: default_pair(),
        fee,
        execute: None,
        admin,
    };
    // The router address is set as info.sender in instantiate, so we use
    // addr_make("router") to match what the test router address is.
    let router = deps.api.addr_make("router");
    let info = message_info(&router, &[]);
    instantiate(deps.as_mut(), mock_env(), info, msg).unwrap()
}

// -----------------------------------------------------------------------
// Seed helpers
// -----------------------------------------------------------------------

/// Register a chain pool with zero LP tokens (simulates RegisterPool).
pub fn seed_pool(deps: &mut MockDeps, chain_uid: &ChainUid) {
    CHAIN_LP_TOKENS
        .save(deps.as_mut().storage, chain_uid.clone(), &Uint128::zero())
        .unwrap();
}

/// Set balances for both tokens in the pair.
pub fn seed_balances(deps: &mut MockDeps, reserve_1: Uint128, reserve_2: Uint128) {
    BALANCES
        .save(deps.as_mut().storage, token1(), &reserve_1)
        .unwrap();
    BALANCES
        .save(deps.as_mut().storage, token2(), &reserve_2)
        .unwrap();
}

/// Seed state with custom total_lp_tokens and balances — used for add/remove
/// liquidity and swap tests that need pre-seeded liquidity.
pub fn seed_liquidity(
    deps: &mut MockDeps,
    reserve_1: Uint128,
    reserve_2: Uint128,
    total_lp: Uint128,
) {
    seed_balances(deps, reserve_1, reserve_2);
    let mut state = STATE.load(&deps.storage).unwrap();
    state.total_lp_tokens = total_lp;
    STATE.save(deps.as_mut().storage, &state).unwrap();
}

/// Seed a chain pool with a given LP token amount.
pub fn seed_chain_lp(deps: &mut MockDeps, chain_uid: &ChainUid, lp_tokens: Uint128) {
    CHAIN_LP_TOKENS
        .save(deps.as_mut().storage, chain_uid.clone(), &lp_tokens)
        .unwrap();
}

/// Build a simple pair-with-amounts for add_liquidity calls.
pub fn make_pair_with_amount(amount_1: u128, amount_2: u128) -> PairWithAmount {
    PairWithAmount::new(
        token1().with_amount(Uint128::new(amount_1)),
        token2().with_amount(Uint128::new(amount_2)),
    )
    .unwrap()
}

/// Build a CrossChainUser with a given chain_uid string and address.
pub fn cross_chain_user(chain_uid_str: &str, address: &str) -> CrossChainUser {
    CrossChainUser::new(
        ChainUid::create(chain_uid_str.to_string()).unwrap(),
        address.to_string(),
    )
}
