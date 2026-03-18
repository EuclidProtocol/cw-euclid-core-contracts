use cosmwasm_std::{ensure, from_json, DepsMut, Env, MessageInfo, Response, Uint128};
use cw20::Cw20ReceiveMsg;
use euclid::{
    admin,
    cross_chain_user::CrossChainUser,
    error::ContractError,
    msgs::{
        factory::{
            cw20::FactoryCw20HookMsg, euclid_receive::FactoryEuclidReceiveHook, ExecuteMsg,
            ExecuteSwapRequest, ManageFactoryState,
        },
        hook::EuclidReceive,
    },
    token::TokenType,
};

use crate::{
    execute::{
        pool::remove_liquidity_request, swap::execute_swap_request, token::execute_deposit_token,
    },
    state::{ADMIN, POSITION_TOKEN_CONTRACT, STATE},
};

pub fn execute_manage_factory_state(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ManageFactoryState,
) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;
    let mut admins = ADMIN.load(deps.storage)?;
    match msg {
        ManageFactoryState::UpdateAdmin { admin, admin_type } => {
            let (updated_admins, response) =
                admin::update_admin(&admins, &deps, &env, &info.sender, admin, admin_type)?;
            admins = updated_admins;
            ADMIN.save(deps.storage, &admins)?;
            Ok(response)
        }
        ManageFactoryState::UpdateEscrowCodeId { escrow_code_id } => {
            ensure!(
                admins.migration_admin == info.sender,
                ContractError::Unauthorized {}
            );
            state.escrow_code_id = escrow_code_id;
            STATE.save(deps.storage, &state)?;
            Ok(Response::new().add_attribute("escrow_code_id", escrow_code_id.to_string()))
        }
        ManageFactoryState::UpdateLPCodeId { lp_code_id } => {
            ensure!(
                admins.migration_admin == info.sender,
                ContractError::Unauthorized {}
            );
            state.lp_code_id = lp_code_id;
            STATE.save(deps.storage, &state)?;
            Ok(Response::new().add_attribute("lp_code_id", lp_code_id.to_string()))
        }
        ManageFactoryState::UpdateRelayerAddress { relayer_address } => {
            ensure!(
                admins.general_admin == info.sender,
                ContractError::Unauthorized {}
            );
            let relayer_address = deps.api.addr_validate(relayer_address.as_str())?;
            state.relayer_contract = relayer_address.clone();
            STATE.save(deps.storage, &state)?;
            Ok(Response::new().add_attribute("relayer_address", relayer_address))
        }
        ManageFactoryState::UpdatePositionTokenContract {
            position_token_contract,
        } => {
            let position_token_contract =
                deps.api.addr_validate(position_token_contract.as_str())?;
            POSITION_TOKEN_CONTRACT.save(deps.storage, &position_token_contract)?;
            Ok(Response::new().add_attribute("position_token_contract", position_token_contract))
        }
        ManageFactoryState::UpdatePositionTokenCodeId {
            position_token_code_id,
        } => {
            ensure!(
                admins.migration_admin == info.sender,
                ContractError::Unauthorized {}
            );
            state.position_token_code_id = position_token_code_id;
            STATE.save(deps.storage, &state)?;
            Ok(Response::new()
                .add_attribute("position_token_code_id", position_token_code_id.to_string()))
        }
    }
}

/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
///
/// * **cw20_msg** is the CW20 message that has to be processed.
pub fn receive_cw20(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;

    let sender = CrossChainUser::new(state.chain_uid.clone(), cw20_msg.sender);

    match from_json(&cw20_msg.msg)? {
        FactoryCw20HookMsg::Deposit {
            token,
            recipients,
            cross_chain_config,
        } => {
            let contract_adr = info.sender.clone();

            let asset_in = token.with_type(TokenType::Smart {
                contract_address: contract_adr.to_string(),
            });
            let amount_in = cw20_msg.amount;

            // ensure that the contract address is the same as the asset contract address
            execute_deposit_token(
                &mut deps,
                env,
                info,
                sender,
                asset_in,
                amount_in,
                recipients,
                cross_chain_config,
            )
        }
        FactoryCw20HookMsg::RemoveLiquidity {
            pair,
            recipient,
            cross_chain_config,
        } => remove_liquidity_request(
            &mut deps,
            info,
            env,
            sender,
            pair,
            cw20_msg.amount,
            recipient,
            cross_chain_config,
        ),
        // Allow to swap using a CW20 hook message
        FactoryCw20HookMsg::Swap {
            asset_in,
            asset_out,
            min_amount_out,
            swaps,
            recipients,
            partner_fee,
            cross_chain_config,
        } => {
            let contract_adr = info.sender.clone();

            // ensure that contract address is same as asset being swapped
            ensure!(
                contract_adr.to_string() == asset_in.token_type.get_smart_contract_address()?,
                ContractError::AssetDoesNotExist {}
            );

            let amount_in = cw20_msg.amount;

            // ensure that the contract address is the same as the asset contract address
            execute_swap_request(
                &mut deps,
                env,
                info,
                sender,
                asset_in,
                amount_in,
                asset_out,
                min_amount_out,
                swaps,
                recipients,
                cross_chain_config,
                partner_fee,
            )
        }

        FactoryCw20HookMsg::EuclidReceive(euclid_receive) => {
            receive_euclid_cw20(deps, env, info, sender, cw20_msg.amount, euclid_receive)
        }
    }
}

pub fn receive_euclid_native(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    euclid_receive: EuclidReceive,
) -> Result<Response, ContractError> {
    match from_json::<FactoryEuclidReceiveHook>(euclid_receive.msg.clone())? {
        FactoryEuclidReceiveHook::Swap {
            asset_in,
            asset_out,
            min_amount_out,
            swaps,
            recipients,
            cross_chain_config,
            partner_fee,
        } => {
            let amount_in = if let TokenType::Native { denom } = &asset_in.token_type {
                info.funds
                    .iter()
                    .find(|fund| fund.denom == *denom)
                    .ok_or(ContractError::InsufficientFunds {})?
                    .amount
            } else {
                return Err(ContractError::InvalidAsset {
                    asset: asset_in.token.to_string(),
                });
            };
            let swap_msg = ExecuteSwapRequest {
                asset_in,
                amount_in,
                asset_out,
                min_amount_out,
                swaps,
                recipients,
                cross_chain_config,
                partner_fee,
            };
            let response = crate::contract::execute(
                deps,
                env,
                info,
                ExecuteMsg::ExecuteSwapRequest(swap_msg),
            )?;
            Ok(response)
        }
    }
}

pub fn receive_euclid_cw20(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    sender: CrossChainUser,
    amount: Uint128,
    euclid_msg: EuclidReceive,
) -> Result<Response, ContractError> {
    match from_json::<FactoryEuclidReceiveHook>(euclid_msg.msg.clone())? {
        FactoryEuclidReceiveHook::Swap {
            asset_in,
            asset_out,
            min_amount_out,
            swaps,
            recipients,
            cross_chain_config,
            partner_fee,
        } => {
            ensure!(
                info.sender.to_string() == asset_in.token_type.get_smart_contract_address()?,
                ContractError::Unauthorized {}
            );
            let response = execute_swap_request(
                &mut deps,
                env,
                info,
                sender,
                asset_in,
                amount,
                asset_out,
                min_amount_out,
                swaps,
                recipients,
                cross_chain_config,
                partner_fee,
            )?;
            Ok(response)
        }
    }
}
