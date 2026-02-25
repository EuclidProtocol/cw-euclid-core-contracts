use cosmwasm_std::{ensure, DepsMut, Env, MessageInfo, Response};
use euclid::{chain::ChainUid, error::ContractError};

use euclid_ibc::router_ibc::RouterCrossChainExecuteMsg;

use crate::{
    ibc::receive::{
        pool::{
            ibc_execute_add_liquidity, ibc_execute_remove_liquidity,
            ibc_execute_request_pool_creation,
        },
        swap::ibc_execute_swap,
        token::{
            ibc_execute_deposit_token, ibc_execute_deregister_denom, ibc_execute_register_denom,
            ibc_execute_transfer_virtual_balance,
        },
    },
    state::{LOCKED_CHAINS, STATE},
};

pub fn reusable_internal_call(
    deps: &mut DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: RouterCrossChainExecuteMsg,
    chain_uid: ChainUid,
) -> Result<Response, ContractError> {
    let locked = STATE.load(deps.storage)?.locked;
    ensure!(!locked, ContractError::ContractLocked {});

    let deregistered_chains = LOCKED_CHAINS.may_load(deps.storage)?.unwrap_or_default();
    ensure!(
        !deregistered_chains.contains(&chain_uid),
        ContractError::DeregisteredChain {}
    );
    let tx_id = msg.get_tx_id();

    let mut response = match msg {
        RouterCrossChainExecuteMsg::RegisterDenom {
            token,
            sender,
            tx_id,
        } => {
            ensure!(
                sender.chain_uid == chain_uid,
                ContractError::new("Chain UID mismatch")
            );
            ibc_execute_register_denom(deps.branch(), env, sender, token, tx_id)?
        }
        RouterCrossChainExecuteMsg::DeregisterDenom {
            token,
            sender,
            tx_id,
        } => {
            ensure!(
                sender.chain_uid == chain_uid,
                ContractError::new("Chain UID mismatch")
            );
            ibc_execute_deregister_denom(deps.branch(), env, sender, token, tx_id)?
        }
        RouterCrossChainExecuteMsg::TransferVoucher(msg) => {
            ensure!(
                msg.sender.chain_uid == chain_uid,
                ContractError::new("Chain UID mismatch")
            );
            ibc_execute_transfer_virtual_balance(deps, env, msg)?
        }
        RouterCrossChainExecuteMsg::DepositToken(msg) => {
            ensure!(
                msg.sender.chain_uid == chain_uid,
                ContractError::new("Chain UID mismatch")
            );

            ibc_execute_deposit_token(deps, env, msg)?
        }
        RouterCrossChainExecuteMsg::RequestPoolCreation {
            pair,
            sender,
            tx_id,
            slippage_tolerance_bps,
            pool_config,
        } => {
            ensure!(
                sender.chain_uid == chain_uid,
                ContractError::new("Chain UID mismatch")
            );
            ibc_execute_request_pool_creation(
                deps.branch(),
                env,
                sender,
                pair,
                pool_config,
                tx_id,
                slippage_tolerance_bps,
            )?
        }

        RouterCrossChainExecuteMsg::AddLiquidity {
            slippage_tolerance_bps,
            pair,
            tx_id,
            sender,
            ..
        } => {
            ensure!(
                sender.chain_uid == chain_uid,
                ContractError::new("Chain UID mismatch")
            );
            ibc_execute_add_liquidity(deps.branch(), sender, pair, slippage_tolerance_bps, tx_id)?
        }
        RouterCrossChainExecuteMsg::RemoveLiquidity(msg) => {
            ensure!(
                msg.sender.chain_uid == chain_uid,
                ContractError::new("Chain UID mismatch")
            );
            ibc_execute_remove_liquidity(deps.branch(), env, msg)?
        }
        RouterCrossChainExecuteMsg::Swap(msg) => {
            ensure!(
                msg.sender.chain_uid == chain_uid,
                ContractError::new("Chain UID mismatch")
            );
            ibc_execute_swap(deps.branch(), env, msg)?
        }
    };
    response = response.add_attribute("tx_id", tx_id);

    Ok(response)
}
