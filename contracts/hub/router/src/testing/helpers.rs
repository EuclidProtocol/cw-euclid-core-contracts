use crate::contract::instantiate;
use crate::ibc::receive::reusable_internal_call;

use crate::state::{CHAIN_UID_TO_CHAIN, VIRTUAL_BALANCE_CONTRACT, VLPS};

use cosmwasm_std::testing::{message_info, mock_dependencies, mock_env, MockQuerier};
use cosmwasm_std::{
    to_json_binary, Addr, ContractResult, DepsMut, MessageInfo, Response, SystemResult, Uint128,
    WasmQuery,
};

use euclid::chain::{Chain, ChainType, ChainUid};
use euclid::cross_chain_user::CrossChainUser;
use euclid::error::ContractError;
use euclid::limit::Limit;

use euclid::msgs::router::InstantiateMsg;
use euclid::msgs::vlp::base::GetSwapQueryResponse;

use euclid::token::{Pair, PairWithDenomAndAmount, TokenWithDenom, TokenWithDenomAndAmount};
use euclid::token::{Token, TokenType};

use euclid_ibc::router_ibc::RouterCrossChainExecuteMsg;

// -----------------------------------------------------------------------
// Type alias & helpers
// -----------------------------------------------------------------------

pub type MockDeps = cosmwasm_std::OwnedDeps<
    cosmwasm_std::MemoryStorage,
    cosmwasm_std::testing::MockApi,
    MockQuerier,
>;

// Fixture address constants — any test that needs to reference one of these
// addresses by value should use these constants so that a change to init()
// is caught at compile time rather than silently breaking auth assertions.
pub const TEST_RELAYER: &str = "relayer";
pub const TEST_VIRTUAL_BALANCE: &str = "virtual_balance";
pub const TEST_RELEASE_FEE_RECIPIENT: &str = "release_fee_recipient";
pub const TEST_DEFAULT_FEE_RECIPIENT: &str = "default_fee_recipient";

pub(crate) fn init(deps: DepsMut, info: MessageInfo) -> Response {
    let msg = InstantiateMsg {
        concentrated_vlp_code_id: 4,
        relayer_contract: Addr::unchecked(TEST_RELAYER),
        release_fee_recipient: Addr::unchecked(TEST_RELEASE_FEE_RECIPIENT),
        default_fee_recipient: Addr::unchecked(TEST_DEFAULT_FEE_RECIPIENT),
        constant_product_vlp_code_id: 1,
        stable_vlp_code_id: 3,
        virtual_balance_code_id: 2,
    };
    instantiate(deps, mock_env(), info, msg).unwrap()
}

/// Helper: seed VIRTUAL_BALANCE_CONTRACT with address "virtual_balance".
pub fn seed_virtual_balance(deps: &mut MockDeps) {
    VIRTUAL_BALANCE_CONTRACT
        .save(
            deps.as_mut().storage,
            &Addr::unchecked(TEST_VIRTUAL_BALANCE),
        )
        .unwrap();
}

/// Helper: seed CHAIN_UID_TO_CHAIN with chain_uid="chain1", factory="factory1", Native.
pub fn seed_chain1_native(deps: &mut MockDeps) {
    let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
    CHAIN_UID_TO_CHAIN
        .save(
            deps.as_mut().storage,
            chain_uid.clone(),
            &Chain {
                chain_uid: chain_uid.clone(),
                factory_address: "factory1".to_string(),
                chain_type: ChainType::Native {},
            },
        )
        .unwrap();
}

pub(crate) fn make_native_recipient(
    chain_uid: ChainUid,
    address: &str,
    denom: &str,
    limit: Uint128,
) -> euclid::recipient::Recipient {
    use euclid::recipient::Recipient;
    Recipient {
        recipient: euclid::cross_chain_user::CrossChainUser::new(chain_uid, address.to_string()),
        amount: Limit::LessThanOrEqual(limit),
        denom: TokenType::Native {
            denom: denom.to_string(),
        },
        forwarding_message: None,
        unsafe_refund_as_voucher: None,
    }
}

// -----------------------------------------------------------------------
// reusable_internal_call: gate checks
// -----------------------------------------------------------------------

pub(crate) fn call_reusable(
    deps: &mut MockDeps,
    msg: RouterCrossChainExecuteMsg,
    chain_uid: ChainUid,
) -> Result<Response, ContractError> {
    let mut deps_mut = deps.as_mut();
    reusable_internal_call(
        &mut deps_mut,
        mock_env(),
        message_info(&Addr::unchecked("anyone"), &[]),
        msg,
        chain_uid,
    )
}

pub(crate) fn register_denom_msg(chain_uid: ChainUid) -> RouterCrossChainExecuteMsg {
    RouterCrossChainExecuteMsg::RegisterDenom {
        sender: CrossChainUser::new(chain_uid, "user".to_string()),
        token: TokenWithDenom {
            token: Token::create("usdc".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "uusdc".to_string(),
            },
        },
        tx_id: "tx1".to_string(),
    }
}

// -----------------------------------------------------------------------
// RequestPoolCreation dispatch
// -----------------------------------------------------------------------

pub(crate) fn make_pool_pair(amount_a: u128, amount_b: u128) -> PairWithDenomAndAmount {
    PairWithDenomAndAmount {
        token_1: TokenWithDenomAndAmount {
            token: Token::create("aaa".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "uaaa".to_string(),
            },
            amount: Uint128::new(amount_a),
        },
        token_2: TokenWithDenomAndAmount {
            token: Token::create("bbb".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "ubbb".to_string(),
            },
            amount: Uint128::new(amount_b),
        },
    }
}

// -----------------------------------------------------------------------
// AddLiquidity dispatch
// -----------------------------------------------------------------------

pub(crate) fn seed_vlp_aaa_bbb(deps: &mut MockDeps) {
    let pair = Pair::new(
        Token::create("aaa".to_string()).unwrap(),
        Token::create("bbb".to_string()).unwrap(),
    )
    .unwrap();
    VLPS.save(
        deps.as_mut().storage,
        pair.get_tupple(),
        &Addr::unchecked("vlp_contract"),
    )
    .unwrap();
}

// -----------------------------------------------------------------------
// Swap dispatch
// -----------------------------------------------------------------------

pub(crate) fn make_swap_deps_with_mock_querier(amount_out: u128) -> MockDeps {
    let mut deps = mock_dependencies();
    let creator = deps.api.addr_make("creator");
    init(deps.as_mut(), message_info(&creator, &[]));

    seed_virtual_balance(&mut deps);
    seed_vlp_aaa_bbb(&mut deps);

    let token_b = Token::create("bbb".to_string()).unwrap();
    deps.querier.update_wasm(move |q| match q {
        WasmQuery::Smart { .. } => {
            let resp = GetSwapQueryResponse {
                amount_out: Uint128::new(amount_out),
                asset_out: token_b.clone(),
                spread_amount: Uint128::zero(),
                lp_fee: Uint128::zero(),
                euclid_fee: Uint128::zero(),
            };
            SystemResult::Ok(ContractResult::Ok(to_json_binary(&resp).unwrap()))
        }
        _ => panic!("unexpected wasm query in swap test"),
    });
    deps
}
