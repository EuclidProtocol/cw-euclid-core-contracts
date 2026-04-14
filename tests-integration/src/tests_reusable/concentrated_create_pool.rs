#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::Uint128;
use cw20::{Cw20Coin, MinterResponse};
use cw_orch::{mock::MockBase, prelude::*};
use cw_orch_interchain::mock::MockInterchainEnv;
use cw_orch_interchain::prelude::InterchainEnv;
use euclid::chain::ChainUid;
use euclid::cross_chain_user::CrossChainUser;
use euclid::msgs::cross_chain_config::CrossChainConfig;
use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
use euclid::msgs::lp_token::msg::InstantiateMsg as LpTokenInstantiateMsg;
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
use euclid::msgs::virtual_balance::msg::{ExecuteMint, ExecuteMsg as VirtualBalanceExecuteMsg};
use euclid::token::{Pair, PairWithDenomAndAmount, Token, TokenType, TokenWithDenom};
use euclid::voucher::BalanceKey;
use factory::FactoryContract;
use lp_token::LpTokenContract;
use router::RouterContract;
use rstest::rstest;

use crate::helpers::chains::{get_virtual_balance, setup_router};
use crate::helpers::factory::{
    concentrated_pool_key, create_concentrated_pool, create_concentrated_pool_with_tick, faucet,
};
use crate::tests_reusable::constants::{
    FACTORY_CHAIN_ID_EVM, FACTORY_CHAIN_ID_IBC, FACTORY_CHAIN_ID_LOCAL, ROUTER_CHAIN_ID,
};
use crate::tests_reusable::factory_register::{setup_factory_with_mode, FactorySetupMode};
use crate::tests_reusable::factory_register_denom::{deregister_denom, register_denom};

/// Sets up the interchain, router, and factory without registering any tokens.
pub fn setup_concentrated_base(
    mode: FactorySetupMode,
    factory_chain_id: &str,
) -> (
    MockInterchainEnv,
    FactoryContract<MockBase>,
    RouterContract<MockBase>,
) {
    let sender = "sender_for_all_chains";
    let mut chains = vec![(ROUTER_CHAIN_ID, sender)];
    if factory_chain_id != ROUTER_CHAIN_ID {
        chains.push((factory_chain_id, sender));
    }
    let interchain = MockInterchainEnv::new(chains);
    let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
    let router = setup_router(&router_chain, vec![factory_chain_id]).unwrap();
    let factory = setup_factory_with_mode(&interchain, factory_chain_id, &router, mode).unwrap();
    (interchain, factory, router)
}

/// Deploys a CW20 contract and returns a `TokenWithDenom` backed by it.
pub fn make_smart_token(factory: &FactoryContract<MockBase>, token_id: &str) -> TokenWithDenom {
    let chain = factory.environment();
    let sender = chain.sender.to_string();
    let cw20 = LpTokenContract::new(chain.clone());
    cw20.upload().unwrap();

    let token = Token::create(token_id.to_string()).unwrap();
    let aux = Token::create(format!("{token_id}.aux")).unwrap();
    let token_pair = Pair::new(token.clone(), aux).unwrap();

    cw20.instantiate(
        &LpTokenInstantiateMsg {
            name: format!("{token_id}_cw20"),
            symbol: "CLP".to_string(),
            decimals: 6,
            initial_balances: vec![Cw20Coin {
                address: sender,
                amount: Uint128::new(1_000_000_000),
            }],
            mint: Some(MinterResponse {
                minter: chain.sender.to_string(),
                cap: None,
            }),
            marketing: None,
            vlp: chain.addr_make("dummy_vlp").to_string(),
            factory: chain.addr_make("dummy_factory"),
            token_pair,
        },
        None,
        &[],
    )
    .unwrap();

    TokenWithDenom {
        token,
        token_type: TokenType::Smart {
            contract_address: cw20.address().unwrap().to_string(),
        },
    }
}

/// Build a `TokenWithDenom` for the given kind literal.
///
/// `kind` is one of `"native"`, `"smart"`, or `"voucher"`. `token_id` is the
/// logical token identifier; for `native` it is also used as the denom, for
/// `smart` a CW20 contract is deployed, and for `voucher` the token type is
/// the hub voucher variant with the token_id as its identifier.
pub fn make_token(
    factory: &FactoryContract<MockBase>,
    token_id: &str,
    kind: &str,
) -> TokenWithDenom {
    match kind {
        "native" => TokenWithDenom {
            token: Token::create(token_id.to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: token_id.to_string(),
            },
        },
        "smart" => make_smart_token(factory, token_id),
        "voucher" => TokenWithDenom {
            token: Token::create(token_id.to_string()).unwrap(),
            token_type: TokenType::Voucher {},
        },
        other => panic!("unknown token kind: {other}"),
    }
}

