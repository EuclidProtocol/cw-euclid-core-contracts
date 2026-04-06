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
    // Reject mixed-case or empty addresses from IBC packet data
    msg.get_sender().validate()?;

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

#[cfg(test)]
mod tests {

    use euclid::{
        chain::ChainUid,
        cross_chain_user::CrossChainUser,
        error::ContractError,
        token::{Token, TokenType, TokenWithDenom},
    };
    use euclid_ibc::router_ibc::RouterCrossChainExecuteMsg;

    use crate::{
        state::{LOCKED_CHAINS, STATE},
        testing::{
            fixtures::initialized,
            helpers::{call_reusable, register_denom_msg, MockDeps},
        },
    };

    use rstest::*;

    #[rstest]
    fn test_receive_dispatch_contract_locked(mut initialized: MockDeps) {
        let mut state = STATE.load(initialized.as_ref().storage).unwrap();
        state.locked = true;
        STATE.save(initialized.as_mut().storage, &state).unwrap();

        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let result = call_reusable(
            &mut initialized,
            register_denom_msg(chain_uid.clone()),
            chain_uid,
        );
        assert_eq!(result.unwrap_err(), ContractError::ContractLocked {});
    }

    #[rstest]
    fn test_receive_dispatch_chain_locked(mut initialized: MockDeps) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        LOCKED_CHAINS
            .save(initialized.as_mut().storage, &vec![chain_uid.clone()])
            .unwrap();

        let result = call_reusable(
            &mut initialized,
            register_denom_msg(chain_uid.clone()),
            chain_uid,
        );
        assert_eq!(result.unwrap_err(), ContractError::DeregisteredChain {});
    }

    #[rstest]
    fn test_receive_dispatch_chain_uid_mismatch(mut initialized: MockDeps) {
        let chain1 = ChainUid::create("chain1".to_string()).unwrap();
        let chain2 = ChainUid::create("chain2".to_string()).unwrap();
        // sender is from chain2 but chain_uid param is chain1
        let msg = RouterCrossChainExecuteMsg::RegisterDenom {
            sender: CrossChainUser::new(chain2, "user".to_string()),
            token: TokenWithDenom {
                token: Token::create("usdc".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uusdc".to_string(),
                },
            },
            tx_id: "tx1".to_string(),
        };
        let result = call_reusable(&mut initialized, msg, chain1);
        assert_eq!(
            result.unwrap_err(),
            ContractError::new("Chain UID mismatch")
        );
    }
}
