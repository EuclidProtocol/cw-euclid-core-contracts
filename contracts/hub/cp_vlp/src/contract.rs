use std::collections::HashMap;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, Uint256};
use cw2::set_contract_version;
use euclid::{
    error::ContractError,
    fee::{DenomFees, TotalFees},
    msgs::vlp::{
        base::{State, NEXT_SWAP_REPLY_ID},
        cp::msg::{ExecuteMsg, InstantiateMsg, QueryMsg},
    },
};
use euclid_pool::{
    add_liquidity, execute_swap, register_pool, remove_liquidity, update_admin, update_fee,
    SwapCalculationMethod,
};

use crate::{
    query::{
        query_admin, query_all_pools, query_fee, query_liquidity, query_pool, query_simulate_swap,
        query_state, query_total_fees_collected, query_total_fees_per_denom,
    },
    reply,
    state::{ADMIN, BALANCES, CHAIN_LP_TOKENS, COLLATERAL_LP_TOKENS, STATE},
};
// version info for migration info
pub(crate) const CONTRACT_NAME: &str = "crates.io:vlp";
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

    let response =
        msg.execute
            .map_or(Ok(Response::default()), |execute_msg| match execute_msg {
                ExecuteMsg::RegisterPool(register_pool_msg) => register_pool(
                    deps,
                    env.clone(),
                    info.clone(),
                    &STATE,
                    &CHAIN_LP_TOKENS,
                    None,
                    register_pool_msg.sender,
                    register_pool_msg.pair,
                    register_pool_msg.tx_id,
                ),
                _ => Err(ContractError::Unauthorized {}),
            })?;

    Ok(response
        .add_attribute("method", "instantiate")
        .add_attribute("vlp_address", env.contract.address.to_string())
        .add_attribute("owner", info.sender)
        .add_attribute("pool_type", "xyk")
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
        ExecuteMsg::RegisterPool(register_pool_msg) => register_pool(
            deps,
            env,
            info,
            &STATE,
            &CHAIN_LP_TOKENS,
            None,
            register_pool_msg.sender,
            register_pool_msg.pair,
            register_pool_msg.tx_id,
        ),
        ExecuteMsg::AddLiquidity(add_liquidity_msg) => add_liquidity(
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
            None,
            add_liquidity_msg.tx_id,
        ),
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
        ExecuteMsg::Swap(swap_msg) => execute_swap(
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
            SwapCalculationMethod::Regular,
            swap_msg.test_fail,
        ),
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
        ExecuteMsg::UpdateAdmin { admin, admin_type } => {
            update_admin(deps, env, info, &ADMIN, admin, admin_type)
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::State {} => query_state(deps),
        QueryMsg::GetAdmin {} => query_admin(deps),
        QueryMsg::SimulateSwap(msg) => {
            query_simulate_swap(deps, msg.asset, msg.asset_amount, msg.swaps)
        }
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
    use crate::contract::{execute, instantiate};
    use crate::state::{ADMIN, BALANCES, CHAIN_LP_TOKENS, COLLATERAL_LP_TOKENS, STATE};
    use crate::testing::helpers::{
        cross_chain_user, default_admin, default_fee, default_pair, init, make_pair_with_amount,
        token1, token2, TEST_VIRTUAL_BALANCE,
    };
    use cosmwasm_std::testing::{message_info, mock_dependencies, mock_env};
    use cosmwasm_std::{coins, Addr, Uint256};
    use euclid::admin::AdminType;
    use euclid::chain::ChainUid;
    use euclid::cross_chain_user::CrossChainUser;
    use euclid::error::ContractError;
    use euclid::fee::{DenomFees, TotalFees};
    use euclid::msgs::vlp::base::{
        State, VlpAddLiquidityMsg, VlpRegisterPoolMsg, VlpRemoveLiquidityMsg, VlpSwapMsg,
    };
    use euclid::msgs::vlp::cp::msg::{ExecuteMsg, InstantiateMsg};
    use euclid::token::{Pair, Token};
    use rstest::rstest;
    use std::collections::HashMap;

    // -----------------------------------------------------------------------
    // Instantiate
    // -----------------------------------------------------------------------

    #[test]
    fn test_init() {
        let mut deps = mock_dependencies();
        let router = deps.api.addr_make("router");
        let res = init(&mut deps);
        assert_eq!(0, res.messages.len());
        let admin = default_admin(&deps);
        let expected_state = State {
            pair: default_pair(),
            router: router.clone(),
            virtual_balance_contract: Addr::unchecked(TEST_VIRTUAL_BALANCE),
            fee: default_fee(&deps),
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
        let state = STATE.load(&deps.storage).unwrap();
        assert_eq!(state, expected_state);
        let saved_admin = ADMIN.load(&deps.storage).unwrap();
        assert_eq!(saved_admin, admin);

        let balance_1 = BALANCES.load(&deps.storage, token1()).unwrap();
        assert_eq!(balance_1, Uint256::zero());

        let balance_2 = BALANCES.load(&deps.storage, token2()).unwrap();
        assert_eq!(balance_2, Uint256::zero());
    }

    #[test]
    fn test_instantiate_response_attributes() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let admin = default_admin(&deps);
        let msg = InstantiateMsg {
            router: Addr::unchecked("router"),
            virtual_balance_contract: Addr::unchecked(TEST_VIRTUAL_BALANCE),
            pair: default_pair(),
            fee: default_fee(&deps),
            execute: None,
            admin,
        };
        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);
        let res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let method_attr = res.attributes.iter().find(|a| a.key == "method").unwrap();
        assert_eq!(method_attr.value, "instantiate");

        let pool_type_attr = res
            .attributes
            .iter()
            .find(|a| a.key == "pool_type")
            .unwrap();
        assert_eq!(pool_type_attr.value, "xyk");

        let owner_attr = res.attributes.iter().find(|a| a.key == "owner").unwrap();
        assert_eq!(owner_attr.value, router.to_string());

        let token_1_attr = res.attributes.iter().find(|a| a.key == "token_1").unwrap();
        assert_eq!(token_1_attr.value, "token1");

        let token_2_attr = res.attributes.iter().find(|a| a.key == "token_2").unwrap();
        assert_eq!(token_2_attr.value, "token2");
    }

    #[test]
    fn test_instantiate_with_invalid_pair_fails() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let admin = default_admin(&deps);
        let invalid_pair = Pair {
            token_1: Token::create("sametoken".to_string()).unwrap(),
            token_2: Token::create("sametoken".to_string()).unwrap(),
        };
        let msg = InstantiateMsg {
            router: Addr::unchecked("router"),
            virtual_balance_contract: Addr::unchecked(TEST_VIRTUAL_BALANCE),
            pair: invalid_pair,
            fee: default_fee(&deps),
            execute: None,
            admin,
        };
        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);
        let err = instantiate(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::DuplicateTokens {});
    }

    #[test]
    fn test_instantiate_with_execute_register_pool() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let admin = default_admin(&deps);
        let sender = cross_chain_user("chain1", "user1");
        let register_msg = VlpRegisterPoolMsg {
            sender,
            pair: default_pair(),
            tx_id: "tx1".to_string(),
        };
        let msg = InstantiateMsg {
            router: Addr::unchecked("router"),
            virtual_balance_contract: Addr::unchecked(TEST_VIRTUAL_BALANCE),
            pair: default_pair(),
            fee: default_fee(&deps),
            execute: Some(ExecuteMsg::RegisterPool(register_msg)),
            admin,
        };
        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);
        let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(res.messages.len(), 0);

        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let lp = CHAIN_LP_TOKENS.load(&deps.storage, chain_uid).unwrap();
        assert_eq!(lp, Uint256::zero());
    }

    #[test]
    fn test_instantiate_with_non_register_execute_fails() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let admin = default_admin(&deps);
        let update_fee_msg = ExecuteMsg::UpdateFee {
            lp_fee_bps: Some(5),
            euclid_fee_bps: Some(5),
            recipient: None,
        };
        let msg = InstantiateMsg {
            router: Addr::unchecked("router"),
            virtual_balance_contract: Addr::unchecked(TEST_VIRTUAL_BALANCE),
            pair: default_pair(),
            fee: default_fee(&deps),
            execute: Some(update_fee_msg),
            admin,
        };
        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);
        let err = instantiate(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    // -----------------------------------------------------------------------
    // Execute: RegisterPool
    // -----------------------------------------------------------------------

    #[test]
    fn test_execute_register_pool() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let sender = cross_chain_user("1", "sender_address");
        let msg = ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
            sender,
            pair: default_pair(),
            tx_id: "1".to_string(),
        });
        let router = deps.api.addr_make("router");
        let info = message_info(&router, &coins(1000, "earth"));
        let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
        assert_eq!(res.messages.len(), 0);

        let lp = CHAIN_LP_TOKENS
            .load(&deps.storage, ChainUid::create("1".to_string()).unwrap())
            .unwrap();
        assert_eq!(lp, Uint256::zero());
    }

    #[test]
    fn test_execute_register_pool_unauthorized() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let sender = cross_chain_user("1", "sender_address");
        let msg = ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
            sender,
            pair: default_pair(),
            tx_id: "tx1".to_string(),
        });
        let not_router = deps.api.addr_make("not_router");
        let info = message_info(&not_router, &[]);
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    #[test]
    fn test_execute_register_pool_already_exists() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let router = deps.api.addr_make("router");
        let sender = cross_chain_user("chain1", "user1");
        let msg = ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
            sender: sender.clone(),
            pair: default_pair(),
            tx_id: "tx1".to_string(),
        });
        let info = message_info(&router, &[]);
        execute(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap();

        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::PoolAlreadyExists {});
    }

    #[test]
    fn test_execute_register_pool_wrong_pair() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let router = deps.api.addr_make("router");
        let sender = cross_chain_user("chain1", "user1");
        let wrong_pair = Pair {
            token_1: Token::create("aaaa".to_string()).unwrap(),
            token_2: Token::create("bbbb".to_string()).unwrap(),
        };
        let msg = ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
            sender,
            pair: wrong_pair,
            tx_id: "tx1".to_string(),
        });
        let info = message_info(&router, &[]);
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::AssetDoesNotExist {});
    }

    // -----------------------------------------------------------------------
    // Execute: AddLiquidity
    // -----------------------------------------------------------------------

    #[test]
    fn test_execute_add_liquidity_happy_path() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let router = deps.api.addr_make("router");
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let sender = CrossChainUser::new(chain_uid.clone(), "user1".to_string());
        let reg_msg = ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
            sender: sender.clone(),
            pair: default_pair(),
            tx_id: "reg_tx".to_string(),
        });
        let info = message_info(&router, &[]);
        execute(deps.as_mut(), env.clone(), info.clone(), reg_msg).unwrap();

        let liquidity = make_pair_with_amount(10_000_000_000, 10_000_000_000);
        let add_msg = ExecuteMsg::AddLiquidity(VlpAddLiquidityMsg {
            sender: sender.clone(),
            tx_id: "add_tx1".to_string(),
            liquidity,
            slippage_tolerance_bps: 0,
        });
        let res = execute(deps.as_mut(), env.clone(), info.clone(), add_msg).unwrap();
        assert!(res.messages.len() >= 1);

        let b1 = BALANCES.load(&deps.storage, token1()).unwrap();
        let b2 = BALANCES.load(&deps.storage, token2()).unwrap();
        assert_eq!(b1, Uint256::from(10_000_000_000u128));
        assert_eq!(b2, Uint256::from(10_000_000_000u128));

        let chain_lp = CHAIN_LP_TOKENS
            .load(&deps.storage, chain_uid.clone())
            .unwrap();
        assert!(!chain_lp.is_zero());

        let state = STATE.load(&deps.storage).unwrap();
        assert!(!state.total_lp_tokens.is_zero());
    }

    #[test]
    fn test_execute_add_liquidity_unauthorized() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let sender = cross_chain_user("chain1", "user1");
        let liquidity = make_pair_with_amount(1_000, 1_000);
        let add_msg = ExecuteMsg::AddLiquidity(VlpAddLiquidityMsg {
            sender,
            tx_id: "tx1".to_string(),
            liquidity,
            slippage_tolerance_bps: 0,
        });
        let not_router = deps.api.addr_make("not_router");
        let info = message_info(&not_router, &[]);
        let err = execute(deps.as_mut(), env, info, add_msg).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    #[test]
    fn test_execute_add_liquidity_slippage_exceeded() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let router = deps.api.addr_make("router");
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let sender = CrossChainUser::new(chain_uid.clone(), "user1".to_string());

        let reg_msg = ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
            sender: sender.clone(),
            pair: default_pair(),
            tx_id: "reg".to_string(),
        });
        let info = message_info(&router, &[]);
        execute(deps.as_mut(), env.clone(), info.clone(), reg_msg).unwrap();

        let liq1 = make_pair_with_amount(10_000_000_000, 10_000_000_000);
        let add1 = ExecuteMsg::AddLiquidity(VlpAddLiquidityMsg {
            sender: sender.clone(),
            tx_id: "add1".to_string(),
            liquidity: liq1,
            slippage_tolerance_bps: 0,
        });
        execute(deps.as_mut(), env.clone(), info.clone(), add1).unwrap();

        let liq2 = make_pair_with_amount(1_000_000_000, 2_000_000_000);
        let add2 = ExecuteMsg::AddLiquidity(VlpAddLiquidityMsg {
            sender: sender.clone(),
            tx_id: "add2".to_string(),
            liquidity: liq2,
            slippage_tolerance_bps: 0,
        });
        let err = execute(deps.as_mut(), env, info, add2).unwrap_err();
        assert!(matches!(
            err,
            ContractError::LiquiditySlippageExceeded { .. }
        ));
    }

    #[test]
    fn test_execute_add_liquidity_slippage_bps_too_high() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let router = deps.api.addr_make("router");
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let sender = CrossChainUser::new(chain_uid, "user1".to_string());

        let reg_msg = ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
            sender: sender.clone(),
            pair: default_pair(),
            tx_id: "reg".to_string(),
        });
        let info = message_info(&router, &[]);
        execute(deps.as_mut(), env.clone(), info.clone(), reg_msg).unwrap();

        let liq = make_pair_with_amount(10_000_000_000, 10_000_000_000);
        let add_msg = ExecuteMsg::AddLiquidity(VlpAddLiquidityMsg {
            sender: sender.clone(),
            tx_id: "add".to_string(),
            liquidity: liq,
            slippage_tolerance_bps: 5001,
        });
        let err = execute(deps.as_mut(), env, info, add_msg).unwrap_err();
        assert_eq!(err, ContractError::InvalidSlippageTolerance {});
    }

    // -----------------------------------------------------------------------
    // Execute: RemoveLiquidity
    // -----------------------------------------------------------------------

    #[test]
    fn test_execute_remove_liquidity_happy_path() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let router = deps.api.addr_make("router");
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let sender = CrossChainUser::new(chain_uid.clone(), "user1".to_string());

        let info = message_info(&router, &[]);
        execute(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
                sender: sender.clone(),
                pair: default_pair(),
                tx_id: "reg".to_string(),
            }),
        )
        .unwrap();

        let liq = make_pair_with_amount(20_000_000_000, 20_000_000_000);
        execute(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::AddLiquidity(VlpAddLiquidityMsg {
                sender: sender.clone(),
                tx_id: "add".to_string(),
                liquidity: liq,
                slippage_tolerance_bps: 0,
            }),
        )
        .unwrap();

        let chain_lp_before = CHAIN_LP_TOKENS
            .load(&deps.storage, chain_uid.clone())
            .unwrap();
        let total_lp_before = STATE.load(&deps.storage).unwrap().total_lp_tokens;

        let remove_amount = chain_lp_before / Uint256::from(2u128);
        let remove_msg = ExecuteMsg::RemoveLiquidity(VlpRemoveLiquidityMsg {
            sender: sender.clone(),
            lp_allocation: remove_amount,
            tx_id: "remove".to_string(),
        });
        let res = execute(deps.as_mut(), env, info, remove_msg).unwrap();
        assert_eq!(res.messages.len(), 2);

        let chain_lp_after = CHAIN_LP_TOKENS
            .load(&deps.storage, chain_uid.clone())
            .unwrap();
        let total_lp_after = STATE.load(&deps.storage).unwrap().total_lp_tokens;

        assert_eq!(chain_lp_after, chain_lp_before - remove_amount);
        assert_eq!(total_lp_after, total_lp_before - remove_amount);
    }

    #[test]
    fn test_execute_remove_liquidity_unauthorized() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let sender = cross_chain_user("chain1", "user1");
        let remove_msg = ExecuteMsg::RemoveLiquidity(VlpRemoveLiquidityMsg {
            sender,
            lp_allocation: Uint256::from(100u128),
            tx_id: "tx1".to_string(),
        });
        let not_router = deps.api.addr_make("not_router");
        let info = message_info(&not_router, &[]);
        let err = execute(deps.as_mut(), env, info, remove_msg).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    // -----------------------------------------------------------------------
    // Execute: Swap
    // -----------------------------------------------------------------------

    #[test]
    fn test_execute_swap_happy_path() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let router = deps.api.addr_make("router");
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let sender = CrossChainUser::new(chain_uid.clone(), "user1".to_string());

        let info = message_info(&router, &[]);
        execute(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
                sender: sender.clone(),
                pair: default_pair(),
                tx_id: "reg".to_string(),
            }),
        )
        .unwrap();
        execute(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::AddLiquidity(VlpAddLiquidityMsg {
                sender: sender.clone(),
                tx_id: "add".to_string(),
                liquidity: make_pair_with_amount(10_000_000_000, 10_000_000_000),
                slippage_tolerance_bps: 0,
            }),
        )
        .unwrap();

        let swap_amount = Uint256::from(10_000u128);
        let swap_msg = ExecuteMsg::Swap(VlpSwapMsg {
            sender: sender.clone(),
            tx_id: "swap1".to_string(),
            asset_in: token1(),
            amount_in: swap_amount,
            min_token_out: Uint256::zero(),
            next_swaps: vec![],
            test_fail: None,
        });
        let res = execute(deps.as_mut(), env, info, swap_msg).unwrap();
        assert!(res.messages.len() >= 2);

        let b1 = BALANCES.load(&deps.storage, token1()).unwrap();
        assert!(b1 > Uint256::from(10_000_000_000u128));

        let b2 = BALANCES.load(&deps.storage, token2()).unwrap();
        assert!(b2 < Uint256::from(10_000_000_000u128));
    }

    #[test]
    fn test_execute_swap_zero_amount() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let router = deps.api.addr_make("router");
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let sender = CrossChainUser::new(chain_uid, "user1".to_string());
        let info = message_info(&router, &[]);

        execute(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
                sender: sender.clone(),
                pair: default_pair(),
                tx_id: "reg".to_string(),
            }),
        )
        .unwrap();

        execute(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::AddLiquidity(VlpAddLiquidityMsg {
                sender: sender.clone(),
                tx_id: "add".to_string(),
                liquidity: make_pair_with_amount(10_000_000_000, 10_000_000_000),
                slippage_tolerance_bps: 0,
            }),
        )
        .unwrap();

        let swap_msg = ExecuteMsg::Swap(VlpSwapMsg {
            sender: sender.clone(),
            tx_id: "swap_zero".to_string(),
            asset_in: token1(),
            amount_in: Uint256::zero(),
            min_token_out: Uint256::zero(),
            next_swaps: vec![],
            test_fail: None,
        });
        let err = execute(deps.as_mut(), env, info, swap_msg).unwrap_err();
        assert_eq!(err, ContractError::ZeroAssetAmount {});
    }

    #[test]
    fn test_execute_swap_slippage_exceeded() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let router = deps.api.addr_make("router");
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let sender = CrossChainUser::new(chain_uid, "user1".to_string());
        let info = message_info(&router, &[]);

        execute(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
                sender: sender.clone(),
                pair: default_pair(),
                tx_id: "reg".to_string(),
            }),
        )
        .unwrap();
        execute(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::AddLiquidity(VlpAddLiquidityMsg {
                sender: sender.clone(),
                tx_id: "add".to_string(),
                liquidity: make_pair_with_amount(10_000_000_000, 10_000_000_000),
                slippage_tolerance_bps: 0,
            }),
        )
        .unwrap();

        let swap_msg = ExecuteMsg::Swap(VlpSwapMsg {
            sender: sender.clone(),
            tx_id: "swap_slip".to_string(),
            asset_in: token1(),
            amount_in: Uint256::from(10_000u128),
            min_token_out: Uint256::from(999_999_999u128),
            next_swaps: vec![],
            test_fail: None,
        });
        let err = execute(deps.as_mut(), env, info, swap_msg).unwrap_err();
        assert!(matches!(err, ContractError::SlippageExceeded { .. }));
    }

    #[test]
    fn test_execute_swap_asset_does_not_exist() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let router = deps.api.addr_make("router");
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let sender = CrossChainUser::new(chain_uid, "user1".to_string());
        let info = message_info(&router, &[]);

        execute(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
                sender: sender.clone(),
                pair: default_pair(),
                tx_id: "reg".to_string(),
            }),
        )
        .unwrap();

        let swap_msg = ExecuteMsg::Swap(VlpSwapMsg {
            sender: sender.clone(),
            tx_id: "swap_bad_asset".to_string(),
            asset_in: Token::create("unknown".to_string()).unwrap(),
            amount_in: Uint256::from(1_000u128),
            min_token_out: Uint256::zero(),
            next_swaps: vec![],
            test_fail: None,
        });
        let err = execute(deps.as_mut(), env, info, swap_msg).unwrap_err();
        assert_eq!(err, ContractError::AssetDoesNotExist {});
    }

    #[test]
    fn test_execute_swap_test_fail_flag() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let router = deps.api.addr_make("router");
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let sender = CrossChainUser::new(chain_uid, "user1".to_string());
        let info = message_info(&router, &[]);

        execute(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
                sender: sender.clone(),
                pair: default_pair(),
                tx_id: "reg".to_string(),
            }),
        )
        .unwrap();

        execute(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::AddLiquidity(VlpAddLiquidityMsg {
                sender: sender.clone(),
                tx_id: "add".to_string(),
                liquidity: make_pair_with_amount(10_000_000_000, 10_000_000_000),
                slippage_tolerance_bps: 0,
            }),
        )
        .unwrap();

        let swap_msg = ExecuteMsg::Swap(VlpSwapMsg {
            sender: sender.clone(),
            tx_id: "swap_fail".to_string(),
            asset_in: token1(),
            amount_in: Uint256::from(10_000u128),
            min_token_out: Uint256::zero(),
            next_swaps: vec![],
            test_fail: Some(true),
        });
        let err = execute(deps.as_mut(), env, info, swap_msg).unwrap_err();
        assert_eq!(err, ContractError::new("Force fail flag"));
    }

    // -----------------------------------------------------------------------
    // Execute: UpdateFee
    // -----------------------------------------------------------------------

    #[test]
    fn test_update_fee() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let msg = ExecuteMsg::UpdateFee {
            lp_fee_bps: Some(5),
            euclid_fee_bps: Some(4),
            recipient: Some(CrossChainUser::new(
                ChainUid::create("2".to_string()).unwrap(),
                "addr_2".to_string(),
            )),
        };
        let not_admin = deps.api.addr_make("not_admin");
        let info = message_info(&not_admin, &[]);

        let err = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});

        let admin = deps.api.addr_make("admin");
        let info = message_info(&admin, &[]);
        execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let fee = STATE.load(&deps.storage).unwrap().fee;
        assert_eq!(
            fee,
            euclid::fee::Fee::new(
                5,
                4,
                CrossChainUser::new(
                    ChainUid::create("2".to_string()).unwrap(),
                    "addr_2".to_string(),
                )
            )
        );

        let msg = ExecuteMsg::UpdateFee {
            lp_fee_bps: Some(5000),
            euclid_fee_bps: Some(4),
            recipient: Some(CrossChainUser::new(
                ChainUid::create("2".to_string()).unwrap(),
                "addr_2".to_string(),
            )),
        };
        let err = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap_err();
        assert_eq!(
            err,
            ContractError::new("LP Fee cannot exceed maximum limit")
        );

        let msg = ExecuteMsg::UpdateFee {
            lp_fee_bps: Some(50),
            euclid_fee_bps: Some(4000),
            recipient: Some(CrossChainUser::new(
                ChainUid::create("2".to_string()).unwrap(),
                "addr_2".to_string(),
            )),
        };
        let err = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap_err();
        assert_eq!(
            err,
            ContractError::new("Euclid Fee cannot exceed maximum limit")
        );
    }

    #[rstest]
    #[case::zero_lp_fee(Some(0), Some(0), false)]
    #[case::max_lp_fee(Some(1000), Some(0), false)]
    #[case::max_euclid_fee(Some(0), Some(1000), false)]
    #[case::over_max_lp_fee(Some(1001), Some(0), true)]
    #[case::over_max_euclid_fee(Some(0), Some(1001), true)]
    fn test_update_fee_boundary_cases(
        #[case] lp_fee: Option<u64>,
        #[case] euclid_fee: Option<u64>,
        #[case] should_fail: bool,
    ) {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let msg = ExecuteMsg::UpdateFee {
            lp_fee_bps: lp_fee,
            euclid_fee_bps: euclid_fee,
            recipient: None,
        };
        let admin = deps.api.addr_make("admin");
        let info = message_info(&admin, &[]);
        let res = execute(deps.as_mut(), env, info, msg);
        if should_fail {
            assert!(res.is_err());
        } else {
            assert!(res.is_ok());
        }
    }

    // -----------------------------------------------------------------------
    // Execute: UpdateAdmin
    // -----------------------------------------------------------------------

    #[test]
    fn test_update_admin_general() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let new_general_admin = deps.api.addr_make("new_general_admin");
        let msg = ExecuteMsg::UpdateAdmin {
            admin: new_general_admin.to_string(),
            admin_type: AdminType::GeneralAdmin,
        };
        let admin = deps.api.addr_make("admin");
        let info = message_info(&admin, &[]);
        execute(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap();

        let saved_admin = ADMIN.load(&deps.storage).unwrap();
        assert_eq!(saved_admin.general_admin, new_general_admin);
    }

    #[test]
    fn test_update_admin_unauthorized() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let new_admin = deps.api.addr_make("new_admin");
        let msg = ExecuteMsg::UpdateAdmin {
            admin: new_admin.to_string(),
            admin_type: AdminType::FeeAdmin,
        };
        let not_fee_admin = deps.api.addr_make("not_fee_admin");
        let info = message_info(&not_fee_admin, &[]);
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert!(matches!(err, ContractError::UnauthorizedWithMsg { .. }));
    }

    // -----------------------------------------------------------------------
    // State invariants
    // -----------------------------------------------------------------------

    #[test]
    fn test_state_invariant_lp_tokens_match_chain_lp_sum() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);

        for (chain_name, a1, a2) in &[
            ("chain1", 10_000_000_000u128, 10_000_000_000u128),
            ("chain2", 5_000_000_000u128, 5_000_000_000u128),
        ] {
            let chain_uid = ChainUid::create(chain_name.to_string()).unwrap();
            let sender = CrossChainUser::new(chain_uid.clone(), "user".to_string());
            execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
                    sender: sender.clone(),
                    pair: default_pair(),
                    tx_id: format!("reg_{chain_name}"),
                }),
            )
            .unwrap();
            execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                ExecuteMsg::AddLiquidity(VlpAddLiquidityMsg {
                    sender,
                    tx_id: format!("add_{chain_name}"),
                    liquidity: make_pair_with_amount(*a1, *a2),
                    slippage_tolerance_bps: 0,
                }),
            )
            .unwrap();
        }

        let state = STATE.load(&deps.storage).unwrap();
        let collateral_lp = COLLATERAL_LP_TOKENS
            .may_load(&deps.storage)
            .unwrap()
            .unwrap_or_default();

        let chain1_lp = CHAIN_LP_TOKENS
            .load(
                &deps.storage,
                ChainUid::create("chain1".to_string()).unwrap(),
            )
            .unwrap();
        let chain2_lp = CHAIN_LP_TOKENS
            .load(
                &deps.storage,
                ChainUid::create("chain2".to_string()).unwrap(),
            )
            .unwrap();

        assert_eq!(
            state.total_lp_tokens,
            chain1_lp + chain2_lp + collateral_lp,
            "total_lp_tokens should equal sum of chain LP tokens plus collateral"
        );
    }

    #[test]
    fn test_state_invariant_balances_increase_on_add() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let router = deps.api.addr_make("router");
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let sender = CrossChainUser::new(chain_uid.clone(), "user1".to_string());
        let info = message_info(&router, &[]);

        execute(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
                sender: sender.clone(),
                pair: default_pair(),
                tx_id: "reg".to_string(),
            }),
        )
        .unwrap();

        let b1_before = BALANCES.load(&deps.storage, token1()).unwrap();
        let b2_before = BALANCES.load(&deps.storage, token2()).unwrap();

        execute(
            deps.as_mut(),
            env.clone(),
            info,
            ExecuteMsg::AddLiquidity(VlpAddLiquidityMsg {
                sender,
                tx_id: "add".to_string(),
                liquidity: make_pair_with_amount(2_000_000_000, 2_000_000_000),
                slippage_tolerance_bps: 0,
            }),
        )
        .unwrap();

        let b1_after = BALANCES.load(&deps.storage, token1()).unwrap();
        let b2_after = BALANCES.load(&deps.storage, token2()).unwrap();

        assert!(b1_after > b1_before);
        assert!(b2_after > b2_before);
    }

    #[test]
    fn test_state_invariant_balances_decrease_on_remove() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let router = deps.api.addr_make("router");
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let sender = CrossChainUser::new(chain_uid.clone(), "user1".to_string());
        let info = message_info(&router, &[]);

        execute(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
                sender: sender.clone(),
                pair: default_pair(),
                tx_id: "reg".to_string(),
            }),
        )
        .unwrap();
        execute(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::AddLiquidity(VlpAddLiquidityMsg {
                sender: sender.clone(),
                tx_id: "add".to_string(),
                liquidity: make_pair_with_amount(10_000_000_000, 10_000_000_000),
                slippage_tolerance_bps: 0,
            }),
        )
        .unwrap();

        let b1_before = BALANCES.load(&deps.storage, token1()).unwrap();
        let b2_before = BALANCES.load(&deps.storage, token2()).unwrap();
        let chain_lp = CHAIN_LP_TOKENS.load(&deps.storage, chain_uid).unwrap();

        execute(
            deps.as_mut(),
            env,
            info,
            ExecuteMsg::RemoveLiquidity(VlpRemoveLiquidityMsg {
                sender,
                lp_allocation: chain_lp / Uint256::from(4u128),
                tx_id: "remove".to_string(),
            }),
        )
        .unwrap();

        let b1_after = BALANCES.load(&deps.storage, token1()).unwrap();
        let b2_after = BALANCES.load(&deps.storage, token2()).unwrap();

        assert!(b1_after < b1_before);
        assert!(b2_after < b2_before);
    }

    #[test]
    fn test_state_invariant_swap_conserves_k() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let router = deps.api.addr_make("router");
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let sender = CrossChainUser::new(chain_uid, "user1".to_string());
        let info = message_info(&router, &[]);

        execute(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
                sender: sender.clone(),
                pair: default_pair(),
                tx_id: "reg".to_string(),
            }),
        )
        .unwrap();
        execute(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::AddLiquidity(VlpAddLiquidityMsg {
                sender: sender.clone(),
                tx_id: "add".to_string(),
                liquidity: make_pair_with_amount(10_000_000_000, 10_000_000_000),
                slippage_tolerance_bps: 0,
            }),
        )
        .unwrap();

        let b1_before = BALANCES.load(&deps.storage, token1()).unwrap();
        let b2_before = BALANCES.load(&deps.storage, token2()).unwrap();
        let k_before = b1_before * b2_before;

        execute(
            deps.as_mut(),
            env,
            info,
            ExecuteMsg::Swap(VlpSwapMsg {
                sender,
                tx_id: "swap_k".to_string(),
                asset_in: token1(),
                amount_in: Uint256::from(50_000u128),
                min_token_out: Uint256::zero(),
                next_swaps: vec![],
                test_fail: None,
            }),
        )
        .unwrap();

        let b1_after = BALANCES.load(&deps.storage, token1()).unwrap();
        let b2_after = BALANCES.load(&deps.storage, token2()).unwrap();
        let k_after = b1_after * b2_after;

        assert!(
            k_after >= k_before,
            "k invariant violated: before={k_before}, after={k_after}"
        );
    }

    #[test]
    fn test_state_invariant_fees_only_increase() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let router = deps.api.addr_make("router");
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let sender = CrossChainUser::new(chain_uid, "user1".to_string());
        let info = message_info(&router, &[]);

        execute(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
                sender: sender.clone(),
                pair: default_pair(),
                tx_id: "reg".to_string(),
            }),
        )
        .unwrap();
        execute(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::AddLiquidity(VlpAddLiquidityMsg {
                sender: sender.clone(),
                tx_id: "add".to_string(),
                liquidity: make_pair_with_amount(10_000_000_000, 10_000_000_000),
                slippage_tolerance_bps: 0,
            }),
        )
        .unwrap();

        let mut last_fee = Uint256::zero();
        for i in 0..3 {
            execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                ExecuteMsg::Swap(VlpSwapMsg {
                    sender: sender.clone(),
                    tx_id: format!("swap_{i}"),
                    asset_in: token1(),
                    amount_in: Uint256::from(5_000u128),
                    min_token_out: Uint256::zero(),
                    next_swaps: vec![],
                    test_fail: None,
                }),
            )
            .unwrap();

            let state = STATE.load(&deps.storage).unwrap();
            let fee_now = state
                .total_fees_collected
                .lp_fees
                .get_fee(token1().to_string().as_str());
            assert!(
                fee_now >= last_fee,
                "Fees decreased after swap {i}: before={last_fee}, after={fee_now}"
            );
            last_fee = fee_now;
        }
    }
}
