#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::coin;
use cosmwasm_std::testing::{mock_dependencies, mock_env};
use cosmwasm_std::{Addr, Order, Storage, Uint128, Uint256};
use cw2::set_contract_version;
use cw_multi_test::Executor;
use euclid::{
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    fee::{DenomFees, Fee, TotalFees},
    msgs::vlp::{
        base::{PoolConfig, PoolKey, PoolType, State, VlpConcentratedRegisterPoolMsg},
        concentrated::msg::{
            ExecuteMsg, GetStateResponse, InstantiateMsg, LegacyLiquidityMode, MigrateMsg, QueryMsg,
        },
    },
    token::{Pair, Token},
};
use mock::{mock::mock_app, mock_builder::MockEuclidBuilder};

use crate::{
    contract::amounts_for_position_liquidity_with_bound,
    math::{liquidity_amounts::get_amounts_for_liquidity, tick_math::get_sqrt_ratio_at_tick},
    migrate::{fit_liquidity_with_bound, migrate},
    mock::mock_concentrated_vlp,
    state::{
        initialize_position_namespace_if_missing, next_position_id, ConcentratedPosition,
        MigrationMetadata, TickInfo, ACTIVE_LIQUIDITY, BALANCES, CHAIN_LP_TOKENS,
        FEE_GROWTH_GLOBAL_0_X128, FEE_GROWTH_GLOBAL_1_X128, MIGRATION_METADATA, MIGRATION_REVISION,
        OBSERVATIONS, POOL_KEY, POSITIONS, POSITION_ID_PREFIX, POSITION_NONCE, PROTOCOL_FEES_0,
        PROTOCOL_FEES_1, SLOT0, STATE, TICKS,
    },
};

#[test]
fn test_proper_instantiation() {
    let mut app = mock_app(None);
    let andr = MockEuclidBuilder::new(&mut app, "admin")
        .with_wallets(vec![("owner", vec![coin(1000, "eucl")])])
        .with_contracts(vec![("concentrated_vlp", mock_concentrated_vlp())])
        .build(&mut app);

    let owner = andr.get_wallet("owner");

    let pair = Pair::new(
        Token::create("tokena".to_string()).unwrap(),
        Token::create("tokenb".to_string()).unwrap(),
    )
    .unwrap();

    let fee = Fee::new(
        10,
        10,
        CrossChainUser::new(ChainUid::vsl_chain_uid().unwrap(), "recipient".to_string()),
    );

    let contract = app
        .instantiate_contract(
            1,
            owner.clone(),
            &InstantiateMsg {
                virtual_balance_contract: owner.clone(),
                pair: pair.clone(),
                fee: fee.clone(),
                execute: Some(ExecuteMsg::RegisterPool(VlpConcentratedRegisterPoolMsg {
                    sender: CrossChainUser::new(
                        ChainUid::create("chain1".to_string()).unwrap(),
                        "user".to_string(),
                    ),
                    pool_key: PoolKey {
                        pair: pair.clone(),
                        pool_type: PoolType::Concentrated {
                            fee_tier_bps: 500,
                            tick_spacing: 10,
                        },
                    },
                    tx_id: "tx1".to_string(),
                })),
                admin: owner.clone(),
                fee_tier_bps: 500,
                tick_spacing: 10,
            },
            &[],
            "Concentrated VLP",
            None,
        )
        .unwrap();

    let state: GetStateResponse = app
        .wrap()
        .query_wasm_smart(contract, &QueryMsg::State {})
        .unwrap();

    assert_eq!(state.pair, pair);
    assert_eq!(state.router, owner);
    assert_eq!(state.fee, fee);
    assert_eq!(
        state.pool_config,
        PoolConfig::Concentrated {
            fee_tier_bps: 500,
            tick_spacing: 10
        }
    );
    assert_eq!(
        state.total_fees_collected,
        TotalFees {
            lp_fees: DenomFees {
                totals: Default::default()
            },
            euclid_fees: DenomFees {
                totals: Default::default()
            }
        }
    );
}

fn sample_pair() -> Pair {
    Pair::new(
        Token::create("tokena".to_string()).unwrap(),
        Token::create("tokenb".to_string()).unwrap(),
    )
    .unwrap()
}

fn sample_pool_key(pair: Pair) -> PoolKey {
    PoolKey {
        pair,
        pool_type: PoolType::Concentrated {
            fee_tier_bps: 500,
            tick_spacing: 10,
        },
    }
}

