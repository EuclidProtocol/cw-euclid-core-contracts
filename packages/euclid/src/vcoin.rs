use cosmwasm_schema::cw_serde;

type ChainId = String;
type AnyChainAddress = String;
type TokenId = String;
// Balance is stored again Chain Id, Address of the user on any chain, and for a specific Token Id
pub type SerializedBalanceKey = (ChainId, AnyChainAddress, TokenId);

#[cw_serde]
pub struct BalanceKey {
    pub chain_id: ChainId,
    pub address: AnyChainAddress,
    pub token_id: TokenId,
}

impl BalanceKey {
    pub fn to_serialized_balance_key(self) -> SerializedBalanceKey {
        (self.chain_id, self.address, self.token_id)
    }

    pub fn from_serialized_balance_key(balance_key: SerializedBalanceKey) -> Self {
        Self {
            chain_id: balance_key.0,
            address: balance_key.1,
            token_id: balance_key.2,
        }
    }
}