/// Full setup with both tokens defaulting to `native` (backward-compatible).
pub fn setup_concentrated_env(
    mode: FactorySetupMode,
    factory_chain_id: &str,
) -> (
    MockInterchainEnv,
    FactoryContract<MockBase>,
    RouterContract<MockBase>,
    TokenWithDenom,
    TokenWithDenom,
) {
    setup_concentrated_env_ext(mode, factory_chain_id, "native", "native")
}

/// Full setup parameterised by each token's kind (`"native"`, `"smart"`, or
/// `"voucher"`). Each token is registered on the factory/router so pool
/// creation sees them as existing tokens. For voucher tokens, a backing
/// native denom is registered (so the router recognises the token id) and
/// voucher balance is minted on the hub for the sender.
pub fn setup_concentrated_env_ext(
    mode: FactorySetupMode,
    factory_chain_id: &str,
    token_a_kind: &str,
    token_b_kind: &str,
) -> (
    MockInterchainEnv,
    FactoryContract<MockBase>,
    RouterContract<MockBase>,
    TokenWithDenom,
    TokenWithDenom,
) {
    let (interchain, factory, router) = setup_concentrated_base(mode, factory_chain_id);

    let token_a = prepare_token(&factory, &router, "conc.token.a", token_a_kind);
    let token_b = prepare_token(&factory, &router, "conc.token.b", token_b_kind);

    (interchain, factory, router, token_a, token_b)
}

/// Build a token of the requested kind and perform any per-kind registration
/// required for it to be usable in a pool-creation request.
fn prepare_token(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    token_id: &str,
    kind: &str,
) -> TokenWithDenom {
    let token = make_token(factory, token_id, kind);
    match kind {
        "voucher" => {
            // Router-side voucher validation requires the token to exist on at
            // least one chain, so register a backing native denom on the
            // factory chain. Then mint voucher balance on the hub so the
            // sender can actually spend vouchers of this token.
            let backing = TokenWithDenom {
                token: token.token.clone(),
                token_type: TokenType::Native {
                    denom: token_id.to_string(),
                },
            };
            register_denom(factory, router, backing).unwrap();
            mint_voucher_balance(factory, router, &token.token, 1_000_000_000u128);
        }
        "native" | "smart" => {
            register_denom(factory, router, token.clone()).unwrap();
        }
        _ => unreachable!("make_token already validated the kind"),
    }
    token
}

/// Mint `amount` of voucher balance for the factory sender on the hub's
/// virtual balance contract. Only the router is authorised to mint, so the
/// call is dispatched via a temporary `set_sender` override.
fn mint_voucher_balance(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    token: &Token,
    amount: u128,
) {
    let factory_chain_uid = factory.get_state().unwrap().chain_uid;
    let sender = factory.environment().sender.to_string();
    let virtual_balance_address = router.get_state().unwrap().virtual_balance_address;
    let mut vb = get_virtual_balance(router.environment(), &virtual_balance_address);
    vb.set_sender(&router.address().unwrap());
    vb.execute(
        &VirtualBalanceExecuteMsg::Mint(ExecuteMint {
            amount: Uint128::new(amount),
            balance_key: BalanceKey {
                cross_chain_user: CrossChainUser::new(factory_chain_uid, sender),
                token_id: token.to_string(),
            },
        }),
        &[],
    )
    .unwrap();
}

/// Maps a `FactorySetupMode` to the canonical factory chain id used in tests.
pub fn chain_id_for_mode(mode: FactorySetupMode) -> &'static str {
    match mode {
        FactorySetupMode::Native => FACTORY_CHAIN_ID_LOCAL,
        FactorySetupMode::Ibc => FACTORY_CHAIN_ID_IBC,
        FactorySetupMode::Evm => FACTORY_CHAIN_ID_EVM,
    }
}

pub fn pair_with_amounts(
    token_a: &TokenWithDenom,
    token_b: &TokenWithDenom,
    amount_a: u128,
    amount_b: u128,
) -> PairWithDenomAndAmount {
    PairWithDenomAndAmount {
        token_1: token_a.with_amount(Uint128::new(amount_a)),
        token_2: token_b.with_amount(Uint128::new(amount_b)),
    }
}

