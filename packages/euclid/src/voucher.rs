use crate::{chain::ChainUid, cross_chain_user::CrossChainUser, error::ContractError};
use cosmwasm_schema::cw_serde;
type AnyChainAddress = String;
type TokenId = String;
// Balance is stored again Chain Id, Address of the user on any chain, and for a specific Token Id
// Why token denom is not included in the key?
// Vouchers are made denom independent as it can be used from a chain where that token doesn't even exist. For example, a user can manage BNB or ETH from a cosmos chain as a voucher.
// So, if denom is included in the key then it would be difficult to assign it to a chain that doesn't have that token (in our example, bnb on cosmoshub)
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
            cross_chain_user: CrossChainUser::new(
                balance_key.0.validate()?.clone(),
                balance_key.1.clone(),
            ),
            token_id: balance_key.2,
        })
    }
}
