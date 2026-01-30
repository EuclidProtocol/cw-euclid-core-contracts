use cosmwasm_schema::cw_serde;
use cosmwasm_std::{ensure, Uint128};

use crate::{
    chain::ChainUid, cross_chain_user::CrossChainUser, error::ContractError, limit::Limit,
    token::TokenType,
};

#[cw_serde]
pub struct Recipient {
    pub recipient: CrossChainUser,
    pub amount: Limit,
    pub denom: TokenType,
    pub forwarding_message: Option<String>,
    pub unsafe_refund_as_voucher: Option<bool>,
}

impl Recipient {
    pub fn validate(&self) -> Result<(), ContractError> {
        self.amount.validate()?;
        self.recipient.validate()?;
        if !self.denom.is_voucher() {
            ensure!(
                self.recipient.chain_uid != ChainUid::vsl_chain_uid()?,
                ContractError::new(
                    "Recipient chain UID should not be VSL chain UID if denom is not a voucher"
                )
            );
        }
        Ok(())
    }

    pub fn default_voucher_recipient(user: CrossChainUser, limit: Limit) -> Self {
        Self {
            recipient: user,
            amount: limit,
            denom: TokenType::Voucher {},
            forwarding_message: None,
            unsafe_refund_as_voucher: None,
        }
    }
}
