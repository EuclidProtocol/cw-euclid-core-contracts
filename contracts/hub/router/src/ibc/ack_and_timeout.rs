use cosmwasm_std::to_json_binary;
use cosmwasm_std::{from_json, Binary, CosmosMsg, DepsMut, Env, Response, Uint256, WasmMsg};
use euclid::chain::{Chain, ChainType, ChainUid};
use euclid::cross_chain_user::CrossChainUser;
use euclid::error::ContractError;
use euclid::events::{tx_event, TxType};
use euclid::msgs::factory::{RegisterFactoryResponse, ReleaseEscrowResponse};
use euclid::msgs::virtual_balance::msg::{ExecuteMint, ExecuteMsg as VirtualBalanceExecuteMsg};
use euclid::token::Token;
use euclid::voucher::BalanceKey;
use euclid_ibc::ack::AcknowledgementMsg;
use euclid_ibc::factory_ibc::FactoryCrossChainExecuteMsg;

use euclid::token::TokenType;

use crate::state::{
    CHAIN_UID_TO_CHAIN, FEE_STATE, PENDING_RELEASE_VOUCHER, VIRTUAL_BALANCE_CONTRACT,
};

pub fn reusable_internal_ack_call(
    deps: DepsMut,
    env: Env,
    chain_uid: ChainUid,
    msg: FactoryCrossChainExecuteMsg,
    ack: Binary,
    chain_type: euclid::chain::ChainType,
) -> Result<Response, ContractError> {
    let tx_id = msg.get_tx_id();

    let response = match msg {
        FactoryCrossChainExecuteMsg::RegisterFactory {
            chain_uid, tx_id, ..
        } => {
            let res = from_json(ack)?;
            ibc_ack_register_factory(deps, env, chain_uid, chain_type, res, tx_id)?
        }
        FactoryCrossChainExecuteMsg::ReleaseEscrow {
            sender,
            amount,
            token,
            tx_id,
            recipient,
            denom,
            ..
        } => {
            let res = from_json(ack)?;
            let recipient = CrossChainUser::new(chain_uid, recipient.to_string());
            ibc_ack_release_escrow(
                deps, env, sender, amount, token, denom, res, recipient, tx_id,
            )?
        }
    };
    let response = response.add_attribute("tx_id", tx_id);
    Ok(response)
}

pub fn ibc_ack_register_factory(
    deps: DepsMut,
    env: Env,
    chain_uid: ChainUid,
    chain_type: ChainType,
    res: AcknowledgementMsg<RegisterFactoryResponse>,
    tx_id: String,
) -> Result<Response, ContractError> {
    let response = Response::new().add_event(tx_event(
        &tx_id,
        env.contract.address.as_str(),
        TxType::RegisterFactory,
    ));
    match res {
        AcknowledgementMsg::Ok(data) => {
            let chain_data = Chain {
                chain_uid: chain_uid.clone(),
                factory_address: data.factory_address.clone(),
                chain_type: chain_type.clone(),
            };
            CHAIN_UID_TO_CHAIN.save(deps.storage, chain_uid.clone(), &chain_data)?;
            Ok(response
                .add_attribute("method", "register_factory_ack_success")
                .add_attribute("chain_uid", chain_uid.to_string())
                .add_attribute("factory_chain", data.chain_id)
                .add_attribute("factory_address", data.factory_address))
        }

        AcknowledgementMsg::Error(err) => {
            // If its a native then reject via error
            if matches!(chain_type, ChainType::Native {}) {
                return Err(ContractError::new(&err));
            }
            Ok(response
                .add_attribute("method", "register_factory_ack_error")
                .add_attribute("chain_uid", chain_uid.to_string())
                .add_attribute("error", err.clone()))
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn ibc_ack_release_escrow(
    deps: DepsMut,
    _env: Env,
    sender: CrossChainUser,
    amount: Uint256,
    token: Token,
    token_type: TokenType,
    res: AcknowledgementMsg<ReleaseEscrowResponse>,
    recipient: CrossChainUser,
    tx_id: String,
) -> Result<Response, ContractError> {
    let response = Response::new().add_event(tx_event(
        &tx_id,
        sender.address.as_str(),
        TxType::EscrowRelease,
    ));
    let pending_release_voucher = PENDING_RELEASE_VOUCHER.load(deps.storage, tx_id.clone())?;
    PENDING_RELEASE_VOUCHER.remove(deps.storage, tx_id);
    let virtual_balance_address = VIRTUAL_BALANCE_CONTRACT.load(deps.storage)?.to_string();
    match res {
        AcknowledgementMsg::Ok(data) => {
            let mut response = response
                .add_attribute("method", "release_escrow_success")
                .add_attribute("amount", amount.to_string())
                .add_attribute("recipient", data.to_address)
                .add_attribute("updated_escrow_balance", data.escrow_balance.to_string())
                .add_attribute("chain_uid", sender.chain_uid.to_string());

            if !pending_release_voucher.release_fee_amount.is_zero() {
                let fee_recipient = FEE_STATE.load(deps.storage)?.release_fee_recipient;
                let balance_key = BalanceKey {
                    cross_chain_user: CrossChainUser::new(
                        ChainUid::vsl_chain_uid()?,
                        fee_recipient.to_string(),
                    ),
                    token_id: token.to_string(),
                };

                // Mint release fee to fee recipient
                let mint_msg = VirtualBalanceExecuteMsg::Mint(ExecuteMint {
                    amount: pending_release_voucher.release_fee_amount.into(),
                    balance_key: balance_key.clone(),
                    token_type: token_type.clone(),
                    token_source_chain_uid: recipient.chain_uid.clone(),
                });
                let msg: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: virtual_balance_address.to_string(),
                    msg: to_json_binary(&mint_msg)?,
                    funds: vec![],
                });
                response = response.add_message(msg)
            };

            Ok(response)
        }
        // Re-mint tokens
        AcknowledgementMsg::Error(err) => {
            // Escrow release failed: re-mint vouchers (virtual_balance will re-increment escrow)
            let refund_recipient = if pending_release_voucher.unsafe_refund_voucher {
                recipient.clone()
            } else {
                sender.clone()
            };

            let balance_key = BalanceKey {
                cross_chain_user: refund_recipient.clone(),
                token_id: token.to_string(),
            };
            let mint_amount = amount.checked_add(pending_release_voucher.release_fee_amount)?;
            // Escrow release failed, mint tokens again for the original cross chain sender
            let mint_msg = VirtualBalanceExecuteMsg::Mint(ExecuteMint {
                amount: mint_amount.into(),
                balance_key: balance_key.clone(),
                token_type,
                token_source_chain_uid: recipient.chain_uid.clone(),
            });
            let msg: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: virtual_balance_address.to_string(),
                msg: to_json_binary(&mint_msg)?,
                funds: vec![],
            });

            // Even if its a native chain, we can't reject via Err because other escrow release will also be rejected
            Ok(response
                .add_message(msg)
                .add_attribute("method", "escrow_release_ack")
                .add_attribute("error", err)
                .add_attribute("mint_amount", amount.to_string())
                .add_attribute("balance_key", format!("{:?}", balance_key)))
        }
    }
}
