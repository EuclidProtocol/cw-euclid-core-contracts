use crate::contract::instantiate;
use crate::state::{
    get_escrow_balance_key, get_token_metadata_key, ADMIN, STATE, VOUCHER_ALLOWANCES,
    VOUCHER_BALANCES,
};
use cosmwasm_std::testing::{message_info, mock_dependencies, mock_env, MockQuerier};
use cosmwasm_std::{Addr, Response, Uint256};
use euclid::admin::EuclidAdmin;
use euclid::chain::ChainUid;
use euclid::cross_chain_user::CrossChainUser;
use euclid::msgs::virtual_balance::msg::{InstantiateMsg, State, VoucherAllowance};
use euclid::token::{Token, TokenMetadata, TokenType};
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
    VOUCHER_BALANCES
        .save(deps.as_mut().storage, key, &Uint256::from(amount))
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
    VOUCHER_ALLOWANCES
        .save(
            deps.as_mut().storage,
            key,
            &VoucherAllowance {
                spender,
                amount: Uint256::from(amount),
                expires_at: None,
            },
        )
        .unwrap();
}

/// Register `TokenMetadata` for `(token_id, chain_uid, token_type)` with `allowed = true`,
/// and pre-seed a generous `ESCROW_BALANCES` entry for the same key so that `execute_burn`
/// can decrement it without underflowing. Voucher token decimals are set to 24 so that
/// `normalize_token_to_voucher` / `normalize_voucher_to_token` are identity in tests.
pub fn seed_token_metadata(
    deps: &mut MockDeps,
    token_id: &str,
    chain_uid: ChainUid,
    token_type: TokenType,
) {
    let token_type_with_decimals = match token_type {
        TokenType::Native { denom, .. } => TokenType::Native {
            denom,
            decimals: Some(24),
        },
        other => other,
    };
    let metadata_key = get_token_metadata_key(
        token_id.to_string(),
        chain_uid.clone(),
        token_type_with_decimals.clone(),
    );
    metadata_key
        .save(
            deps.as_mut().storage,
            &TokenMetadata {
                token: Token::create(token_id.to_string()).unwrap(),
                chain_uid: chain_uid.clone(),
                token_type: token_type_with_decimals.clone(),
                allowed: true,
            },
        )
        .unwrap();
    let escrow_key =
        get_escrow_balance_key(token_id.to_string(), chain_uid, token_type_with_decimals);
    escrow_key
        .save(deps.as_mut().storage, &Uint256::from(u128::MAX))
        .unwrap();
}

/// Like `seed_token_metadata` but uses the provided `decimals` instead of
/// hardcoding 24, so tests can exercise real normalization paths.
/// Escrow is pre-seeded to zero (caller controls initial escrow).
pub fn seed_token_metadata_with_decimals(
    deps: &mut MockDeps,
    token_id: &str,
    chain_uid: ChainUid,
    token_type: TokenType,
    decimals: u32,
) {
    let token_type_with_decimals = match token_type {
        TokenType::Native { denom, .. } => TokenType::Native {
            denom,
            decimals: Some(decimals),
        },
        other => other,
    };
    let metadata_key = get_token_metadata_key(
        token_id.to_string(),
        chain_uid.clone(),
        token_type_with_decimals.clone(),
    );
    metadata_key
        .save(
            deps.as_mut().storage,
            &TokenMetadata {
                token: Token::create(token_id.to_string()).unwrap(),
                chain_uid: chain_uid.clone(),
                token_type: token_type_with_decimals.clone(),
                allowed: true,
            },
        )
        .unwrap();
    let escrow_key =
        get_escrow_balance_key(token_id.to_string(), chain_uid, token_type_with_decimals);
    escrow_key
        .save(deps.as_mut().storage, &Uint256::zero())
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
