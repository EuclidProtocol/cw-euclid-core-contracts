use std::collections::HashMap;

use crate::execute::{provide_liquidity, swap, withdraw_liquidity};
use crate::query::{
    query_all_pools, query_fee, query_liquidity, query_pool, query_simulate_swap, query_state,
    query_total_fees_collected, query_total_fees_per_denom,
};
use crate::state::{
    Config, PairInfo, PoolState, Precisions, AMP_FACTOR, BALANCES, CHAIN_LP_TOKENS,
    CONCENTRATED_BALANCES, CONFIG, STATE,
};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_json, Binary, Decimal256, Deps, DepsMut, Env, MessageInfo, Reply, Response, StdError,
    SubMsg, SubMsgResponse, SubMsgResult, Uint128,
};
use cw2::set_contract_version;
use euclid::error::ContractError;
use euclid::fee::{DenomFees, TotalFees};
use euclid::msgs::concentrated_vlp::{
    tf_create_denom_msg, AmpGamma, ConcentratedPoolParams, ExecuteMsg, InstantiateMsg,
    MsgCreateDenomResponse, PoolParams, PriceState, QueryMsg, DEFAULT_AMP_FACTOR,
};
use euclid::pool::{register_pool, update_fee, update_state, PoolType};
use euclid::pool::{PoolConfig, State};
/// An LP token's precision.
pub(crate) const LP_TOKEN_PRECISION: u8 = 6;
// version info for migration info
const CONTRACT_NAME: &str = "crates.io:concentrated_vlp";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    Precisions::store_precisions(
        deps.storage,
        &[msg.asset_infos[0].clone()],
        &env.contract.address,
    )
    .unwrap();
    Precisions::store_precisions(
        deps.storage,
        &[msg.asset_infos[1].clone()],
        &env.contract.address,
    )
    .unwrap();
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    // Concentrated VLP Config
    let factory_addr = deps.api.addr_validate(&msg.factory_addr)?;
    // Initializing cumulative prices
    let cumulative_prices = vec![
        (
            msg.asset_infos[0].clone(),
            msg.asset_infos[1].clone(),
            Uint128::zero(),
        ),
        (
            msg.asset_infos[1].clone(),
            msg.asset_infos[0].clone(),
            Uint128::zero(),
        ),
    ];

    let params: ConcentratedPoolParams =
        from_json(msg.clone().init_params.ok_or(ContractError::Generic {
            err: "InitParamsNotFound".to_string(),
        })?)?;

    let pool_params = PoolParams {
        mid_fee: params.mid_fee,
        out_fee: params.out_fee,
        fee_gamma: params.fee_gamma,
        repeg_profit_threshold: params.repeg_profit_threshold,
        min_price_scale_delta: params.min_price_scale_delta,
        ma_half_time: params.ma_half_time,
        allowed_xcp_profit_drop: params.allowed_xcp_profit_drop.unwrap_or_default(),
        xcp_profit_losses_threshold: params.xcp_profit_losses_threshold.unwrap_or_default(),
    };

    let pool_state = PoolState {
        initial: AmpGamma::default(),
        future: AmpGamma {
            amp: params.amp,
            gamma: params.gamma,
        },
        future_time: env.block.time.seconds(),
        initial_time: 0,
        price_state: PriceState {
            oracle_price: params.price_scale.into(),
            last_price: params.price_scale.into(),
            price_scale: params.price_scale.into(),
            last_price_update: env.block.time.seconds(),
            xcp_profit: Decimal256::zero(),
            xcp_profit_real: Decimal256::zero(),
            xcp_profit_losses: Decimal256::zero(),
        },
    };

    let config = Config {
        pair_info: PairInfo {
            contract_addr: env.contract.address.clone(),
            liquidity_token: "".to_owned(),
            asset_infos: msg.asset_infos.clone(),
            pair_type: msg.pair_type.clone(),
        },
        factory_addr,
        block_time_last: env.block.time.seconds(),
        cumulative_prices,
        pool_params,
        pool_state,
        owner: None,
        track_asset_balances: params.track_asset_balances.unwrap_or_default(),
        fee_share: None,
        tracker_addr: None,
    };

    if config.track_asset_balances {
        for asset in &config.pair_info.asset_infos {
            CONCENTRATED_BALANCES.save(deps.storage, asset, &Uint128::zero(), env.block.height)?;
        }
    }

    CONFIG.save(deps.storage, &config)?;

    // let create_denom_msg = SubMsg::reply_on_success(
    //     tf_create_denom_msg(env.contract.address.to_string(), "lp_subdenom"),
    //     1,
    // );

    let state = State {
        pair: msg.pair,
        virtual_balance: msg.virtual_balance,
        router: info.sender.to_string(),
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
        total_lp_tokens: Uint128::zero(),
        admin: msg.admin,
        pool_type: PoolType::Concentrated,
    };

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    STATE.save(deps.storage, &state)?;

    BALANCES.save(deps.storage, state.pair.token_1, &Uint128::zero())?;
    BALANCES.save(deps.storage, state.pair.token_2, &Uint128::zero())?;

    let response =
        msg.execute
            .map_or(Ok(Response::default()), |execute_msg| match execute_msg {
                ExecuteMsg::RegisterPool {
                    sender,
                    pair,
                    tx_id,
                    ..
                } => register_pool(
                    deps,
                    env.clone(),
                    info.clone(),
                    &STATE,
                    &CHAIN_LP_TOKENS,
                    None,
                    sender,
                    pair,
                    tx_id,
                    PoolType::Concentrated,
                ),
                _ => Err(ContractError::Unauthorized {}),
            })?;

    Ok(response
        // .add_submessage(create_denom_msg)
        .add_attribute("method", "instantiate")
        .add_attribute("vlp_address", env.contract.address.to_string())
        .add_attribute("owner", info.sender)
        .add_attribute("pool_type", "concentrated"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::RegisterPool {
            sender,
            pair,
            tx_id,
        } => register_pool(
            deps,
            env,
            info,
            &STATE,
            &CHAIN_LP_TOKENS,
            None,
            sender,
            pair,
            tx_id,
            PoolType::Concentrated,
        ),
        ExecuteMsg::UpdateFee {
            lp_fee_bps,
            euclid_fee_bps,
            recipient,
        } => update_fee(deps, info, &STATE, lp_fee_bps, euclid_fee_bps, recipient),
        ExecuteMsg::AddLiquidity {
            assets,
            slippage_tolerance,
            auto_stake,
            receiver,
            min_lp_to_receive,
            sender,
            tx_id,
            liquidity,
            slippage_tolerance_bps,
        } => provide_liquidity(
            deps,
            env,
            info,
            assets,
            slippage_tolerance,
            auto_stake,
            receiver,
            min_lp_to_receive,
            sender,
            tx_id,
            liquidity,
            slippage_tolerance_bps,
        ),
        ExecuteMsg::RemoveLiquidity { assets } => withdraw_liquidity(deps, env, info, assets),
        ExecuteMsg::Swap {
            sender,
            offer_asset,
            belief_price,
            max_spread,
            to,
        } => swap(deps, env, sender, offer_asset, belief_price, max_spread, to),
        ExecuteMsg::UpdateState {
            router,
            virtual_balance,
            fee,
            last_updated,
            admin,
            amp_factor,
        } => update_state(
            deps,
            info,
            &STATE,
            Some(&AMP_FACTOR),
            router,
            virtual_balance,
            fee,
            last_updated,
            admin,
            amp_factor,
        ),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::State {} => query_state(deps),
        QueryMsg::SimulateSwap {
            asset,
            asset_amount,
            swaps,
        } => query_simulate_swap(deps, asset, asset_amount, swaps),
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
    println!("reply: {:?}", msg);
    match msg.id {
        1 => {
            if let SubMsgResult::Ok(SubMsgResponse { data: Some(b), .. }) = msg.result {
                let MsgCreateDenomResponse { new_token_denom } =
                    b.try_into().map_err(|_| ContractError::Generic {
                        err: "Failed to parse MsgCreateDenomResponse".to_string(),
                    })?;

                let config = CONFIG.load(deps.storage)?;

                // let tracking = config.track_asset_balances;
                // let mut sub_msgs = vec![];

                // #[cfg(feature = "injective")]
                // let tracking = false;

                // if tracking {
                //     let factory_config =
                //         query_factory_config(&deps.querier, config.factory_addr.clone())?;
                //     let tracker_config = query_tracker_config(&deps.querier, config.factory_addr)?;
                //     // Instantiate tracking contract
                //     let sub_msg: Vec<SubMsg> = vec![SubMsg::reply_on_success(
                //         WasmMsg::Instantiate {
                //             admin: Some(factory_config.owner.to_string()),
                //             code_id: tracker_config.code_id,
                //             msg: to_json_binary(&tokenfactory_tracker::InstantiateMsg {
                //                 tokenfactory_module_address: tracker_config
                //                     .token_factory_addr
                //                     .to_string(),
                //                 tracked_denom: new_token_denom.clone(),
                //                 track_over_seconds: false,
                //             })?,
                //             funds: vec![],
                //             label: format!("{new_token_denom} tracking contract"),
                //         },
                //         ReplyIds::InstantiateTrackingContract as u64,
                //     )];

                //     sub_msgs.extend(sub_msg);
                // }

                CONFIG.update(deps.storage, |mut config| {
                    if !config.pair_info.liquidity_token.is_empty() {
                        return Err(StdError::generic_err(
                            "Liquidity token is already set in the config",
                        ));
                    }

                    config.pair_info.liquidity_token = new_token_denom.clone();
                    Ok(config)
                })?;

                Ok(Response::new()
                    // .add_submessages(sub_msgs)
                    .add_attribute("lp_denom", new_token_denom))
            } else {
                Err(ContractError::Generic {
                    err: "Failed to parse MsgCreateDenomResponse".to_string(),
                })
            }
        }

        id => Err(ContractError::Generic {
            err: format!("Unknown reply id: {id}"),
        }),
    }
}
