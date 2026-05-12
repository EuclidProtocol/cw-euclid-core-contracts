#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, Uint128, Uint256,
};
use cw2::set_contract_version;
use euclid::msgs::escrow::Cw20InstantiateResponse;

use crate::cw20::{
    execute_burn, execute_burn_from, execute_decrease_allowance, execute_increase_allowance,
    execute_mint, execute_send, execute_send_from, execute_transfer, execute_transfer_from,
    execute_update_marketing, execute_upload_logo, instantiate_cw20, query_all_accounts,
    query_all_allowances, query_allowance, query_balance, query_download_logo,
    query_marketing_info, query_minter, query_token_info,
};
use crate::execute::execute_update_state;
use crate::state::{State, STATE};
use euclid::error::ContractError;
use euclid::msgs::lp_token::msg::{ExecuteMsg, InstantiateMsg, QueryMsg, StateResponse};

// version info for migration info
pub(crate) const CONTRACT_NAME: &str = "crates.io:lp_token";
pub(crate) const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

fn to_u128(amount: Uint256) -> Result<Uint128, ContractError> {
    Uint128::try_from(amount).map_err(|_| ContractError::new("Amount exceeds Uint128 maximum"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let cw20_resp = instantiate_cw20(deps.branch(), env.clone(), info, &msg)?;
    let state = State {
        token_pair: msg.token_pair.clone(),
        factory_address: msg.factory,
        vlp: msg.vlp.clone(),
    };
    STATE.save(deps.storage, &state)?;

    let data = Cw20InstantiateResponse {
        pair: msg.token_pair,
        address: env.contract.address.into_string(),
        vlp: msg.vlp,
    };

    Ok(cw20_resp.set_data(to_json_binary(&data)?))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::UpdateState {
            token_pair,
            factory_address,
            vlp,
        } => execute_update_state(deps, env, info, token_pair, factory_address, vlp),
        ExecuteMsg::Transfer { recipient, amount } => {
            execute_transfer(deps, env, info, recipient, to_u128(amount)?)
        }
        ExecuteMsg::Burn { amount } => execute_burn(deps, env, info, to_u128(amount)?),
        ExecuteMsg::Send {
            contract,
            amount,
            msg,
        } => execute_send(deps, env, info, contract, to_u128(amount)?, msg),
        ExecuteMsg::IncreaseAllowance {
            spender,
            amount,
            expires,
        } => execute_increase_allowance(deps, env, info, spender, to_u128(amount)?, expires),
        ExecuteMsg::DecreaseAllowance {
            spender,
            amount,
            expires,
        } => execute_decrease_allowance(deps, env, info, spender, to_u128(amount)?, expires),
        ExecuteMsg::TransferFrom {
            owner,
            recipient,
            amount,
        } => execute_transfer_from(deps, env, info, owner, recipient, to_u128(amount)?),
        ExecuteMsg::SendFrom {
            owner,
            contract,
            amount,
            msg,
        } => execute_send_from(deps, env, info, owner, contract, to_u128(amount)?, msg),
        ExecuteMsg::BurnFrom { owner, amount } => {
            execute_burn_from(deps, env, info, owner, to_u128(amount)?)
        }
        ExecuteMsg::Mint { recipient, amount } => {
            execute_mint(deps, env, info, recipient, to_u128(amount)?)
        }
        ExecuteMsg::UpdateMarketing {
            project,
            description,
            marketing,
        } => execute_update_marketing(deps, env, info, project, description, marketing),
        ExecuteMsg::UploadLogo(logo) => execute_upload_logo(deps, env, info, logo),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::State {} => {
            let state = STATE.load(deps.storage)?;
            let response = StateResponse {
                token_pair: state.token_pair,
                factory_address: state.factory_address,
                vlp: state.vlp,
            };
            Ok(to_json_binary(&response)?)
        }
        QueryMsg::Balance { address } => query_balance(deps, address),
        QueryMsg::TokenInfo {} => query_token_info(deps),
        QueryMsg::Minter {} => query_minter(deps),
        QueryMsg::Allowance { owner, spender } => query_allowance(deps, owner, spender),
        QueryMsg::AllAllowances {
            owner,
            start_after,
            limit,
        } => query_all_allowances(deps, owner, start_after, limit),
        QueryMsg::AllAccounts { start_after, limit } => {
            query_all_accounts(deps, start_after, limit)
        }
        QueryMsg::MarketingInfo {} => query_marketing_info(deps),
        QueryMsg::DownloadLogo {} => query_download_logo(deps),
    }
}
