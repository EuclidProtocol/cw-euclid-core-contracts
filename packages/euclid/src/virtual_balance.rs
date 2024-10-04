use cosmwasm_schema::cw_serde;
use cosmwasm_std::{to_json_binary, DepsMut, Env, Response, Uint128, WasmMsg};

use crate::{
    chain::{ChainUid, CrossChainUser},
    error::ContractError,
    msgs::virtual_balance::ExecuteTransfer,
    token::Token,
};

type AnyChainAddress = String;
type TokenId = String;
// Balance is stored again Chain Id, Address of the user on any chain, and for a specific Token Id
pub type SerializedBalanceKey = (ChainUid, AnyChainAddress, TokenId);

#[cw_serde]
pub struct BalanceKey {
    pub cross_chain_user: CrossChainUser,
    pub token_id: TokenId,
}

impl BalanceKey {
    pub fn to_serialized_balance_key(self) -> SerializedBalanceKey {
        (
            self.cross_chain_user.chain_uid,
            self.cross_chain_user.address,
            self.token_id,
        )
    }

    pub fn from_serialized_balance_key(
        balance_key: SerializedBalanceKey,
    ) -> Result<Self, ContractError> {
        Ok(Self {
            cross_chain_user: CrossChainUser {
                chain_uid: balance_key.0.validate()?.clone(),
                address: balance_key.1,
            },
            token_id: balance_key.2,
        })
    }
}

pub fn transfer_virtual_balance(
    sender: CrossChainUser,
    token: Token,
    amount: Uint128,
    recipient_address: CrossChainUser,
    virtual_balance_address: String,
) -> Result<Response, ContractError> {
    let transfer_voucher_msg =
        crate::msgs::virtual_balance::ExecuteMsg::Transfer(ExecuteTransfer {
            amount,
            token_id: token.to_string(),
            from: sender,
            to: CrossChainUser {
                address: recipient_address.address.clone(),
                chain_uid: recipient_address.chain_uid,
            },
        });

    let transfer_voucher_msg = WasmMsg::Execute {
        contract_addr: virtual_balance_address,
        msg: to_json_binary(&transfer_voucher_msg)?,
        funds: vec![],
    };

    Ok(Response::new()
        .add_message(transfer_voucher_msg)
        .add_attribute("action", "transfer_virtual_balance"))
}