fn sample_state(pair: Pair, total_lp_tokens: Uint128) -> State {
    let fee = Fee::new(
        500,
        0,
        CrossChainUser::new(ChainUid::vsl_chain_uid().unwrap(), "recipient".to_string()),
    );

    State {
        pair,
        router: Addr::unchecked("router"),
        virtual_balance_contract: Addr::unchecked("vb"),
        fee,
        total_fees_collected: TotalFees {
            lp_fees: DenomFees {
                totals: Default::default(),
            },
            euclid_fees: DenomFees {
                totals: Default::default(),
            },
        },
        last_updated: 0,
        total_lp_tokens,
    }
}

fn owner(chain: &str, address: &str) -> CrossChainUser {
    CrossChainUser::new(
        ChainUid::create(chain.to_string()).unwrap(),
        address.to_string(),
    )
}

fn make_position(
    owner: CrossChainUser,
    pool_key: PoolKey,
    lower_tick_index: i64,
    upper_tick_index: i64,
    liquidity: Uint128,
) -> ConcentratedPosition {
    ConcentratedPosition {
        owner,
        lower_tick_index,
        upper_tick_index,
        liquidity,
        pool_key,
        fee_growth_inside_0_last_x128: Uint256::zero(),
        fee_growth_inside_1_last_x128: Uint256::zero(),
        tokens_owed_0: Uint128::zero(),
        tokens_owed_1: Uint128::zero(),
    }
}

fn save_legacy_fixture(
    storage: &mut dyn Storage,
    prev_version: &str,
    state: State,
    pool_key: PoolKey,
    reserve_0: Uint128,
    reserve_1: Uint128,
    positions: Vec<(u128, ConcentratedPosition)>,
) {
    set_contract_version(storage, "crates.io:concentrated_vlp", prev_version).unwrap();
    STATE.save(storage, &state.clone()).unwrap();
    POOL_KEY.save(storage, &pool_key).unwrap();
    BALANCES
        .save(storage, state.pair.token_1.clone(), &reserve_0)
        .unwrap();
    BALANCES
        .save(storage, state.pair.token_2.clone(), &reserve_1)
        .unwrap();
    for (id, position) in positions {
        POSITIONS.save(storage, id, &position).unwrap();
    }
}

fn migrate_msg(mode: LegacyLiquidityMode) -> MigrateMsg {
    MigrateMsg {
        legacy_liquidity_mode: mode,
        expected_prev_version: None,
        force_rebuild: None,
    }
}

fn collect_ticks(storage: &dyn Storage) -> Vec<(i64, TickInfo)> {
    TICKS
        .range(storage, None, None, Order::Ascending)
        .map(|item| item.unwrap())
        .collect()
}

