use cosmwasm_std::{
    ensure, to_json_binary, CosmosMsg, Decimal, Decimal256, DepsMut, Env, IbcTimeout, MessageInfo,
    Response, SubMsg, Uint128, Uint64, WasmMsg,
};
use euclid::{
    chain::{ChainUid, CrossChainUser},
    error::ContractError,
    events::{liquidity_event, simple_event, tx_event, TxType},
    fee::{Fee, BPS_50_PERCENT, MAX_FEE_BPS},
    liquidity::AddLiquidityResponse,
    msgs::{
        migrator::IbcExecuteMsg,
        stable_vlp::{VlpRemoveLiquidityResponse, VlpSwapResponse},
        virtual_balance::{ExecuteTransfer, VBalanceMigrateMsg},
    },
    pool::PoolCreationResponse,
    swap::NextSwapVlp,
    timeout::get_timeout,
    token::{Pair, PairWithAmount, Token},
    utils::math::Decimal256Ext,
};

use crate::state::{State, STATE};

#[allow(clippy::too_many_arguments)]
pub fn update_state(
    deps: DepsMut,
    info: MessageInfo,
    router: Option<String>,
    virtual_balance: Option<String>,
    admin: Option<String>,
) -> Result<Response, ContractError> {
    let mut response = Response::new()
        .add_attribute("action", "update_state")
        .add_event(simple_event());
    let state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.admin, ContractError::Unauthorized {});
    // Verify that the router is a valid address
    let verified_router = if let Some(router) = router {
        deps.api.addr_validate(&router)?;
        router
    } else {
        state.router
    };

    // Verify that the virtual balance is a valid address
    let verified_virtual_balance = if let Some(virtual_balance) = virtual_balance {
        deps.api.addr_validate(&virtual_balance)?;
        virtual_balance
    } else {
        state.virtual_balance
    };

    // Verify that the admin is a valid address
    let verified_admin = if let Some(admin) = admin {
        deps.api.addr_validate(&admin)?;
        admin
    } else {
        state.admin
    };

    let new_state = State {
        router: verified_router,
        virtual_balance: verified_virtual_balance,
        admin: verified_admin,
    };

    STATE.save(deps.storage, &new_state)?;

    Ok(response)
}

pub fn migrate_vbalance(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    vbalance_address: String,
    channel_id: String,
    timeout: Option<u64>,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.admin, ContractError::Unauthorized {});
    let vbalance_query_msg = euclid::msgs::virtual_balance::QueryMsg::GetMigrateData {};
    let vbalance_query_response: VBalanceMigrateMsg = deps.querier.query(
        &cosmwasm_std::QueryRequest::Wasm(cosmwasm_std::WasmQuery::Smart {
            contract_addr: state.virtual_balance,
            msg: to_json_binary(&vbalance_query_msg)?,
        }),
    )?;

    let data = IbcExecuteMsg::MigrateVBalance {
        migrate_msg: vbalance_query_response,
        vbalance_address,
    };
    let timeout = get_timeout(timeout)?;

    let ibc_packet = CosmosMsg::Ibc(cosmwasm_std::IbcMsg::SendPacket {
        channel_id,
        data: to_json_binary(&data)?,
        timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(timeout)),
    });
    Ok(Response::new().add_message(ibc_packet))
}
