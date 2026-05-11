use cosmwasm_std::{
    testing::{message_info, mock_env, MockQuerier},
    to_json_binary, Addr, ContractResult, Response, SystemResult, WasmQuery,
};
use euclid::{
    chain::ChainUid,
    msgs::{
        escrow::{AllowedDenomsResponse, AllowedTokenResponse},
        factory::InstantiateMsg,
    },
    token::{Token, TokenType},
};

use crate::{
    contract::instantiate,
    state::{FeeState, State, ADMIN, FEE_STATE, PAIR_TO_VLP, STATE, TOKEN_TO_ESCROW},
};

// -----------------------------------------------------------------------
// Type alias
// -----------------------------------------------------------------------

pub type MockDeps = cosmwasm_std::OwnedDeps<
    cosmwasm_std::MemoryStorage,
    cosmwasm_std::testing::MockApi,
    MockQuerier,
>;

// -----------------------------------------------------------------------
// Fixture address constants
// -----------------------------------------------------------------------

pub const TEST_ROUTER: &str = "router_contract";
pub const TEST_RELAYER: &str = "relayer_contract";
pub const TEST_RATE_LIMIT_FEE_RECIPIENT: &str = "rate_limit_fee_recipient";
pub const TEST_CHAIN_UID: &str = "testchain";
pub const TEST_ESCROW: &str = "escrow_contract";

// -----------------------------------------------------------------------
// init helper
// -----------------------------------------------------------------------

pub fn init(deps: &mut MockDeps) -> Response {
    let sender = deps.api.addr_make("sender");
    let relayer = Addr::unchecked(TEST_RELAYER);
    let rate_limit_fee_recipient = Addr::unchecked(TEST_RATE_LIMIT_FEE_RECIPIENT);

    let msg = InstantiateMsg {
        router_contract: TEST_ROUTER.to_string(),
        chain_uid: ChainUid::create(TEST_CHAIN_UID.to_string()).unwrap(),
        escrow_code_id: 10,
        lp_code_id: 11,
        is_native: false,
        relayer_contract: relayer,
        rate_limit_fee_recipient,
        rate_limit_fee_denom: "uusd".to_string(),
        rate_limit_free_limit: cosmwasm_std::Uint256::from(100u128),
    };

    let info = message_info(&sender, &[]);
    instantiate(deps.as_mut(), mock_env(), info, msg).unwrap()
}

// -----------------------------------------------------------------------
// Querier mock helpers
// -----------------------------------------------------------------------

/// Configure the mock querier so that every WasmQuery::Smart returns
/// `AllowedTokenResponse { allowed }` for the given escrow address
/// and `AllowedDenomsResponse { denoms: vec![] }` for the denoms query.
pub fn set_escrow_token_allowed(deps: &mut MockDeps, allowed: bool) {
    deps.querier.update_wasm(move |q| match q {
        WasmQuery::Smart { msg, .. } => {
            // Decode to check which query is being made
            let query: serde_json::Value = serde_json::from_slice(msg.as_slice()).unwrap();
            if query.get("token_allowed").is_some() {
                let resp = AllowedTokenResponse { allowed };
                SystemResult::Ok(ContractResult::Ok(to_json_binary(&resp).unwrap()))
            } else if query.get("allowed_denoms").is_some() {
                let resp = AllowedDenomsResponse { denoms: vec![] };
                SystemResult::Ok(ContractResult::Ok(to_json_binary(&resp).unwrap()))
            } else {
                SystemResult::Ok(ContractResult::Ok(
                    to_json_binary(&AllowedTokenResponse { allowed: false }).unwrap(),
                ))
            }
        }
        _ => panic!("unexpected query"),
    });
}

// -----------------------------------------------------------------------
// State seeding helpers
// -----------------------------------------------------------------------

/// Seed TOKEN_TO_ESCROW so that `token` maps to `escrow_addr`.
pub fn seed_escrow(deps: &mut MockDeps, token_id: &str, escrow_addr: &str) {
    let token = Token::create(token_id.to_string()).unwrap();
    TOKEN_TO_ESCROW
        .save(deps.as_mut().storage, token, &Addr::unchecked(escrow_addr))
        .unwrap();
}

/// Seed PAIR_TO_VLP so that pair (token_a, token_b) maps to `vlp`.
/// Tokens are sorted lexicographically as required by `Pair`.
pub fn seed_vlp(deps: &mut MockDeps, token_a: &str, token_b: &str, vlp: &str) {
    use euclid::token::Pair;
    let pair = Pair::new(
        Token::create(token_a.to_string()).unwrap(),
        Token::create(token_b.to_string()).unwrap(),
    )
    .unwrap();
    PAIR_TO_VLP
        .save(deps.as_mut().storage, pair.get_tupple(), &vlp.to_string())
        .unwrap();
}