#[test]
fn migration_rebuilds_ticks_and_active_liquidity_from_positions() {
    let mut deps = mock_dependencies();
    let env = mock_env();

    let pair = sample_pair();
    let pool_key = sample_pool_key(pair.clone());
    let p1 = make_position(
        owner("andr", "alice"),
        pool_key.clone(),
        -10,
        10,
        Uint128::new(1000),
    );
    let p2 = make_position(
        owner("sepolia", "bob"),
        pool_key.clone(),
        10,
        20,
        Uint128::new(2000),
    );

    save_legacy_fixture(
        deps.as_mut().storage,
        "0.1.0",
        sample_state(pair, Uint128::new(999_999)),
        pool_key,
        Uint128::new(100_000),
        Uint128::new(100_000),
        vec![(1, p1), (2, p2)],
    );

    TICKS
        .save(
            deps.as_mut().storage,
            123,
            &TickInfo {
                initialized: true,
                liquidity_gross: Uint128::new(1),
                liquidity_net: 1,
                fee_growth_outside_0_x128: Uint256::zero(),
                fee_growth_outside_1_x128: Uint256::zero(),
            },
        )
        .unwrap();

    CHAIN_LP_TOKENS
        .save(
            deps.as_mut().storage,
            ChainUid::create("legacy".to_string()).unwrap(),
            &Uint128::new(7),
        )
        .unwrap();

    migrate(
        deps.as_mut(),
        env,
        migrate_msg(LegacyLiquidityMode::AlreadyV3Liquidity),
    )
    .unwrap();

    assert_eq!(
        ACTIVE_LIQUIDITY.load(deps.as_ref().storage).unwrap(),
        Uint128::new(1000)
    );
    assert_eq!(
        STATE.load(deps.as_ref().storage).unwrap().total_lp_tokens,
        Uint128::new(3000)
    );

    let ticks = collect_ticks(deps.as_ref().storage);
    assert_eq!(ticks.len(), 3);
    assert_eq!(ticks[0].0, -10);
    assert_eq!(ticks[0].1.liquidity_gross, Uint128::new(1000));
    assert_eq!(ticks[0].1.liquidity_net, 1000);
    assert_eq!(ticks[1].0, 10);
    assert_eq!(ticks[1].1.liquidity_gross, Uint128::new(3000));
    assert_eq!(ticks[1].1.liquidity_net, 1000);
    assert_eq!(ticks[2].0, 20);
    assert_eq!(ticks[2].1.liquidity_gross, Uint128::new(2000));
    assert_eq!(ticks[2].1.liquidity_net, -2000);
    assert!(TICKS
        .may_load(deps.as_ref().storage, 123)
        .unwrap()
        .is_none());

    assert_eq!(
        CHAIN_LP_TOKENS
            .load(
                deps.as_ref().storage,
                ChainUid::create("andr".to_string()).unwrap(),
            )
            .unwrap(),
        Uint128::new(1000)
    );
    assert_eq!(
        CHAIN_LP_TOKENS
            .load(
                deps.as_ref().storage,
                ChainUid::create("sepolia".to_string()).unwrap(),
            )
            .unwrap(),
        Uint128::new(2000)
    );
    assert!(CHAIN_LP_TOKENS
        .may_load(
            deps.as_ref().storage,
            ChainUid::create("legacy".to_string()).unwrap(),
        )
        .unwrap()
        .is_none());
}

#[test]
fn migration_legacy_share_mode_converts_liquidity_and_recomputes_totals() {
    let mut deps = mock_dependencies();
    let env = mock_env();

    let pair = sample_pair();
    let pool_key = sample_pool_key(pair.clone());
    let p1 = make_position(
        owner("andr", "alice"),
        pool_key.clone(),
        -10,
        10,
        Uint128::new(100),
    );
    let p2 = make_position(
        owner("andr", "bob"),
        pool_key.clone(),
        -10,
        10,
        Uint128::new(300),
    );

    let old_total = Uint128::new(400);
    save_legacy_fixture(
        deps.as_mut().storage,
        "0.0.1",
        sample_state(pair, old_total),
        pool_key,
        Uint128::new(1_000_000),
        Uint128::new(1_000_000),
        vec![(1, p1), (2, p2)],
    );

    migrate(
        deps.as_mut(),
        env,
        migrate_msg(LegacyLiquidityMode::LegacyShareProRata),
    )
    .unwrap();

    let p1_after = POSITIONS.load(deps.as_ref().storage, 1).unwrap();
    let p2_after = POSITIONS.load(deps.as_ref().storage, 2).unwrap();
    let new_total = STATE.load(deps.as_ref().storage).unwrap().total_lp_tokens;

    assert!(p1_after.liquidity > Uint128::zero());
    assert!(p2_after.liquidity > p1_after.liquidity);
    assert_eq!(
        new_total,
        p1_after.liquidity.checked_add(p2_after.liquidity).unwrap()
    );
    assert_ne!(new_total, old_total);
    assert_eq!(
        ACTIVE_LIQUIDITY.load(deps.as_ref().storage).unwrap(),
        new_total
    );
}

