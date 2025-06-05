use cosmwasm_std::{
    ensure, to_json_binary, CosmosMsg, DepsMut, Env, IbcTimeout, MessageInfo, Response,
};
use euclid::{
    error::ContractError,
    events::simple_event,
    msgs::{
        migrator::IbcExecuteMsg, router::RouterMigrateMsg, virtual_balance::VBalanceMigrateMsg,
        vlp::VlpMigrateMsg,
    },
    timeout::get_timeout,
};

use crate::state::{State, STATE};

#[allow(clippy::too_many_arguments)]
pub fn update_state(
    deps: DepsMut,
    info: MessageInfo,
    router: Option<String>,
    virtual_balance: Option<String>,
    vlp: Option<String>,
    admin: Option<String>,
) -> Result<Response, ContractError> {
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

    // Verify that the vlp is a valid address
    let verified_vlp = if let Some(vlp) = vlp {
        deps.api.addr_validate(&vlp)?;
        vlp
    } else {
        state.vlp
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
        vlp: verified_vlp,
        admin: verified_admin,
    };

    STATE.save(deps.storage, &new_state)?;
    let response = Response::new()
        .add_attribute("action", "update_state")
        .add_event(simple_event());
    Ok(response)
}

pub fn migrate_vbalance(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    vbalance_address: String,
    router_address: String,
    channel_id: String,
    timeout: Option<u64>,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.admin, ContractError::Unauthorized {});
    let vbalance_query_msg = euclid::msgs::virtual_balance::QueryMsg::GetMigrateData {};
    let mut vbalance_query_response: VBalanceMigrateMsg = deps.querier.query(
        &cosmwasm_std::QueryRequest::Wasm(cosmwasm_std::WasmQuery::Smart {
            contract_addr: state.virtual_balance,
            msg: to_json_binary(&vbalance_query_msg)?,
        }),
    )?;
    vbalance_query_response.state.state.router = router_address;

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
pub fn migrate_vlp(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    virtual_balance_address: String,
    router_address: String,
    vlp_address: String,
    channel_id: String,
    timeout: Option<u64>,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.admin, ContractError::Unauthorized {});
    let vlp_query_msg = euclid::msgs::vlp::QueryMsg::GetMigrateData {};
    let mut vlp_query_response: VlpMigrateMsg = deps.querier.query(
        &cosmwasm_std::QueryRequest::Wasm(cosmwasm_std::WasmQuery::Smart {
            contract_addr: state.vlp,
            msg: to_json_binary(&vlp_query_msg)?,
        }),
    )?;
    vlp_query_response.state.state.router = router_address;
    vlp_query_response.state.state.virtual_balance = virtual_balance_address.clone();

    let data = IbcExecuteMsg::MigrateVLP {
        migrate_msg: vlp_query_response,
        vlp_address,
    };
    let timeout = get_timeout(timeout)?;

    let ibc_packet = CosmosMsg::Ibc(cosmwasm_std::IbcMsg::SendPacket {
        channel_id,
        data: to_json_binary(&data)?,
        timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(timeout)),
    });
    Ok(Response::new().add_message(ibc_packet))
}

pub fn migrate_router(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    virtual_balance_address: String,
    router_address: String,
    vlp_address: String,
    channel_id: String,
    timeout: Option<u64>,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.admin, ContractError::Unauthorized {});
    let router_query_msg = euclid::msgs::router::QueryMsg::GetMigrateData {};
    let mut router_query_response: RouterMigrateMsg = deps.querier.query(
        &cosmwasm_std::QueryRequest::Wasm(cosmwasm_std::WasmQuery::Smart {
            contract_addr: state.router,
            msg: to_json_binary(&router_query_msg)?,
        }),
    )?;

    let data = IbcExecuteMsg::MigrateRouter {
        migrate_msg: router_query_response,
        router_address,
    };
    let timeout = get_timeout(timeout)?;

    let ibc_packet = CosmosMsg::Ibc(cosmwasm_std::IbcMsg::SendPacket {
        channel_id,
        data: to_json_binary(&data)?,
        timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(timeout)),
    });
    Ok(Response::new().add_message(ibc_packet))
}
