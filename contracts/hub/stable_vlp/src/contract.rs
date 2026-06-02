use std::collections::HashMap;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{ensure, Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, Uint256};
use cw2::set_contract_version;
use euclid::fee::{DenomFees, TotalFees};

use crate::query::{
    query_admin, query_all_pools, query_fee, query_liquidity, query_pool, query_simulate_swap,
    query_state, query_total_fees_collected, query_total_fees_per_denom,
};
use crate::reply;
use crate::state::{ADMIN, AMP_FACTOR, BALANCES, CHAIN_LP_TOKENS, COLLATERAL_LP_TOKENS, STATE};
use euclid::error::ContractError;
use euclid::msgs::vlp::base::{State, NEXT_SWAP_REPLY_ID};
use euclid::msgs::vlp::stable::msg::{ExecuteMsg, InstantiateMsg, QueryMsg, DEFAULT_AMP_FACTOR};
use euclid_pool::{
    common::{register_pool, remove_liquidity, update_admin, update_amp_factor, update_fee},
    stable::{add_liquidity, execute_swap},
};
// version info for migration info
pub(crate) const CONTRACT_NAME: &str = "crates.io:stable_vlp";
pub(crate) const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    // Validate token pair
    msg.pair.validate()?;
    let token_1 = msg.pair.token_1.to_string();
    let token_2 = msg.pair.token_2.to_string();

    let state = State {
        pair: msg.pair,
        virtual_balance_contract: msg.virtual_balance_contract,
        router: info.sender.clone(),
        fee: msg.fee,
        total_fees_collected: TotalFees {
            lp_fees: DenomFees {
                totals: HashMap::default(),
            },
            euclid_fees: DenomFees {
                totals: HashMap::default(),
            },
        },
        last_updated: 0,
        total_lp_tokens: Uint256::zero(),
    };

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    STATE.save(deps.storage, &state)?;
    ADMIN.save(deps.storage, &msg.admin)?;

    BALANCES.save(deps.storage, state.pair.token_1, &Uint256::zero())?;
    BALANCES.save(deps.storage, state.pair.token_2, &Uint256::zero())?;

    let amp_factor = msg.amp_factor.unwrap_or(DEFAULT_AMP_FACTOR);
    ensure!(
        amp_factor.u64() >= euclid_pool::stable_math::MIN_AMP,
        ContractError::new(&format!(
            "Amp factor must be at least {}",
            euclid_pool::stable_math::MIN_AMP
        ))
    );
    AMP_FACTOR.save(deps.storage, &amp_factor)?;

    let response =
        msg.execute
            .map_or(Ok(Response::default()), |execute_msg| match execute_msg {
                ExecuteMsg::RegisterPool(register_pool_msg) => register_pool(
                    deps,
                    env.clone(),
                    info.clone(),
                    &STATE,
                    &CHAIN_LP_TOKENS,
                    register_pool_msg.sender,
                    register_pool_msg.pair,
                    register_pool_msg.tx_id,
                )
                .map(|r| {
                    r.add_attribute("pool_type", "stable")
                        .add_attribute("amp_factor", amp_factor.to_string())
                }),
                _ => Err(ContractError::Unauthorized {}),
            })?;

    Ok(response
        .add_attribute("method", "instantiate")
        .add_attribute("vlp_address", env.contract.address.to_string())
        .add_attribute("owner", info.sender)
        .add_attribute("pool_type", "stable")
        .add_attribute("token_1", token_1)
        .add_attribute("token_2", token_2))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::RegisterPool(register_pool_msg) => {
            let amp_factor = AMP_FACTOR.load(deps.storage).unwrap_or(DEFAULT_AMP_FACTOR);
            register_pool(
                deps,
                env,
                info,
                &STATE,
                &CHAIN_LP_TOKENS,
                register_pool_msg.sender,
                register_pool_msg.pair,
                register_pool_msg.tx_id,
            )
            .map(|r| {
                r.add_attribute("pool_type", "stable")
                    .add_attribute("amp_factor", amp_factor.to_string())
            })
        }
        ExecuteMsg::UpdateFee {
            lp_fee_bps,
            euclid_fee_bps,
            recipient,
        } => update_fee(
            deps,
            info,
            &STATE,
            &ADMIN,
            lp_fee_bps,
            euclid_fee_bps,
            recipient,
        ),
        ExecuteMsg::AddLiquidity(add_liquidity_msg) => {
            let amp_factor = AMP_FACTOR.load(deps.storage)?;
            add_liquidity(
                deps,
                env,
                info,
                &STATE,
                &BALANCES,
                &CHAIN_LP_TOKENS,
                &COLLATERAL_LP_TOKENS,
                add_liquidity_msg.sender,
                add_liquidity_msg.liquidity,
                add_liquidity_msg.slippage_tolerance_bps,
                amp_factor,
                add_liquidity_msg.tx_id,
            )
        }
        ExecuteMsg::RemoveLiquidity(remove_liquidity_msg) => remove_liquidity(
            deps,
            env,
            info,
            &STATE,
            &BALANCES,
            &CHAIN_LP_TOKENS,
            remove_liquidity_msg.sender,
            remove_liquidity_msg.lp_allocation,
            remove_liquidity_msg.tx_id,
        ),
        ExecuteMsg::Swap(swap_msg) => {
            let amp_factor = AMP_FACTOR.load(deps.storage).unwrap_or(DEFAULT_AMP_FACTOR);
            execute_swap(
                deps,
                env,
                info,
                &STATE,
                &BALANCES,
                swap_msg.sender,
                swap_msg.asset_in,
                swap_msg.amount_in,
                swap_msg.min_token_out,
                swap_msg.tx_id,
                swap_msg.next_swaps,
                amp_factor,
                swap_msg.test_fail,
            )
        }
        ExecuteMsg::UpdateAdmin { admin, admin_type } => {
            update_admin(deps, env, info, &ADMIN, admin, admin_type)
        }
        ExecuteMsg::UpdateAmpFactor { amp_factor } => {
            update_amp_factor(deps, info, &ADMIN, &AMP_FACTOR, amp_factor)
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::State {} => query_state(deps),
        QueryMsg::GetAdmin {} => query_admin(deps),
        QueryMsg::SimulateSwap(simulate_swap_msg) => query_simulate_swap(
            deps,
            simulate_swap_msg.asset,
            simulate_swap_msg.asset_amount,
            simulate_swap_msg.swaps,
        ),
        QueryMsg::Liquidity {} => query_liquidity(deps, env),
        QueryMsg::Fee {} => query_fee(deps),
        QueryMsg::TotalFeesCollected {} => query_total_fees_collected(deps),
        QueryMsg::TotalFeesPerDenom { denom } => query_total_fees_per_denom(deps, denom),
        QueryMsg::Pool { chain_uid } => query_pool(deps, chain_uid),

        QueryMsg::GetAllPools {} => query_all_pools(deps),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        NEXT_SWAP_REPLY_ID => reply::on_next_swap_reply(deps, msg),

        id => Err(ContractError::Generic {
            err: format!("Unknown reply id: {id}"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        contract::{execute, instantiate},
        state::{ADMIN, AMP_FACTOR, BALANCES, CHAIN_LP_TOKENS, COLLATERAL_LP_TOKENS, STATE},
        testing::helpers::{
            chain1, chain2, default_fee, default_pair, init, register_pool, seed_liquidity,
            sender_on_chain1, sender_on_chain2, token1, token2,
        },
    };
    use cosmwasm_std::{
        attr,
        testing::{message_info, mock_dependencies, mock_env},
        Addr, Uint256, Uint64,
    };
    use euclid::{
        admin::{AdminType, EuclidAdmin},
        chain::ChainUid,
        cross_chain_user::CrossChainUser,
        error::ContractError,
        fee::Fee,
        msgs::vlp::{
            base::{VlpAddLiquidityMsg, VlpRegisterPoolMsg, VlpRemoveLiquidityMsg, VlpSwapMsg},
            stable::msg::{ExecuteMsg, InstantiateMsg, DEFAULT_AMP_FACTOR},
        },
        token::{Pair, PairWithAmount, Token, TokenWithAmount},
    };
    use rstest::rstest;

    // -----------------------------------------------------------------------
    // Instantiation tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_init_state_is_written_correctly() {
        let mut deps = mock_dependencies();
        let res = init(&mut deps);

        assert_eq!(0, res.messages.len());

        let router = deps.api.addr_make("router");
        let admin = EuclidAdmin::default(deps.api.addr_make("admin"));

        let state = STATE.load(&deps.storage).unwrap();
        assert_eq!(state.pair, default_pair());
        assert_eq!(state.router, router);
        assert_eq!(
            state.virtual_balance_contract,
            Addr::unchecked("virtual_balance_contract")
        );
        assert_eq!(state.fee, default_fee());
        assert_eq!(state.total_lp_tokens, Uint256::zero());
        assert_eq!(state.last_updated, 0);

        let saved_admin = ADMIN.load(&deps.storage).unwrap();
        assert_eq!(saved_admin, admin);

        let amp = AMP_FACTOR.load(&deps.storage).unwrap();
        assert_eq!(amp, Uint64::from(1000u64));
    }

    #[test]
    fn test_init_balances_are_zeroed() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let b1 = BALANCES.load(&deps.storage, token1()).unwrap();
        let b2 = BALANCES.load(&deps.storage, token2()).unwrap();
        assert_eq!(b1, Uint256::zero());
        assert_eq!(b2, Uint256::zero());
    }

    #[test]
    fn test_init_default_amp_factor_used_when_none_provided() {
        let mut deps = mock_dependencies();
        let router = deps.api.addr_make("router");
        let admin = EuclidAdmin::default(deps.api.addr_make("admin"));
        let msg = InstantiateMsg {
            router: Addr::unchecked("router"),
            virtual_balance_contract: Addr::unchecked("virtual_balance_contract"),
            pair: default_pair(),
            fee: default_fee(),
            execute: None,
            admin,
            amp_factor: None,
        };
        let info = message_info(&router, &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let amp = AMP_FACTOR.load(&deps.storage).unwrap();
        assert_eq!(amp, DEFAULT_AMP_FACTOR);
    }

    #[test]
    fn test_init_response_attributes_present() {
        let mut deps = mock_dependencies();
        let res = init(&mut deps);

        assert!(res.attributes.contains(&attr("method", "instantiate")));
        assert!(res.attributes.iter().any(|a| a.key == "vlp_address"));
        assert!(res.attributes.iter().any(|a| a.key == "owner"));
    }

    #[test]
    fn test_init_invalid_pair_duplicate_tokens_fails() {
        let mut deps = mock_dependencies();
        let router = deps.api.addr_make("router");
        let admin = EuclidAdmin::default(deps.api.addr_make("admin"));
        let msg = InstantiateMsg {
            router: Addr::unchecked("router"),
            virtual_balance_contract: Addr::unchecked("virtual_balance_contract"),
            pair: Pair {
                token_1: token1(),
                token_2: token1(),
            },
            fee: default_fee(),
            execute: None,
            admin,
            amp_factor: None,
        };
        let info = message_info(&router, &[]);
        let err = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::DuplicateTokens {});
    }

    #[test]
    fn test_init_with_register_pool_execute_msg() {
        let mut deps = mock_dependencies();
        let router = deps.api.addr_make("router");
        let admin = EuclidAdmin::default(deps.api.addr_make("admin"));
        let msg = InstantiateMsg {
            router: Addr::unchecked("router"),
            virtual_balance_contract: Addr::unchecked("virtual_balance_contract"),
            pair: default_pair(),
            fee: default_fee(),
            execute: Some(ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
                sender: sender_on_chain1(),
                pair: default_pair(),
                tx_id: "init-reg".to_string(),
            })),
            admin,
            amp_factor: Some(Uint64::from(1000u64)),
        };
        let info = message_info(&router, &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let lp = CHAIN_LP_TOKENS.load(&deps.storage, chain1()).unwrap();
        assert_eq!(lp, Uint256::zero());
    }

    #[test]
    fn test_init_with_non_register_pool_execute_msg_is_unauthorized() {
        let mut deps = mock_dependencies();
        let router = deps.api.addr_make("router");
        let admin = EuclidAdmin::default(deps.api.addr_make("admin"));
        let msg = InstantiateMsg {
            router: Addr::unchecked("router"),
            virtual_balance_contract: Addr::unchecked("virtual_balance_contract"),
            pair: default_pair(),
            fee: default_fee(),
            execute: Some(ExecuteMsg::UpdateAmpFactor {
                amp_factor: Uint64::from(500u64),
            }),
            admin,
            amp_factor: None,
        };
        let info = message_info(&router, &[]);
        let err = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    #[test]
    fn test_init_rejects_amp_below_min() {
        let mut deps = mock_dependencies();
        let router = deps.api.addr_make("router");
        let admin = EuclidAdmin::default(deps.api.addr_make("admin"));
        let msg = InstantiateMsg {
            router: Addr::unchecked("router"),
            virtual_balance_contract: Addr::unchecked("virtual_balance_contract"),
            pair: default_pair(),
            fee: default_fee(),
            execute: None,
            admin,
            amp_factor: Some(Uint64::from(49u64)),
        };
        let info = message_info(&router, &[]);
        let err = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert!(
            err.to_string().contains("Amp factor must be at least"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_init_accepts_amp_at_min() {
        let mut deps = mock_dependencies();
        let router = deps.api.addr_make("router");
        let admin = EuclidAdmin::default(deps.api.addr_make("admin"));
        let msg = InstantiateMsg {
            router: Addr::unchecked("router"),
            virtual_balance_contract: Addr::unchecked("virtual_balance_contract"),
            pair: default_pair(),
            fee: default_fee(),
            execute: None,
            admin,
            amp_factor: Some(Uint64::from(euclid_pool::stable_math::MIN_AMP)),
        };
        let info = message_info(&router, &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        let amp = AMP_FACTOR.load(&deps.storage).unwrap();
        assert_eq!(amp.u64(), euclid_pool::stable_math::MIN_AMP);
    }

    // -----------------------------------------------------------------------
    // RegisterPool tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_register_pool_happy_path() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);
        let msg = ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
            sender: sender_on_chain1(),
            pair: default_pair(),
            tx_id: "tx-1".to_string(),
        });

        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(res.messages.len(), 0);

        let lp = CHAIN_LP_TOKENS.load(&deps.storage, chain1()).unwrap();
        assert_eq!(lp, Uint256::zero());
    }

    #[test]
    fn test_register_pool_unauthorized_non_router() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let non_router = deps.api.addr_make("not_router");
        let info = message_info(&non_router, &[]);
        let msg = ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
            sender: sender_on_chain1(),
            pair: default_pair(),
            tx_id: "tx-1".to_string(),
        });

        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    #[test]
    fn test_register_pool_duplicate_chain_fails() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        register_pool(&mut deps, chain1(), "tx-1");

        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);
        let msg = ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
            sender: sender_on_chain1(),
            pair: default_pair(),
            tx_id: "tx-2".to_string(),
        });
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::PoolAlreadyExists {});
    }

    #[test]
    fn test_register_pool_wrong_pair_fails() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);
        let wrong_pair = Pair {
            token_1: Token::create("atoken".to_string()).unwrap(),
            token_2: Token::create("btoken".to_string()).unwrap(),
        };
        let msg = ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
            sender: sender_on_chain1(),
            pair: wrong_pair,
            tx_id: "tx-1".to_string(),
        });
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::AssetDoesNotExist {});
    }

    #[test]
    fn test_register_pool_response_attributes() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);
        let msg = ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
            sender: sender_on_chain1(),
            pair: default_pair(),
            tx_id: "tx-1".to_string(),
        });
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        assert!(res.attributes.contains(&attr("action", "register_pool")));
        assert!(res.attributes.contains(&attr("pool_type", "stable")));
        assert!(res.attributes.iter().any(|a| a.key == "amp_factor"));
    }

    // -----------------------------------------------------------------------
    // UpdateFee tests
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::unauthorized_non_admin("not_admin", Some(5), Some(4), Some(ContractError::Unauthorized {}))]
    #[case::lp_fee_exceeds_max(
        "admin",
        Some(5000),
        Some(4),
        Some(ContractError::new("LP Fee cannot exceed maximum limit"))
    )]
    #[case::euclid_fee_exceeds_max(
        "admin",
        Some(50),
        Some(4000),
        Some(ContractError::new("Euclid Fee cannot exceed maximum limit"))
    )]
    #[case::happy_path("admin", Some(5), Some(4), None)]
    fn test_update_fee(
        #[case] sender: &str,
        #[case] lp_fee_bps: Option<u64>,
        #[case] euclid_fee_bps: Option<u64>,
        #[case] expected_error: Option<ContractError>,
    ) {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let sender_addr = deps.api.addr_make(sender);
        let info = message_info(&sender_addr, &[]);
        let msg = ExecuteMsg::UpdateFee {
            lp_fee_bps,
            euclid_fee_bps,
            recipient: Some(CrossChainUser::new(chain2(), "addr_2".to_string())),
        };

        let res = execute(deps.as_mut(), env, info, msg);
        match expected_error {
            Some(err) => assert_eq!(res.unwrap_err(), err),
            None => {
                res.unwrap();
                let fee = STATE.load(&deps.storage).unwrap().fee;
                assert_eq!(fee.lp_fee_bps, lp_fee_bps.unwrap());
                assert_eq!(fee.euclid_fee_bps, euclid_fee_bps.unwrap());
            }
        }
    }

    #[test]
    fn test_update_fee_partial_update_only_lp_fee() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let admin = deps.api.addr_make("admin");
        let info = message_info(&admin, &[]);
        let original_fee = STATE.load(&deps.storage).unwrap().fee;

        let msg = ExecuteMsg::UpdateFee {
            lp_fee_bps: Some(7),
            euclid_fee_bps: None,
            recipient: None,
        };
        execute(deps.as_mut(), env, info, msg).unwrap();

        let fee = STATE.load(&deps.storage).unwrap().fee;
        assert_eq!(fee.lp_fee_bps, 7);
        assert_eq!(fee.euclid_fee_bps, original_fee.euclid_fee_bps);
        assert_eq!(fee.recipient, original_fee.recipient);
    }

    #[test]
    fn test_update_fee_response_attributes() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let admin = deps.api.addr_make("admin");
        let info = message_info(&admin, &[]);
        let msg = ExecuteMsg::UpdateFee {
            lp_fee_bps: Some(3),
            euclid_fee_bps: Some(2),
            recipient: None,
        };
        let res = execute(deps.as_mut(), env, info, msg).unwrap();
        assert!(res.attributes.contains(&attr("action", "update_fee")));
        assert!(res.attributes.contains(&attr("lp_fee_bps", "3")));
        assert!(res.attributes.contains(&attr("euclid_fee_bps", "2")));
    }

    // -----------------------------------------------------------------------
    // UpdateAmpFactor tests
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::authorized_admin("admin", 500u64, None)]
    #[case::unauthorized_non_admin("not_admin", 500u64, Some(ContractError::Unauthorized {}))]
    #[case::below_min_amp(
        "admin",
        50u64,
        Some(ContractError::new("Amp factor must be at least 100"))
    )]
    fn test_update_amp_factor(
        #[case] sender: &str,
        #[case] amp_factor: u64,
        #[case] expected_error: Option<ContractError>,
    ) {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let sender_addr = deps.api.addr_make(sender);
        let info = message_info(&sender_addr, &[]);
        let msg = ExecuteMsg::UpdateAmpFactor {
            amp_factor: Uint64::from(amp_factor),
        };

        let res = execute(deps.as_mut(), env, info, msg);
        match expected_error {
            Some(err) => assert_eq!(res.unwrap_err(), err),
            None => {
                res.unwrap();
                let saved = AMP_FACTOR.load(&deps.storage).unwrap();
                assert_eq!(saved, Uint64::from(amp_factor));
            }
        }
    }

    #[test]
    fn test_update_amp_factor_response_attributes() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let admin = deps.api.addr_make("admin");
        let info = message_info(&admin, &[]);
        let msg = ExecuteMsg::UpdateAmpFactor {
            amp_factor: Uint64::from(2000u64),
        };
        let res = execute(deps.as_mut(), env, info, msg).unwrap();
        assert!(res
            .attributes
            .contains(&attr("action", "update_amp_factor")));
        assert!(res.attributes.contains(&attr("amp_factor", "2000")));
    }

    // -----------------------------------------------------------------------
    // UpdateAdmin tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_update_general_admin_happy_path() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let new_admin_addr = deps.api.addr_make("new_general_admin");
        let admin = deps.api.addr_make("admin");
        let info = message_info(&admin, &[]);
        let msg = ExecuteMsg::UpdateAdmin {
            admin: new_admin_addr.to_string(),
            admin_type: AdminType::GeneralAdmin,
        };
        execute(deps.as_mut(), env, info, msg).unwrap();

        let saved = ADMIN.load(&deps.storage).unwrap();
        assert_eq!(saved.general_admin, new_admin_addr);
    }

    #[test]
    fn test_update_fee_admin_happy_path() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let new_fee_admin = deps.api.addr_make("new_fee_admin");
        let admin = deps.api.addr_make("admin");
        let info = message_info(&admin, &[]);
        let msg = ExecuteMsg::UpdateAdmin {
            admin: new_fee_admin.to_string(),
            admin_type: AdminType::FeeAdmin,
        };
        execute(deps.as_mut(), env, info, msg).unwrap();

        let saved = ADMIN.load(&deps.storage).unwrap();
        assert_eq!(saved.fee_admin, new_fee_admin);
    }

    #[test]
    fn test_update_admin_unauthorized() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let attacker = deps.api.addr_make("attacker");
        let info = message_info(&attacker, &[]);
        let msg = ExecuteMsg::UpdateAdmin {
            admin: attacker.to_string(),
            admin_type: AdminType::GeneralAdmin,
        };
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert!(matches!(err, ContractError::UnauthorizedWithMsg { .. }));
    }

    // -----------------------------------------------------------------------
    // AddLiquidity tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_add_liquidity_happy_path_first_deposit() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        register_pool(&mut deps, chain1(), "reg-tx");

        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);
        let reserve = 10_000_000_000u128;
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
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert!(res.attributes.contains(&attr("action", "add_liquidity")));

        let b1 = BALANCES.load(&deps.storage, token1()).unwrap();
        let b2 = BALANCES.load(&deps.storage, token2()).unwrap();
        assert_eq!(b1, Uint256::from(reserve));
        assert_eq!(b2, Uint256::from(reserve));

        let chain_lp = CHAIN_LP_TOKENS.load(&deps.storage, chain1()).unwrap();
        assert!(chain_lp > Uint256::zero());

        let collateral = COLLATERAL_LP_TOKENS.load(&deps.storage).unwrap();
        assert_eq!(collateral, Uint256::from(1_000_000_000u128));
    }

    #[test]
    fn test_add_liquidity_second_deposit_increases_balances() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let reserve = 10_000_000_000u128;
        seed_liquidity(&mut deps, reserve);

        register_pool(&mut deps, chain2(), "reg-tx-2");
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
            sender: sender_on_chain2(),
            tx_id: "liq-tx-2".to_string(),
            liquidity,
            slippage_tolerance_bps: 5000,
        });
        execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        let b1 = BALANCES.load(&deps.storage, token1()).unwrap();
        let b2 = BALANCES.load(&deps.storage, token2()).unwrap();
        assert_eq!(b1, Uint256::from(reserve * 2u128));
        assert_eq!(b2, Uint256::from(reserve * 2u128));
    }

    #[test]
    fn test_add_liquidity_unauthorized_non_router() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        register_pool(&mut deps, chain1(), "reg-tx");

        let non_router = deps.api.addr_make("not_router");
        let info = message_info(&non_router, &[]);
        let liquidity = PairWithAmount::new(
            TokenWithAmount {
                token: token1(),
                amount: Uint256::from(1_000u128),
            },
            TokenWithAmount {
                token: token2(),
                amount: Uint256::from(1_000u128),
            },
        )
        .unwrap();
        let msg = ExecuteMsg::AddLiquidity(VlpAddLiquidityMsg {
            sender: sender_on_chain1(),
            tx_id: "liq-tx".to_string(),
            liquidity,
            slippage_tolerance_bps: 5000,
        });
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    #[test]
    fn test_add_liquidity_invalid_slippage_tolerance() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        register_pool(&mut deps, chain1(), "reg-tx");

        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);
        let liquidity = PairWithAmount::new(
            TokenWithAmount {
                token: token1(),
                amount: Uint256::from(1_000u128),
            },
            TokenWithAmount {
                token: token2(),
                amount: Uint256::from(1_000u128),
            },
        )
        .unwrap();
        let msg = ExecuteMsg::AddLiquidity(VlpAddLiquidityMsg {
            sender: sender_on_chain1(),
            tx_id: "liq-tx".to_string(),
            liquidity,
            slippage_tolerance_bps: 10001,
        });
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::InvalidSlippageTolerance {});
    }

    // -----------------------------------------------------------------------
    // RemoveLiquidity tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_remove_liquidity_happy_path() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let reserve = 10_000_000_000u128;
        seed_liquidity(&mut deps, reserve);

        let chain_lp_before = CHAIN_LP_TOKENS.load(&deps.storage, chain1()).unwrap();
        assert!(chain_lp_before > Uint256::zero());

        let remove_amount = chain_lp_before / Uint256::from(2u128);
        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);
        let msg = ExecuteMsg::RemoveLiquidity(VlpRemoveLiquidityMsg {
            sender: sender_on_chain1(),
            tx_id: "rem-tx".to_string(),
            lp_allocation: remove_amount,
        });
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert!(res.attributes.contains(&attr("action", "remove_liquidity")));

        let chain_lp_after = CHAIN_LP_TOKENS.load(&deps.storage, chain1()).unwrap();
        assert_eq!(chain_lp_after, chain_lp_before - remove_amount);
    }

    #[test]
    fn test_remove_liquidity_unauthorized_non_router() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_liquidity(&mut deps, 10_000_000_000);

        let chain_lp = CHAIN_LP_TOKENS.load(&deps.storage, chain1()).unwrap();
        let non_router = deps.api.addr_make("not_router");
        let info = message_info(&non_router, &[]);
        let msg = ExecuteMsg::RemoveLiquidity(VlpRemoveLiquidityMsg {
            sender: sender_on_chain1(),
            tx_id: "rem-tx".to_string(),
            lp_allocation: chain_lp / Uint256::from(2u128),
        });
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    #[test]
    fn test_remove_liquidity_overflow_lp_allocation_fails() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_liquidity(&mut deps, 10_000_000_000);

        let chain_lp = CHAIN_LP_TOKENS.load(&deps.storage, chain1()).unwrap();
        let too_much = chain_lp + Uint256::from(1u128);

        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);
        let msg = ExecuteMsg::RemoveLiquidity(VlpRemoveLiquidityMsg {
            sender: sender_on_chain1(),
            tx_id: "rem-tx".to_string(),
            lp_allocation: too_much,
        });
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert!(matches!(
            err,
            ContractError::Overflow(_) | ContractError::Std(_)
        ));
    }

    // -----------------------------------------------------------------------
    // Swap tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_swap_happy_path_final_swap() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_liquidity(&mut deps, 10_000_000_000);

        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);
        let amount_in = Uint256::from(1_000u128);
        let msg = ExecuteMsg::Swap(VlpSwapMsg {
            sender: sender_on_chain1(),
            tx_id: "swap-tx".to_string(),
            asset_in: token1(),
            amount_in,
            min_token_out: Uint256::from(1u128),
            next_swaps: vec![],
            test_fail: None,
        });

        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        assert!(res.attributes.contains(&attr("action", "swap")));
        assert!(res.attributes.contains(&attr("asset_in", "token1")));
        assert!(res.attributes.contains(&attr("asset_out", "token2")));
        assert!(res.attributes.iter().any(|a| a.key == "receive_amount"));

        let b1 = BALANCES.load(&deps.storage, token1()).unwrap();
        let b2 = BALANCES.load(&deps.storage, token2()).unwrap();
        assert!(b1 > Uint256::from(10_000_000_000u128));
        assert!(b2 < Uint256::from(10_000_000_000u128));
    }

    #[test]
    fn test_swap_zero_amount_in_fails() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_liquidity(&mut deps, 10_000_000_000);

        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);
        let msg = ExecuteMsg::Swap(VlpSwapMsg {
            sender: sender_on_chain1(),
            tx_id: "swap-tx".to_string(),
            asset_in: token1(),
            amount_in: Uint256::zero(),
            min_token_out: Uint256::from(1u128),
            next_swaps: vec![],
            test_fail: None,
        });

        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::ZeroAssetAmount {});
    }

    #[test]
    fn test_swap_slippage_exceeded_fails() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_liquidity(&mut deps, 10_000_000_000);

        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);
        let msg = ExecuteMsg::Swap(VlpSwapMsg {
            sender: sender_on_chain1(),
            tx_id: "swap-tx".to_string(),
            asset_in: token1(),
            amount_in: Uint256::from(1_000u128),
            min_token_out: Uint256::from(1_000_000u128),
            next_swaps: vec![],
            test_fail: None,
        });

        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert!(matches!(err, ContractError::SlippageExceeded { .. }));
    }

    #[test]
    fn test_swap_unknown_asset_fails() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_liquidity(&mut deps, 10_000_000_000);

        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);
        let msg = ExecuteMsg::Swap(VlpSwapMsg {
            sender: sender_on_chain1(),
            tx_id: "swap-tx".to_string(),
            asset_in: Token::create("unknowntoken".to_string()).unwrap(),
            amount_in: Uint256::from(1_000u128),
            min_token_out: Uint256::from(1u128),
            next_swaps: vec![],
            test_fail: None,
        });

        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::AssetDoesNotExist {});
    }

    #[test]
    fn test_swap_test_fail_flag_forces_failure() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_liquidity(&mut deps, 10_000_000_000);

        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);
        let msg = ExecuteMsg::Swap(VlpSwapMsg {
            sender: sender_on_chain1(),
            tx_id: "swap-tx".to_string(),
            asset_in: token1(),
            amount_in: Uint256::from(1_000u128),
            min_token_out: Uint256::from(1u128),
            next_swaps: vec![],
            test_fail: Some(true),
        });

        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::new("Force fail flag"));
    }

    #[test]
    fn test_swap_non_router_sender_accepted_via_voucher_path() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_liquidity(&mut deps, 10_000_000_000);

        let other = deps.api.addr_make("other_vlp");
        let info = message_info(&other, &[]);
        let msg = ExecuteMsg::Swap(VlpSwapMsg {
            sender: sender_on_chain1(),
            tx_id: "swap-tx".to_string(),
            asset_in: token1(),
            amount_in: Uint256::from(1_000u128),
            min_token_out: Uint256::from(1u128),
            next_swaps: vec![],
            test_fail: None,
        });

        assert!(execute(deps.as_mut(), mock_env(), info, msg).is_ok());
    }

    // -----------------------------------------------------------------------
    // State invariants after sequences of operations
    // -----------------------------------------------------------------------

    #[test]
    fn test_invariant_add_then_remove_all_liquidity() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let reserve = 10_000_000_000u128;
        seed_liquidity(&mut deps, reserve);

        let chain_lp = CHAIN_LP_TOKENS.load(&deps.storage, chain1()).unwrap();

        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);
        let msg = ExecuteMsg::RemoveLiquidity(VlpRemoveLiquidityMsg {
            sender: sender_on_chain1(),
            tx_id: "rem-tx".to_string(),
            lp_allocation: chain_lp,
        });
        execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        let chain_lp_after = CHAIN_LP_TOKENS.load(&deps.storage, chain1()).unwrap();
        assert_eq!(chain_lp_after, Uint256::zero());

        let state = STATE.load(&deps.storage).unwrap();
        let collateral = COLLATERAL_LP_TOKENS
            .may_load(&deps.storage)
            .unwrap()
            .unwrap_or_default();
        assert_eq!(state.total_lp_tokens, collateral);
    }

    #[test]
    fn test_invariant_swap_fees_accumulate_in_state() {
        let mut deps = mock_dependencies();
        let router_addr = deps.api.addr_make("router");
        let admin = EuclidAdmin::default(deps.api.addr_make("admin"));

        let msg = InstantiateMsg {
            router: Addr::unchecked("router"),
            virtual_balance_contract: Addr::unchecked("virtual_balance_contract"),
            pair: default_pair(),
            fee: Fee::new(30, 10, CrossChainUser::new(chain1(), "addr".to_string())),
            execute: None,
            admin,
            amp_factor: Some(Uint64::from(1000u64)),
        };
        let info = message_info(&router_addr, &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        seed_liquidity(&mut deps, 10_000_000_000);

        for i in 0..2u64 {
            let info = message_info(&router_addr, &[]);
            let msg = ExecuteMsg::Swap(VlpSwapMsg {
                sender: sender_on_chain1(),
                tx_id: format!("swap-tx-{i}"),
                asset_in: token1(),
                amount_in: Uint256::from(1_000u128),
                min_token_out: Uint256::from(1u128),
                next_swaps: vec![],
                test_fail: None,
            });
            execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        }

        let state = STATE.load(&deps.storage).unwrap();
        let lp_fee_total = state.total_fees_collected.lp_fees.get_fee("token1");
        assert_eq!(lp_fee_total, Uint256::from(6u128));
        let euclid_fee_total = state.total_fees_collected.euclid_fees.get_fee("token1");
        assert_eq!(euclid_fee_total, Uint256::from(2u128));
    }

    #[test]
    fn test_invariant_register_multiple_chains_then_query_all() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        register_pool(&mut deps, chain1(), "reg-tx-1");
        register_pool(&mut deps, chain2(), "reg-tx-2");
        register_pool(
            &mut deps,
            ChainUid::create("3".to_string()).unwrap(),
            "reg-tx-3",
        );

        let count = CHAIN_LP_TOKENS
            .range(&deps.storage, None, None, cosmwasm_std::Order::Ascending)
            .count();
        assert_eq!(count, 3);
    }

    // -----------------------------------------------------------------------
    // Stable LP allocation tests (D-invariant math)
    //
    // These tests verify the stable_vlp `add_liquidity` handler threads
    // `amp_factor` correctly into the underlying pool function and that the
    // resulting LP mint is computed using the D-invariant (Curve-style)
    // formula, not the constant-product geometric mean.
    // -----------------------------------------------------------------------

    /// Instantiate the stable VLP with a custom amp factor.
    fn init_with_amp(deps: &mut crate::testing::helpers::MockDeps, amp: u64) {
        let router = deps.api.addr_make("router");
        let admin = EuclidAdmin::default(deps.api.addr_make("admin"));
        let msg = InstantiateMsg {
            router: Addr::unchecked("router"),
            virtual_balance_contract: Addr::unchecked("virtual_balance_contract"),
            pair: default_pair(),
            fee: default_fee(),
            execute: None,
            admin,
            amp_factor: Some(Uint64::from(amp)),
        };
        let info = message_info(&router, &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
    }

    /// Seed a pool with imbalanced reserves on chain1.
    fn seed_imbalanced_liquidity(
        deps: &mut crate::testing::helpers::MockDeps,
        amount_1: u128,
        amount_2: u128,
    ) {
        register_pool(deps, chain1(), "reg-tx-imb");
        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);
        let liquidity = PairWithAmount::new(
            TokenWithAmount {
                token: token1(),
                amount: Uint256::from(amount_1),
            },
            TokenWithAmount {
                token: token2(),
                amount: Uint256::from(amount_2),
            },
        )
        .unwrap();
        let msg = ExecuteMsg::AddLiquidity(VlpAddLiquidityMsg {
            sender: sender_on_chain1(),
            tx_id: "liq-imb-tx".to_string(),
            liquidity,
            slippage_tolerance_bps: 5000,
        });
        execute(deps.as_mut(), mock_env(), info, msg).unwrap();
    }

    // The integration test fixture: seed a stable VLP at amp=100 with
    // 10k/100k reserves and verify total_lp_tokens == 82026 (the D-invariant).
    // This is the unit-level mirror of
    // tests-integration/src/tests/factory.rs:2474.
    #[test]
    fn test_stable_vlp_first_deposit_10k_100k_amp_100_yields_d_82026() {
        let mut deps = mock_dependencies();
        init_with_amp(&mut deps, 100);
        seed_imbalanced_liquidity(&mut deps, 10_000_000_000, 100_000_000_000);

        let state = STATE.load(&deps.storage).unwrap();
        let total_lp = state.total_lp_tokens;
        let expected_d = Uint256::from(82_026_000_000u128);
        let tolerance = Uint256::from(200_000u128);
        assert!(
            total_lp.abs_diff(expected_d) <= tolerance,
            "D should be ~82026*10^6, got {total_lp}"
        );

        let chain_lp = CHAIN_LP_TOKENS.load(&deps.storage, chain1()).unwrap();
        assert_eq!(
            chain_lp,
            total_lp - Uint256::from(euclid_pool::MINIMUM_LIQUIDITY)
        );

        let collateral = COLLATERAL_LP_TOKENS.load(&deps.storage).unwrap();
        assert_eq!(collateral, Uint256::from(euclid_pool::MINIMUM_LIQUIDITY));
    }

    // Same fixture but at amp=1000 (the default). D should be larger
    // (closer to arithmetic mean 110_000) than at amp=100.
    #[test]
    fn test_stable_vlp_amp_factor_threaded_through_changes_lp_mint() {
        let mut deps_low = mock_dependencies();
        init_with_amp(&mut deps_low, 100);
        seed_imbalanced_liquidity(&mut deps_low, 10_000_000_000, 100_000_000_000);
        let total_lp_low = STATE.load(&deps_low.storage).unwrap().total_lp_tokens;

        let mut deps_high = mock_dependencies();
        init_with_amp(&mut deps_high, 1000);
        seed_imbalanced_liquidity(&mut deps_high, 10_000_000_000, 100_000_000_000);
        let total_lp_high = STATE.load(&deps_high.storage).unwrap().total_lp_tokens;

        assert_ne!(
            total_lp_low, total_lp_high,
            "amp_factor must be threaded through and affect LP mint"
        );
        assert!(
            total_lp_high > total_lp_low,
            "Higher amp must produce higher D for imbalanced pools (low={total_lp_low}, high={total_lp_high})"
        );
        assert!(total_lp_low >= Uint256::from(31_622_000_000u128));
        assert!(total_lp_high <= Uint256::from(110_000_000_000u128));
    }

    // Sequential deposits: D-invariant grows additively with reserves.
    // Doubling reserves should ~double the D-invariant (and hence ~double LP).
    #[test]
    fn test_stable_vlp_sequential_deposits_track_d_growth() {
        let mut deps = mock_dependencies();
        init_with_amp(&mut deps, 100);
        seed_imbalanced_liquidity(&mut deps, 10_000_000_000, 100_000_000_000);

        let total_lp_before = STATE.load(&deps.storage).unwrap().total_lp_tokens;
        let expected_d = Uint256::from(82_026_000_000u128);
        let tolerance = Uint256::from(200_000u128);
        assert!(
            total_lp_before.abs_diff(expected_d) <= tolerance,
            "D should be ~82026*10^6, got {total_lp_before}"
        );

        register_pool(&mut deps, chain2(), "reg-tx-2nd");
        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);
        let liquidity = PairWithAmount::new(
            TokenWithAmount {
                token: token1(),
                amount: Uint256::from(10_000_000_000u128),
            },
            TokenWithAmount {
                token: token2(),
                amount: Uint256::from(100_000_000_000u128),
            },
        )
        .unwrap();
        let msg = ExecuteMsg::AddLiquidity(VlpAddLiquidityMsg {
            sender: sender_on_chain2(),
            tx_id: "liq-tx-2nd".to_string(),
            liquidity,
            slippage_tolerance_bps: 5000,
        });
        execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        let total_lp_after = STATE.load(&deps.storage).unwrap().total_lp_tokens;

        let expected_doubled = Uint256::from(164_052_000_000u128);
        let delta = if total_lp_after > expected_doubled {
            total_lp_after - expected_doubled
        } else {
            expected_doubled - total_lp_after
        };
        assert!(
            delta <= Uint256::from(2_000_000u128),
            "Doubling reserves must ~double D; expected ~164052*10^6, got {total_lp_after}"
        );

        let chain2_lp = CHAIN_LP_TOKENS.load(&deps.storage, chain2()).unwrap();
        let expected_chain2_lp = total_lp_after - total_lp_before;
        assert_eq!(chain2_lp, expected_chain2_lp);

        let b1 = BALANCES.load(&deps.storage, token1()).unwrap();
        let b2 = BALANCES.load(&deps.storage, token2()).unwrap();
        assert_eq!(b1, Uint256::from(20_000_000_000u128));
        assert_eq!(b2, Uint256::from(200_000_000_000u128));
    }

    // Three sequential balanced deposits: each deposit grows total_lp_tokens
    // monotonically and the chain's LP allocation matches the new mint.
    #[test]
    fn test_stable_vlp_three_balanced_deposits_grow_lp_supply() {
        let mut deps = mock_dependencies();
        init_with_amp(&mut deps, 100);
        let reserve = 1_000_000_000_000u128;
        seed_liquidity(&mut deps, reserve);

        let mut prev_total = STATE.load(&deps.storage).unwrap().total_lp_tokens;
        let chain1_uid = chain1();

        for i in 1..=3 {
            let router = deps.api.addr_make("router");
            let info = message_info(&router, &[]);
            let liquidity = PairWithAmount::new(
                TokenWithAmount {
                    token: token1(),
                    amount: Uint256::from(reserve as u128),
                },
                TokenWithAmount {
                    token: token2(),
                    amount: Uint256::from(reserve as u128),
                },
            )
            .unwrap();
            let msg = ExecuteMsg::AddLiquidity(VlpAddLiquidityMsg {
                sender: sender_on_chain1(),
                tx_id: format!("liq-tx-{i}"),
                liquidity,
                slippage_tolerance_bps: 5000,
            });
            execute(deps.as_mut(), mock_env(), info, msg).unwrap();

            let new_total = STATE.load(&deps.storage).unwrap().total_lp_tokens;
            assert!(
                new_total > prev_total,
                "iteration {i}: LP supply must grow (prev={prev_total}, new={new_total})"
            );

            // chain1 LP after deposit `i` should be (i+1)*reserve units' worth
            // of LP (1 from seed + i additions, all equal because reserves
            // double each round in proportion to D doubling).
            let chain_lp = CHAIN_LP_TOKENS
                .load(&deps.storage, chain1_uid.clone())
                .unwrap();
            // chain_lp can never exceed total_lp_tokens.
            assert!(chain_lp < new_total);

            prev_total = new_total;
        }

        let b1 = BALANCES.load(&deps.storage, token1()).unwrap();
        let b2 = BALANCES.load(&deps.storage, token2()).unwrap();
        assert_eq!(b1, Uint256::from(reserve * 4u128));
        assert_eq!(b2, Uint256::from(reserve * 4u128));
    }

    // The default amp_factor (1000) on a balanced 1M/1M pool should produce
    // total_lp_tokens approximately equal to the sum of reserves.
    // (For balanced pools at high amp, D ≈ sum.)
    #[test]
    fn test_stable_vlp_default_init_balanced_d_close_to_sum() {
        let mut deps = mock_dependencies();
        init(&mut deps); // default amp = 1000
        let reserve = 1_000_000_000_000u128;
        seed_liquidity(&mut deps, reserve);

        let total = STATE.load(&deps.storage).unwrap().total_lp_tokens;
        assert_eq!(total, Uint256::from(2_000_000_000_000u128));
    }

    // Confirm the `add_liquidity` handler returns the expected `lp_allocation`
    // attribute (which is the user-facing mint, post-MINIMUM_LIQUIDITY haircut
    // on the first deposit only).
    #[test]
    fn test_stable_vlp_add_liquidity_response_lp_allocation_attribute() {
        let mut deps = mock_dependencies();
        init_with_amp(&mut deps, 100);
        register_pool(&mut deps, chain1(), "reg-tx");

        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);
        let liquidity = PairWithAmount::new(
            TokenWithAmount {
                token: token1(),
                amount: Uint256::from(10_000_000_000u128),
            },
            TokenWithAmount {
                token: token2(),
                amount: Uint256::from(100_000_000_000u128),
            },
        )
        .unwrap();
        let msg = ExecuteMsg::AddLiquidity(VlpAddLiquidityMsg {
            sender: sender_on_chain1(),
            tx_id: "liq-tx".to_string(),
            liquidity,
            slippage_tolerance_bps: 5000,
        });
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        let lp_alloc = res
            .attributes
            .iter()
            .find(|a| a.key == "lp_allocation")
            .unwrap();
        let lp_val: Uint256 = lp_alloc.value.parse::<u128>().unwrap().into();
        let state = STATE.load(&deps.storage).unwrap();
        let expected = state.total_lp_tokens - Uint256::from(euclid_pool::MINIMUM_LIQUIDITY);
        assert_eq!(lp_val, expected);
        assert!(res
            .attributes
            .contains(&attr("liquidity_1_added", "10000000000")));
        assert!(res
            .attributes
            .contains(&attr("liquidity_2_added", "100000000000")));
    }
}