#[test]
fn migration_assigns_rounding_residuals_to_protocol_fees() {
    let mut deps = mock_dependencies();
    let env = mock_env();

    let pair = sample_pair();
    let pool_key = sample_pool_key(pair.clone());
    let position = make_position(
        owner("andr", "alice"),
        pool_key.clone(),
        -10,
        10,
        Uint128::new(1000),
    );

    save_legacy_fixture(
        deps.as_mut().storage,
        "0.1.0",
        sample_state(pair.clone(), Uint128::new(1000)),
        pool_key,
        Uint128::new(10_000),
        Uint128::new(10_000),
        vec![(9, position.clone())],
    );

    PROTOCOL_FEES_0
        .save(deps.as_mut().storage, &Uint128::new(5))
        .unwrap();
    PROTOCOL_FEES_1
        .save(deps.as_mut().storage, &Uint128::new(7))
        .unwrap();

    migrate(
        deps.as_mut(),
        env,
        migrate_msg(LegacyLiquidityMode::AlreadyV3Liquidity),
    )
    .unwrap();

    let slot0 = SLOT0.load(deps.as_ref().storage).unwrap();
    let sqrt_lower = get_sqrt_ratio_at_tick(position.lower_tick_index).unwrap();
    let sqrt_upper = get_sqrt_ratio_at_tick(position.upper_tick_index).unwrap();
    let (implied_0_u256, implied_1_u256) = get_amounts_for_liquidity(
        slot0.sqrt_price_x96,
        sqrt_lower,
        sqrt_upper,
        position.liquidity,
        false,
    )
    .unwrap();
    let implied_0 = Uint128::try_from(implied_0_u256).unwrap();
    let implied_1 = Uint128::try_from(implied_1_u256).unwrap();

    let expected_pf0 = Uint128::new(5)
        .checked_add(Uint128::new(10_000).checked_sub(implied_0).unwrap())
        .unwrap();
    let expected_pf1 = Uint128::new(7)
        .checked_add(Uint128::new(10_000).checked_sub(implied_1).unwrap())
        .unwrap();

    assert_eq!(
        PROTOCOL_FEES_0.load(deps.as_ref().storage).unwrap(),
        expected_pf0
    );
    assert_eq!(
        PROTOCOL_FEES_1.load(deps.as_ref().storage).unwrap(),
        expected_pf1
    );
}

#[test]
fn migration_rejects_invalid_tick_alignment_or_bounds() {
    let mut deps = mock_dependencies();
    let env = mock_env();

    let pair = sample_pair();
    let pool_key = sample_pool_key(pair.clone());
    let invalid_position = make_position(
        owner("andr", "alice"),
        pool_key.clone(),
        -7,
        10,
        Uint128::new(10),
    );

    save_legacy_fixture(
        deps.as_mut().storage,
        "0.0.1",
        sample_state(pair, Uint128::new(10)),
        pool_key,
        Uint128::new(10_000),
        Uint128::new(10_000),
        vec![(1, invalid_position)],
    );

    let err = migrate(
        deps.as_mut(),
        env,
        migrate_msg(LegacyLiquidityMode::AlreadyV3Liquidity),
    )
    .unwrap_err();
    assert!(err.to_string().contains("tick not aligned"));
}

#[test]
fn migration_rejects_unknown_source_version() {
    let mut deps = mock_dependencies();
    let env = mock_env();

    let pair = sample_pair();
    let pool_key = sample_pool_key(pair.clone());

    save_legacy_fixture(
        deps.as_mut().storage,
        "9.9.9",
        sample_state(pair, Uint128::zero()),
        pool_key,
        Uint128::zero(),
        Uint128::zero(),
        vec![],
    );

    let err = migrate(
        deps.as_mut(),
        env,
        migrate_msg(LegacyLiquidityMode::AlreadyV3Liquidity),
    )
    .unwrap_err();
    assert!(err
        .to_string()
        .contains("unsupported migration source version"));
}

#[test]
fn migration_idempotent_noop_without_force() {
    let mut deps = mock_dependencies();
    let env = mock_env();

    let pair = sample_pair();
    let pool_key = sample_pool_key(pair.clone());
    let position = make_position(
        owner("andr", "alice"),
        pool_key.clone(),
        -10,
        10,
        Uint128::new(100),
    );

    save_legacy_fixture(
        deps.as_mut().storage,
        "0.1.0",
        sample_state(pair, Uint128::new(100)),
        pool_key,
        Uint128::new(1_000),
        Uint128::new(1_000),
        vec![(1, position)],
    );

    migrate(
        deps.as_mut(),
        env.clone(),
        migrate_msg(LegacyLiquidityMode::AlreadyV3Liquidity),
    )
    .unwrap();
    let metadata_before = MIGRATION_METADATA.load(deps.as_ref().storage).unwrap();

    let second = migrate(
        deps.as_mut(),
        env,
        migrate_msg(LegacyLiquidityMode::AlreadyV3Liquidity),
    )
    .unwrap();

    assert!(second
        .attributes
        .iter()
        .any(|a| a.key == "action" && a.value == "migrate_concentrated_vlp_noop"));
    assert_eq!(
        MIGRATION_METADATA.load(deps.as_ref().storage).unwrap(),
        metadata_before
    );
}