/// Build a minimal `CrossChainConfig` for use in tests.
pub fn default_cross_chain_config() -> euclid::msgs::cross_chain_config::CrossChainConfig {
    euclid::msgs::cross_chain_config::CrossChainConfig::default()
}

/// Helper: return the stored STATE.
pub fn load_state(deps: &MockDeps) -> State {
    STATE.load(&deps.storage).unwrap()
}

/// Helper: return the stored FEE_STATE.
pub fn load_fee_state(deps: &MockDeps) -> FeeState {
    FEE_STATE.load(&deps.storage).unwrap()
}

/// Helper: read current general_admin from storage.
pub fn load_general_admin(deps: &MockDeps) -> Addr {
    ADMIN.load(&deps.storage).unwrap().general_admin
}

/// Build a TokenWithDenom with a native denom.
pub fn native_token(token_id: &str, denom: &str) -> euclid::token::TokenWithDenom {
    euclid::token::TokenWithDenom {
        token: Token::create(token_id.to_string()).unwrap(),
        token_type: TokenType::Native {
            denom: denom.to_string(),
            decimals: None,
        },
    }
}

/// Build a TokenWithDenom with a Smart (CW20) token type.
pub fn smart_token(token_id: &str, contract_address: &str) -> euclid::token::TokenWithDenom {
    euclid::token::TokenWithDenom {
        token: Token::create(token_id.to_string()).unwrap(),
        token_type: TokenType::Smart {
            contract_address: contract_address.to_string(),
            decimals: Some(6),
        },
    }
}

/// Build a TokenWithDenom with a voucher type.
pub fn voucher_token(token_id: &str) -> euclid::token::TokenWithDenom {
    euclid::token::TokenWithDenom {
        token: Token::create(token_id.to_string()).unwrap(),
        token_type: TokenType::Voucher {},
    }
}

// -----------------------------------------------------------------------
// Event assertion helpers
// -----------------------------------------------------------------------

/// Assert that `res.attributes` contains an attribute with the given key and value.
pub fn assert_attribute(res: &Response, key: &str, value: &str) {
    assert!(
        res.attributes
            .iter()
            .any(|a| a.key == key && a.value == value),
        "expected attribute {key}={value}"
    );
}

/// Parse a named attribute from `res.attributes` and return its value as `&str`.
/// Panics with a descriptive message if the attribute is absent.
pub fn get_attribute<'a>(res: &'a Response, key: &str) -> &'a str {
    res.attributes
        .iter()
        .find(|a| a.key == key)
        .unwrap_or_else(|| panic!("attribute '{key}' not found in response"))
        .value
        .as_str()
}

/// Assert that `res.events` contains a `tx_event` — a euclid event with
/// `action = "transaction"` and `type = tx_type`.
pub fn assert_tx_event(res: &Response, tx_type: &str) {
    assert!(
        res.events.iter().any(|e| {
            e.ty == "euclid"
                && e.attributes
                    .iter()
                    .any(|a| a.key == "action" && a.value == "transaction")
                && e.attributes
                    .iter()
                    .any(|a| a.key == "type" && a.value == tx_type)
        }),
        "expected tx_event with type={tx_type}"
    );
}

/// Assert that `res.events` contains a `tx_event` with the exact type, tx_id, and sender.
pub fn assert_tx_event_full(res: &Response, tx_type: &str, tx_id: &str, sender: &str) {
    assert!(
        res.events.iter().any(|e| {
            e.ty == "euclid"
                && e.attributes
                    .iter()
                    .any(|a| a.key == "action" && a.value == "transaction")
                && e.attributes
                    .iter()
                    .any(|a| a.key == "type" && a.value == tx_type)
                && e.attributes
                    .iter()
                    .any(|a| a.key == "tx_id" && a.value == tx_id)
                && e.attributes
                    .iter()
                    .any(|a| a.key == "sender" && a.value == sender)
        }),
        "expected tx_event with type={tx_type}, tx_id={tx_id}, sender={sender}"
    );
}

/// Assert that `res.events` contains a euclid event with the given `action` value.
pub fn assert_euclid_action(res: &Response, action: &str) {
    assert!(
        res.events.iter().any(|e| {
            e.ty == "euclid"
                && e.attributes
                    .iter()
                    .any(|a| a.key == "action" && a.value == action)
        }),
        "expected euclid event with action={action}"
    );
}
