use crate::contract::instantiate;
use crate::state::{ADMIN, ALLOWANCES, BALANCES, STATE};
use cosmwasm_std::testing::{message_info, mock_dependencies, mock_env, MockQuerier};
use cosmwasm_std::{Addr, Response, Uint128};
use euclid::admin::EuclidAdmin;
use euclid::chain::ChainUid;
use euclid::cross_chain_user::CrossChainUser;
use euclid::msgs::virtual_balance::msg::{Allowance, InstantiateMsg, State};
use euclid::voucher::BalanceKey;

// ---------------------------------------------------------------------------
// Type alias
// ---------------------------------------------------------------------------

pub type MockDeps = cosmwasm_std::OwnedDeps<
    cosmwasm_std::MemoryStorage,
    cosmwasm_std::testing::MockApi,
    MockQuerier,
>;

// ---------------------------------------------------------------------------
// Address constants
// ---------------------------------------------------------------------------

/// The default router address used by `init`.
pub const TEST_ROUTER: &str = "router";

// ---------------------------------------------------------------------------
// init helper
// ---------------------------------------------------------------------------

pub fn init(deps: &mut MockDeps) -> Response {
    let msg = InstantiateMsg {
        router: Addr::unchecked(TEST_ROUTER),
        admin: None,
    };
    let router = deps.api.addr_make(TEST_ROUTER);
    let info = message_info(&router, &[]);
    instantiate(deps.as_mut(), mock_env(), info, msg).unwrap()
}

// ---------------------------------------------------------------------------
// Seed helpers
// ---------------------------------------------------------------------------

/// Mint `amount` of `token_id` to `user` by writing directly into storage.
pub fn seed_balance(deps: &mut MockDeps, user: CrossChainUser, token_id: &str, amount: u128) {
    let key = BalanceKey {
        cross_chain_user: user,
        token_id: token_id.to_string(),
    }
    .to_serialized_balance_key();
    BALANCES
        .save(deps.as_mut().storage, key, &Uint128::new(amount))
        .unwrap();
}

/// Write an allowance directly into storage.
pub fn seed_allowance(
    deps: &mut MockDeps,
    owner: CrossChainUser,
    token_id: &str,
    spender: CrossChainUser,
    amount: u128,
) {
    let key = BalanceKey {
        cross_chain_user: owner,
        token_id: token_id.to_string(),
    }
    .to_serialized_balance_key();
    ALLOWANCES
        .save(
            deps.as_mut().storage,
            key,
            &Allowance {
                spender,
                amount: Uint128::new(amount),
            },
        )
        .unwrap();
}

/// Build a `CrossChainUser` on the VSL chain.
pub fn vsl_user(address: &str) -> CrossChainUser {
    CrossChainUser::new(ChainUid::vsl_chain_uid().unwrap(), address.to_string())
}

/// Build a `CrossChainUser` on a numbered remote chain (e.g. "1").
pub fn remote_user(chain_id: &str, address: &str) -> CrossChainUser {
    CrossChainUser::new(
        ChainUid::create(chain_id.to_string()).unwrap(),
        address.to_string(),
    )
}

// ---------------------------------------------------------------------------
// Shared state bootstrap (bypasses `instantiate`, useful in sub-modules)
// ---------------------------------------------------------------------------

/// Write STATE + ADMIN directly. Returns the router `Addr`.
#[allow(dead_code)]
pub fn bootstrap_state(deps: &mut MockDeps) -> Addr {
    let router = deps.api.addr_make(TEST_ROUTER);
    let admin = EuclidAdmin::default(router.clone());
    STATE
        .save(
            deps.as_mut().storage,
            &State {
                router: router.clone(),
            },
        )
        .unwrap();
    ADMIN.save(deps.as_mut().storage, &admin).unwrap();
    router
}

/// Create a fresh `MockDeps` with state already bootstrapped.
#[allow(dead_code)]
pub fn deps_with_state() -> MockDeps {
    let mut deps = mock_dependencies();
    bootstrap_state(&mut deps);
    deps
}