#[test]
fn migration_force_rebuild_is_stable() {
    let mut deps = mock_dependencies();
    let env = mock_env();

    let pair = sample_pair();
    let pool_key = sample_pool_key(pair.clone());
    let position = make_position(
        owner("andr", "alice"),
        pool_key.clone(),
        -10,
        10,
        Uint128::new(100),
    );

    save_legacy_fixture(
        deps.as_mut().storage,
        "0.1.0",
        sample_state(pair, Uint128::new(100)),
        pool_key,
        Uint128::new(1_000),
        Uint128::new(1_000),
        vec![(1, position)],
    );

    migrate(
        deps.as_mut(),
        env.clone(),
        migrate_msg(LegacyLiquidityMode::AlreadyV3Liquidity),
    )
    .unwrap();

    let total_before = STATE.load(deps.as_ref().storage).unwrap().total_lp_tokens;
    let active_before = ACTIVE_LIQUIDITY.load(deps.as_ref().storage).unwrap();
    let ticks_before = collect_ticks(deps.as_ref().storage);

    let force_msg = MigrateMsg {
        legacy_liquidity_mode: LegacyLiquidityMode::AlreadyV3Liquidity,
        expected_prev_version: None,
        force_rebuild: Some(true),
    };
    migrate(deps.as_mut(), env, force_msg).unwrap();

    assert_eq!(
        STATE.load(deps.as_ref().storage).unwrap().total_lp_tokens,
        total_before
    );
    assert_eq!(
        ACTIVE_LIQUIDITY.load(deps.as_ref().storage).unwrap(),
        active_before
    );
    assert_eq!(collect_ticks(deps.as_ref().storage), ticks_before);
}

#[test]
fn migration_preserves_position_ids_and_owner() {
    let mut deps = mock_dependencies();
    let env = mock_env();

    let pair = sample_pair();
    let pool_key = sample_pool_key(pair.clone());

    let id_1 = (123u128 << 64) | 44u128;
    let id_2 = (123u128 << 64) | 45u128;

    let pos_1 = make_position(
        owner("andr", "alice"),
        pool_key.clone(),
        -10,
        10,
        Uint128::new(150),
    );
    let pos_2 = make_position(
        owner("sepolia", "bob"),
        pool_key.clone(),
        10,
        20,
        Uint128::new(200),
    );

    save_legacy_fixture(
        deps.as_mut().storage,
        "0.1.0",
        sample_state(pair, Uint128::new(350)),
        pool_key,
        Uint128::new(100_000),
        Uint128::new(100_000),
        vec![(id_1, pos_1.clone()), (id_2, pos_2.clone())],
    );

    migrate(
        deps.as_mut(),
        env,
        migrate_msg(LegacyLiquidityMode::AlreadyV3Liquidity),
    )
    .unwrap();

    let out_1 = POSITIONS.load(deps.as_ref().storage, id_1).unwrap();
    let out_2 = POSITIONS.load(deps.as_ref().storage, id_2).unwrap();
    assert_eq!(out_1.owner, pos_1.owner);
    assert_eq!(out_2.owner, pos_2.owner);
    assert_eq!(out_1.lower_tick_index, pos_1.lower_tick_index);
    assert_eq!(out_2.upper_tick_index, pos_2.upper_tick_index);

    assert!(POSITION_ID_PREFIX
        .may_load(deps.as_ref().storage)
        .unwrap()
        .is_some());
    assert!(POSITION_NONCE
        .may_load(deps.as_ref().storage)
        .unwrap()
        .is_some());
}

#[test]
fn migration_status_markers_are_written() {
    let mut deps = mock_dependencies();
    let env = mock_env();

    let pair = sample_pair();
    let pool_key = sample_pool_key(pair.clone());

    save_legacy_fixture(
        deps.as_mut().storage,
        "0.1.0",
        sample_state(pair, Uint128::zero()),
        pool_key,
        Uint128::new(100),
        Uint128::new(100),
        vec![],
    );

    migrate(
        deps.as_mut(),
        env,
        migrate_msg(LegacyLiquidityMode::AlreadyV3Liquidity),
    )
    .unwrap();

    assert_eq!(MIGRATION_REVISION.load(deps.as_ref().storage).unwrap(), 2);
    let metadata: MigrationMetadata = MIGRATION_METADATA.load(deps.as_ref().storage).unwrap();
    assert_eq!(metadata.source_version, "0.1.0");
    assert_eq!(metadata.mode, LegacyLiquidityMode::AlreadyV3Liquidity);
    assert_eq!(metadata.positions_migrated, 0);
    assert_eq!(
        FEE_GROWTH_GLOBAL_0_X128
            .load(deps.as_ref().storage)
            .unwrap(),
        Uint256::zero()
    );
    assert_eq!(
        FEE_GROWTH_GLOBAL_1_X128
            .load(deps.as_ref().storage)
            .unwrap(),
        Uint256::zero()
    );
    assert!(OBSERVATIONS
        .may_load(deps.as_ref().storage, 0)
        .unwrap()
        .is_some());
}