// Each rstest below uses `#[values]` on chain mode and each token kind,
// yielding the full cartesian product of chain type (Native/Ibc/Evm) × token
// kinds. Tests restrict the kind list to combinations that are meaningful for
// their scenario (e.g. voucher cannot represent the "new / unregistered" side
// of a pool because the router requires voucher tokens to already be known).

#[rstest]
fn test_create_two_fee_tiers_same_pair(
    #[values(FactorySetupMode::Native, FactorySetupMode::Ibc, FactorySetupMode::Evm)]
    mode: FactorySetupMode,
    #[values("native", "smart", "voucher")] token_a_kind: &str,
    #[values("native", "smart", "voucher")] token_b_kind: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env_ext(mode, chain_id_for_mode(mode), token_a_kind, token_b_kind);

    let pair = pair_with_amounts(&token_a, &token_b, 20_000, 20_000);
    let pool_key_500 = create_concentrated_pool(&factory, &router, pair.clone(), 500, 10, 100)
        .expect("500 bps pool should be created");
    let pool_key_3000 = create_concentrated_pool(&factory, &router, pair.clone(), 3_000, 60, 100)
        .expect("3000 bps pool should be created");

    let pool_500 = factory.get_concentrated_vlp(pool_key_500.clone()).unwrap();
    let pool_3000 = factory.get_concentrated_vlp(pool_key_3000.clone()).unwrap();
    assert_ne!(
        pool_500.vlp_address, pool_3000.vlp_address,
        "different fee tiers must map to different concentrated pools",
    );

    let router_500 = router.get_vlp_by_pool_key(pool_key_500).unwrap();
    let router_3000 = router.get_vlp_by_pool_key(pool_key_3000).unwrap();
    assert_ne!(router_500.vlp, router_3000.vlp);
}

#[rstest]
fn test_create_two_fee_tiers_same_pair_with_initial_tick(
    #[values(FactorySetupMode::Native, FactorySetupMode::Ibc, FactorySetupMode::Evm)]
    mode: FactorySetupMode,
    #[values("native", "smart", "voucher")] token_a_kind: &str,
    #[values("native", "smart", "voucher")] token_b_kind: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env_ext(mode, chain_id_for_mode(mode), token_a_kind, token_b_kind);

    let (amount_a, amount_b) = (10_000_000_u128, 100_000_000_u128);
    let pair = pair_with_amounts(&token_a, &token_b, amount_a, amount_b);

    // tick = floor(ln(price) / ln(1.0001)) where price = amount_token2 / amount_token1
    // Pair sorts tokens alphabetically, so token_1 < token_2.
    // price = amount_b / amount_a = 10 → tick ≈ 23025
    let price = amount_b as f64 / amount_a as f64;
    let initial_tick = (price.ln() / 1.0001_f64.ln()).floor() as i64;

    let pool_key_500 = create_concentrated_pool_with_tick(
        &factory,
        &router,
        pair.clone(),
        500,
        10,
        100,
        Some(initial_tick),
    )
    .expect("500 bps pool should be created");
    let pool_key_3000 = create_concentrated_pool_with_tick(
        &factory,
        &router,
        pair.clone(),
        3_000,
        60,
        100,
        Some(initial_tick),
    )
    .expect("3000 bps pool should be created");

    let pool_500 = factory.get_concentrated_vlp(pool_key_500.clone()).unwrap();
    let pool_3000 = factory.get_concentrated_vlp(pool_key_3000.clone()).unwrap();
    assert_ne!(
        pool_500.vlp_address, pool_3000.vlp_address,
        "different fee tiers must map to different concentrated pools",
    );

    let router_500 = router.get_vlp_by_pool_key(pool_key_500).unwrap();
    let router_3000 = router.get_vlp_by_pool_key(pool_key_3000).unwrap();
    assert_ne!(router_500.vlp, router_3000.vlp);
}

