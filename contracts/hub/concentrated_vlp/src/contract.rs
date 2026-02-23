use std::collections::HashMap;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_json, to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, Uint128,
};
use cw2::set_contract_version;
use euclid::{
    error::ContractError,
    fee::{DenomFees, TotalFees},
    msgs::vlp::{
        base::{
            PoolType, State, VlpConcentratedAddLiquidityResponse, VlpConcentratedRemoveLiquidityResponse,
            NEXT_SWAP_REPLY_ID,
        },
        concentrated::msg::{ExecuteMsg, InstantiateMsg, QueryMsg},
    },
};
use euclid_pool::{
    add_liquidity, execute_swap, register_pool, remove_liquidity, update_fee, update_state,
    SwapCalculationMethod,
};

use crate::{
    query::{
        query_all_pools, query_fee, query_liquidity, query_pool, query_simulate_swap, query_state,
        query_total_fees_collected, query_total_fees_per_denom,
    },
    reply,
    state::{
        initialize_position_nonce, next_position_id, BALANCES, CHAIN_LP_TOKENS,
        COLLATERAL_LP_TOKENS, POOL_KEY, POSITIONS, STATE,
    },
};

const CONTRACT_NAME: &str = "crates.io:concentrated_vlp";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    msg.pair.validate()?;

    let state = State {
        pair: msg.pair.clone(),
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
        total_lp_tokens: Uint128::zero(),
        admin: msg.admin,
    };

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    STATE.save(deps.storage, &state)?;

    BALANCES.save(deps.storage, state.pair.token_1.clone(), &Uint128::zero())?;
    BALANCES.save(deps.storage, state.pair.token_2.clone(), &Uint128::zero())?;
    COLLATERAL_LP_TOKENS.save(deps.storage, &Uint128::zero())?;

    POOL_KEY.save(
        deps.storage,
        &euclid::msgs::vlp::base::PoolKey {
            pair: msg.pair,
            pool_type: PoolType::Concentrated {
                fee_tier_bps: msg.fee_tier_bps,
                tick_spacing: msg.tick_spacing,
            },
        },
    )?;
    initialize_position_nonce(deps.storage, env.contract.address.as_str())?;

    let response =
        msg.execute
            .map_or(Ok(Response::default()), |execute_msg| match execute_msg {
                ExecuteMsg::RegisterPool(register_pool_msg) => {
                    let register_res = register_pool(
                        deps,
                        env.clone(),
                        info.clone(),
                        &STATE,
                        &CHAIN_LP_TOKENS,
                        None,
                        register_pool_msg.sender.clone(),
                        register_pool_msg.pool_key.pair.clone(),
                        register_pool_msg.tx_id.clone(),
                    )?;
                    let ack = euclid::msgs::vlp::base::ConcentratedPoolCreationResponse {
                        vlp_contract: env.contract.address.to_string(),
                        tx_id: register_pool_msg.tx_id,
                        sender: register_pool_msg.sender,
                        pool_key: register_pool_msg.pool_key,
                    };
                    Ok(register_res.set_data(to_json_binary(&ack)?))
                }
                _ => Err(ContractError::Unauthorized {}),
            })?;

    Ok(response
        .add_attribute("method", "instantiate")
        .add_attribute("vlp_address", env.contract.address.to_string())
        .add_attribute("owner", info.sender)
        .add_attribute("pool_type", "concentrated"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::RegisterPool(register_pool_msg) => {
            let register_res = register_pool(
                deps,
                env.clone(),
                info,
                &STATE,
                &CHAIN_LP_TOKENS,
                None,
                register_pool_msg.sender.clone(),
                register_pool_msg.pool_key.pair.clone(),
                register_pool_msg.tx_id.clone(),
            )?;
            let ack = euclid::msgs::vlp::base::ConcentratedPoolCreationResponse {
                vlp_contract: env.contract.address.to_string(),
                tx_id: register_pool_msg.tx_id,
                sender: register_pool_msg.sender,
                pool_key: register_pool_msg.pool_key,
            };
            Ok(register_res.set_data(to_json_binary(&ack)?))
        }
        ExecuteMsg::AddLiquidity(add_liquidity_msg) => {
            let pool_key = add_liquidity_msg.pool_key.clone();
            let sender = add_liquidity_msg.sender.clone();
            let tx_id = add_liquidity_msg.tx_id.clone();
            let liquidity = add_liquidity_msg.liquidity.clone();
            let lp_position_id = add_liquidity_msg.position_id;
            let lower_tick_index = add_liquidity_msg.lower_tick_index;
            let upper_tick_index = add_liquidity_msg.upper_tick_index;

            let response = add_liquidity(
                deps.branch(),
                env,
                info,
                &STATE,
                &BALANCES,
                &CHAIN_LP_TOKENS,
                &COLLATERAL_LP_TOKENS,
                sender.clone(),
                liquidity.clone(),
                add_liquidity_msg.slippage_tolerance_bps,
                tx_id.clone(),
            )?;

            let ack: euclid::liquidity::AddLiquidityResponse =
                from_json(response.data.clone().unwrap_or_default())?;

            let position_id = lp_position_id.unwrap_or(next_position_id(deps.storage)?);

            let mut position = POSITIONS
                .may_load(deps.storage, position_id.u128())?
                .unwrap_or(crate::state::ConcentratedPosition {
                    owner: sender.clone(),
                    lower_tick_index,
                    upper_tick_index,
                    liquidity: Uint128::zero(),
                    pool_key: pool_key.clone(),
                });

            if position.owner != sender {
                return Err(ContractError::Unauthorized {});
            }

            position.liquidity = position.liquidity.checked_add(ack.mint_lp_tokens)?;
            position.lower_tick_index = lower_tick_index;
            position.upper_tick_index = upper_tick_index;
            position.pool_key = pool_key.clone();
            POSITIONS.save(deps.storage, position_id.u128(), &position)?;

            let concentrated_ack = VlpConcentratedAddLiquidityResponse {
                liquidity_added: liquidity,
                liquidity_delta: ack.mint_lp_tokens,
                position_id,
                tx_id,
                sender,
                vlp_address: ack.vlp_address,
                pool_key,
            };

            Ok(response
                .add_attribute("position_id", position_id)
                .set_data(to_json_binary(&concentrated_ack)?))
        }
        ExecuteMsg::RemoveLiquidity(remove_liquidity_msg) => {
            let mut position = POSITIONS
                .may_load(deps.storage, remove_liquidity_msg.position_id.u128())?
                .ok_or(ContractError::new("Position not found"))?;

            if position.owner != remove_liquidity_msg.sender {
                return Err(ContractError::Unauthorized {});
            }

            let response = remove_liquidity(
                deps.branch(),
                env,
                info,
                &STATE,
                &BALANCES,
                &CHAIN_LP_TOKENS,
                remove_liquidity_msg.sender.clone(),
                remove_liquidity_msg.lp_allocation,
                remove_liquidity_msg.tx_id.clone(),
            )?;

            let ack: euclid::msgs::vlp::base::VlpRemoveLiquidityResponse =
                from_json(response.data.clone().unwrap_or_default())?;

            position.liquidity = position
                .liquidity
                .checked_sub(remove_liquidity_msg.lp_allocation)?;
            if position.liquidity.is_zero() {
                POSITIONS.remove(deps.storage, remove_liquidity_msg.position_id.u128());
            } else {
                POSITIONS.save(deps.storage, remove_liquidity_msg.position_id.u128(), &position)?;
            }

            let concentrated_ack = VlpConcentratedRemoveLiquidityResponse {
                liquidity_released: ack.liquidity_released,
                liquidity_delta: remove_liquidity_msg.lp_allocation,
                position_id: remove_liquidity_msg.position_id,
                tx_id: ack.tx_id,
                sender: ack.sender,
                vlp_address: ack.vlp_address,
                pool_key: remove_liquidity_msg.pool_key,
            };

            Ok(response.set_data(to_json_binary(&concentrated_ack)?))
        }
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
        } => update_fee(deps, info, &STATE, lp_fee_bps, euclid_fee_bps, recipient),
        ExecuteMsg::UpdateState { admin } => update_state(deps, info, &STATE, admin),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::State {} => query_state(deps),
        QueryMsg::SimulateSwap(msg) => {
            query_simulate_swap(deps, msg.asset, msg.asset_amount, msg.swaps)
        }
        QueryMsg::Liquidity {} => query_liquidity(deps, env),
        QueryMsg::Fee {} => query_fee(deps),
        QueryMsg::TotalFeesCollected {} => query_total_fees_collected(deps),
        QueryMsg::TotalFeesPerDenom { denom } => query_total_fees_per_denom(deps, denom),
        QueryMsg::Pool { chain_uid, pool_key } => query_pool(deps, chain_uid, pool_key),
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