// ---------------------------------------------------------------------------
// next_position_id — table-driven
// ---------------------------------------------------------------------------

#[test]
fn next_position_id_table() {
    struct Case {
        label: &'static str,
        /// Position IDs to pre-populate (simulates existing positions).
        existing: Vec<u128>,
        /// Expected nonce component of the returned ID (lower 64 bits).
        expected_nonce: u64,
    }

    let cases = vec![
        Case {
            label: "empty storage yields nonce 1",
            existing: vec![],
            expected_nonce: 1,
        },
        Case {
            label: "skips occupied nonce 1, returns nonce 2",
            existing: vec![1], // nonce 1 under same prefix
            expected_nonce: 2,
        },
        Case {
            label: "skips occupied nonces 1-3, returns nonce 4",
            existing: vec![1, 2, 3],
            expected_nonce: 4,
        },
    ];

    for case in cases {
        let mut deps = mock_dependencies();
        let prefix: u64 = 42;
        POSITION_ID_PREFIX
            .save(deps.as_mut().storage, &prefix)
            .unwrap();
        POSITION_NONCE.save(deps.as_mut().storage, &0).unwrap();

        let pool_key = sample_pool_key(sample_pair());
        for nonce in &case.existing {
            let id = (u128::from(prefix) << 64) | u128::from(*nonce);
            let pos = make_position(
                owner("andr", "alice"),
                pool_key.clone(),
                -10,
                10,
                Uint128::new(100),
            );
            POSITIONS.save(deps.as_mut().storage, id, &pos).unwrap();
        }

        let result = next_position_id(deps.as_mut().storage).unwrap();
        let got_nonce = result.u128() as u64;
        assert_eq!(
            got_nonce, case.expected_nonce,
            "FAIL [{}]: expected nonce {}, got {}",
            case.label, case.expected_nonce, got_nonce,
        );
    }
}

#[test]
fn next_position_id_exhaustion_returns_error() {
    let mut deps = mock_dependencies();
    let prefix: u64 = 1;
    POSITION_ID_PREFIX
        .save(deps.as_mut().storage, &prefix)
        .unwrap();
    POSITION_NONCE.save(deps.as_mut().storage, &0).unwrap();

    let pool_key = sample_pool_key(sample_pair());
    // Fill nonces 1..=1024 so the loop exhausts all attempts.
    for nonce in 1..=1024u64 {
        let id = (u128::from(prefix) << 64) | u128::from(nonce);
        let pos = make_position(
            owner("andr", "alice"),
            pool_key.clone(),
            -10,
            10,
            Uint128::new(1),
        );
        POSITIONS.save(deps.as_mut().storage, id, &pos).unwrap();
    }

    let err = next_position_id(deps.as_mut().storage).unwrap_err();
    assert!(
        err.to_string().contains("exhausted"),
        "expected exhaustion error, got: {err}"
    );
}

// ---------------------------------------------------------------------------
// initialize_position_namespace_if_missing — table-driven
// ---------------------------------------------------------------------------

