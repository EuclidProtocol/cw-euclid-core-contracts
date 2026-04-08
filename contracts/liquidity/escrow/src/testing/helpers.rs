use cosmwasm_std::{
    coin,
    testing::{message_info, mock_dependencies, mock_env, MockQuerier},
    Response,
};
use euclid::{
    msgs::escrow::{ExecuteMsg, InstantiateMsg},
    token::{Token, TokenType},
};

use crate::contract::{execute, instantiate};

use cosmwasm_std::{to_json_binary, Uint128};
use cw20::Cw20ReceiveMsg;
use euclid::msgs::escrow::cw20::EscrowCw20HookMsg;

pub type MockDeps = cosmwasm_std::OwnedDeps<
    cosmwasm_std::MemoryStorage,
    cosmwasm_std::testing::MockApi,
    MockQuerier,
>;

pub const TOKEN_ID: &str = "eucl";
pub const NATIVE_DENOM: &str = "ueucl";

pub fn token() -> Token {
    Token::create(TOKEN_ID.to_string()).unwrap()
}

pub fn native_denom() -> TokenType {
    TokenType::Native {
        denom: NATIVE_DENOM.to_string(),
    }
}

pub fn smart_denom(addr: &str) -> TokenType {
    TokenType::Smart {
        contract_address: addr.to_string(),
    }
}

/// Standard init: factory = deps.api.addr_make("factory"), token = eucl, allowed denom = ueucl.
pub fn init(deps: &mut MockDeps) -> Response {
    let msg = InstantiateMsg {
        token_id: token(),
        allowed_denom: Some(native_denom()),
    };
    let factory = deps.api.addr_make("factory");
    let info = message_info(&factory, &[]);
    instantiate(deps.as_mut(), mock_env(), info, msg).unwrap()
}

/// Init without an initial allowed denom.
pub fn init_no_denom(deps: &mut MockDeps) -> Response {
    let msg = InstantiateMsg {
        token_id: token(),
        allowed_denom: None,
    };
    let factory = deps.api.addr_make("factory");
    let info = message_info(&factory, &[]);
    instantiate(deps.as_mut(), mock_env(), info, msg).unwrap()
}

/// Convenience: deposit `amount` of `NATIVE_DENOM` from the factory address.
pub fn deposit_native(deps: &mut MockDeps, amount: u128) {
    let factory = deps.api.addr_make("factory");
    let info = message_info(&factory, &[coin(amount, NATIVE_DENOM)]);
    execute(
        deps.as_mut(),
        mock_env(),
        info,
        ExecuteMsg::DepositNative {},
    )
    .unwrap();
}

pub fn make_cw20_receive_msg(sender: &str, amount: u128) -> Cw20ReceiveMsg {
    Cw20ReceiveMsg {
        sender: sender.to_string(),
        amount: Uint128::new(amount),
        msg: to_json_binary(&EscrowCw20HookMsg::Deposit {}).unwrap(),
    }
}

/// Returns a fresh initialized MockDeps (factory = "factory", allowed denom = ueucl, no deposits).
pub fn initialized_deps() -> MockDeps {
    let mut deps = mock_dependencies();
    init(&mut deps);
    deps
}

/// Returns a fresh MockDeps with 1_000 ueucl deposited.
pub fn with_deposit_deps() -> MockDeps {
    let mut deps = mock_dependencies();
    init(&mut deps);
    deposit_native(&mut deps, 1_000);
    deps
}
