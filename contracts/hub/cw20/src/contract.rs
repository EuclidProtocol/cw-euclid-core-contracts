#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo, Response};
use cw2::set_contract_version;

use crate::state::STATE;
use crate::{execute, query};
use cw20::{Cw20Coin, Cw20ExecuteMsg};
use euclid::error::ContractError;
use euclid::msgs::cw20::{ExecuteMsg, InstantiateMsg, QueryMsg};

use cw20_base::{
    contract::{execute as execute_cw20, instantiate as cw20_instantiate, query as cw20_query},
    state::BALANCES,
};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:cw20";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let cw20_resp = cw20_instantiate(deps, env, info, msg.into())?;

    Ok(cw20_resp)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    Ok(execute_cw20(deps, env, info, msg.into())?)
    // match msg {
    //     ExecuteMsg::Transfer { recipient, amount } => Ok(execute_cw20(
    //         deps,
    //         env,
    //         info,
    //         Cw20ExecuteMsg::Transfer { recipient, amount },
    //     )?),
    //     ExecuteMsg::Burn { amount } => todo!(),
    //     ExecuteMsg::Send {
    //         contract,
    //         amount,
    //         msg,
    //     } => todo!(),
    //     ExecuteMsg::IncreaseAllowance {
    //         spender,
    //         amount,
    //         expires,
    //     } => todo!(),
    //     ExecuteMsg::DecreaseAllowance {
    //         spender,
    //         amount,
    //         expires,
    //     } => todo!(),
    //     ExecuteMsg::TransferFrom {
    //         owner,
    //         recipient,
    //         amount,
    //     } => todo!(),
    //     ExecuteMsg::SendFrom {
    //         owner,
    //         contract,
    //         amount,
    //         msg,
    //     } => todo!(),
    //     ExecuteMsg::BurnFrom { owner, amount } => todo!(),
    //     ExecuteMsg::Mint { recipient, amount } => todo!(),
    //     ExecuteMsg::UpdateMarketing {
    //         project,
    //         description,
    //         marketing,
    //     } => todo!(),
    //     ExecuteMsg::UploadLogo(_) => todo!(),
    // }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    Ok(cw20_query(deps, env, msg.into())?)
    // match msg {
    // QueryMsg::TokenInfo {  } => todo!(),
    // QueryMsg::Minter {  } => todo!(),
    // QueryMsg::Allowance { owner, spender } => todo!(),
    // QueryMsg::AllAllowances { owner, start_after, limit } => todo!(),
    // QueryMsg::AllAccounts { start_after, limit } => todo!(),
    // QueryMsg::MarketingInfo {  } => todo!(),
    // QueryMsg::DownloadLogo {  } => todo!(),
    // QueryMsg::Balance { address } => todo!(),
    // }
}
