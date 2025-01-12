use crate::{
    chain::{ChainUid, CrossChainUser},
    error::ContractError,
};
use cosmwasm_schema::cw_serde;
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