#[test]
fn initialize_position_namespace_table() {
    struct Case {
        label: &'static str,
        /// If Some, pre-save POSITION_NONCE to this value.
        pre_nonce: Option<u64>,
        /// Position IDs to pre-populate.
        existing_ids: Vec<u128>,
        /// Expected POSITION_NONCE value after the call.
        expected_nonce: u64,
    }

    let contract_addr = "wasm1concentrated";
    let cases = vec![
        Case {
            label: "early return when nonce already set",
            pre_nonce: Some(99),
            existing_ids: vec![],
            expected_nonce: 99,
        },
        Case {
            label: "fresh state with no positions yields nonce 0",
            pre_nonce: None,
            existing_ids: vec![],
            expected_nonce: 0,
        },
    ];

    for case in cases {
        let mut deps = mock_dependencies();
        if let Some(nonce) = case.pre_nonce {
            POSITION_NONCE.save(deps.as_mut().storage, &nonce).unwrap();
        }
        let pool_key = sample_pool_key(sample_pair());
        for id in &case.existing_ids {
            let pos = make_position(
                owner("andr", "alice"),
                pool_key.clone(),
                -10,
                10,
                Uint128::new(1),
            );
            POSITIONS.save(deps.as_mut().storage, *id, &pos).unwrap();
        }

        initialize_position_namespace_if_missing(deps.as_mut().storage, contract_addr).unwrap();

        let nonce = POSITION_NONCE.load(deps.as_ref().storage).unwrap();
        assert_eq!(
            nonce, case.expected_nonce,
            "FAIL [{}]: expected nonce {}, got {}",
            case.label, case.expected_nonce, nonce,
        );
    }
}

// ---------------------------------------------------------------------------
// amounts_for_position_liquidity_with_bound — table-driven
// ---------------------------------------------------------------------------

/// Helper: sqrt prices for the standard tick range [-10, 10] around tick 0.
fn sqrt_prices_for_range() -> (Uint256, Uint256, Uint256) {
    let sqrt_price = get_sqrt_ratio_at_tick(0).unwrap(); // price = 1.0
    let sqrt_lower = get_sqrt_ratio_at_tick(-10).unwrap();
    let sqrt_upper = get_sqrt_ratio_at_tick(10).unwrap();
    (sqrt_price, sqrt_lower, sqrt_upper)
}

#[test]
fn liquidity_bound_search_table() {
    let (sqrt_price, sqrt_lower, sqrt_upper) = sqrt_prices_for_range();

    struct Case {
        label: &'static str,
        liquidity: u128,
        max_0: u128,
        max_1: u128,
        /// If true, the target liquidity should fit without binary search.
        expect_exact_fit: bool,
    }

    let cases = vec![
        Case {
            label: "zero liquidity returns zero",
            liquidity: 0,
            max_0: 1_000_000,
            max_1: 1_000_000,
            expect_exact_fit: true,
        },
        Case {
            label: "generous bounds fit exactly",
            liquidity: 1_000,
            max_0: u128::MAX / 2,
            max_1: u128::MAX / 2,
            expect_exact_fit: true,
        },
        Case {
            label: "tight bounds force binary search reduction",
            liquidity: 1_000_000,
            max_0: 10,
            max_1: 10,
            expect_exact_fit: false,
        },
    ];

    for case in cases {
        let liq = Uint128::new(case.liquidity);
        let max_0 = Uint128::new(case.max_0);
        let max_1 = Uint128::new(case.max_1);

        // Test both functions (contract path and migrate path) — identical logic.
        for (func_label, result) in [
            (
                "amounts_for_position_liquidity_with_bound",
                amounts_for_position_liquidity_with_bound(
                    sqrt_price, sqrt_lower, sqrt_upper, liq, max_0, max_1,
                ),
            ),
            (
                "fit_liquidity_with_bound",
                fit_liquidity_with_bound(sqrt_price, sqrt_lower, sqrt_upper, liq, max_0, max_1),
            ),
        ] {
            let (fitted_liq, a0, a1) =
                result.unwrap_or_else(|e| panic!("FAIL [{} / {}]: {e}", case.label, func_label));

            assert!(
                a0 <= max_0,
                "FAIL [{} / {}]: a0 {a0} exceeds max {max_0}",
                case.label,
                func_label,
            );
            assert!(
                a1 <= max_1,
                "FAIL [{} / {}]: a1 {a1} exceeds max {max_1}",
                case.label,
                func_label,
            );

            if case.expect_exact_fit {
                assert_eq!(
                    fitted_liq, liq,
                    "FAIL [{} / {}]: expected exact fit",
                    case.label, func_label,
                );
            } else {
                assert!(
                    fitted_liq < liq,
                    "FAIL [{} / {}]: expected reduced liquidity, got {fitted_liq} >= {liq}",
                    case.label,
                    func_label,
                );
                assert!(
                    fitted_liq > Uint128::zero(),
                    "FAIL [{} / {}]: fitted liquidity should be > 0",
                    case.label,
                    func_label,
                );
            }
        }
    }
}