#[rstest]
fn test_create_pool_with_unregistered_token_passes_and_creates_escrow(
    #[values(FactorySetupMode::Native, FactorySetupMode::Ibc, FactorySetupMode::Evm)]
    mode: FactorySetupMode,
    // token_a is the registered side, so voucher is valid here.
    #[values("native", "smart", "voucher")] token_a_kind: &str,
    // token_c is the intentionally-new side; voucher is excluded because the
    // router rejects voucher tokens that don't yet exist on any chain.
    #[values("native", "smart")] token_c_kind: &str,
) {
    // token_a is registered via setup; the second slot is reused for token_c,
    // which we intentionally do NOT register, so pool creation must lazily
    // create its escrow on ACK.
    let (_interchain, factory, router, token_a, _token_b) =
        setup_concentrated_env_ext(mode, chain_id_for_mode(mode), token_a_kind, "native");

    let token_c = make_token(&factory, "conc.token.c", token_c_kind);

    let pair = pair_with_amounts(&token_a, &token_c, 20_000, 20_000);
    create_concentrated_pool(&factory, &router, pair, 500, 10, 100)
        .expect("pool creation with only 1 unregistered token should succeed");

    let escrow = factory
        .get_escrow(token_c.token.to_string())
        .expect("escrow for the new token must be created by pool creation ACK");
    assert!(
        escrow.denoms.contains(&token_c.token_type),
        "new token denom must be registered in its escrow after pool creation",
    );
}

#[rstest]
fn test_create_pool_with_both_tokens_unregistered_rejected(
    #[values(FactorySetupMode::Native, FactorySetupMode::Ibc, FactorySetupMode::Evm)]
    mode: FactorySetupMode,
    #[values("native", "smart")] token_x_kind: &str,
    #[values("native", "smart")] token_y_kind: &str,
) {
    // Intentionally do not register any tokens
    let (_interchain, factory, router) = setup_concentrated_base(mode, chain_id_for_mode(mode));

    let token_x = make_token(&factory, "conc.token.x", token_x_kind);
    let token_y = make_token(&factory, "conc.token.y", token_y_kind);

    let pair = pair_with_amounts(&token_x, &token_y, 20_000, 20_000);
    let err = create_concentrated_pool(&factory, &router, pair, 500, 10, 100)
        .expect_err("pool creation with both tokens unregistered must fail");
    let err_str = err.root().to_string();
    assert!(
        err_str.contains("Atleast one token must already be registered"),
        "expected both-new-tokens error, got: {err_str}",
    );
}

#[rstest]
fn test_create_pool_with_disallowed_token_rejected(
    #[values(FactorySetupMode::Native, FactorySetupMode::Ibc, FactorySetupMode::Evm)]
    mode: FactorySetupMode,
    #[values("native", "smart")] token_a_kind: &str,
    #[values("native", "smart")] token_b_kind: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env_ext(mode, chain_id_for_mode(mode), token_a_kind, token_b_kind);

    // Deregister token_b — escrow still exists but denom is now disallowed
    deregister_denom(&factory, &router, token_b.clone()).unwrap();

    let pair = pair_with_amounts(&token_a, &token_b, 20_000, 20_000);
    let err = create_concentrated_pool(&factory, &router, pair, 500, 10, 100)
        .expect_err("pool creation with a disallowed token must be rejected on factory call");
    let err_str = err.root().to_string();
    assert!(
        err_str.contains("UnsupportedDenomination"),
        "expected UnsupportedDenomination error, got: {err_str}",
    );
}

#[rstest]
fn test_create_pool_invalid_spacing_rejected(
    #[values(FactorySetupMode::Native, FactorySetupMode::Ibc, FactorySetupMode::Evm)]
    mode: FactorySetupMode,
    #[values("native", "smart", "voucher")] token_a_kind: &str,
    #[values("native", "smart", "voucher")] token_b_kind: &str,
) {
    let (_interchain, factory, _router, token_a, token_b) =
        setup_concentrated_env_ext(mode, chain_id_for_mode(mode), token_a_kind, token_b_kind);
    let pair = pair_with_amounts(&token_a, &token_b, 10_000, 10_000);

    let mut funds = vec![];
    for token in pair.get_vec_token_info() {
        faucet(
            factory.environment(),
            factory.environment().sender.as_str(),
            token.amount.u128(),
            token.token_type,
            &mut funds,
        );
    }

    let pool_key = concentrated_pool_key(&pair, 500, 11);
    let err = factory
        .execute(
            &euclid::msgs::factory::ExecuteMsg::RequestConcentratedPoolCreation {
                pair_with_denom_and_amount: pair,
                fee_tier_bps: 500,
                tick_spacing: 11,
                slippage_tolerance_bps: 100,
                initial_tick: None,
                cross_chain_config: CrossChainConfig::default(),
            },
            &funds,
        )
        .unwrap_err()
        .to_string();

    assert!(!err.is_empty(), "expected invalid spacing request to fail");
    assert!(
        factory.get_concentrated_vlp(pool_key).is_err(),
        "invalid pool must not be created",
    );
}
