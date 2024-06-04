use std::fmt;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    forward_ref_partial_eq, to_json_binary, Addr, BankMsg, Coin, CosmosMsg, StdError, StdResult,
    Uint128, WasmMsg,
};
use cw_storage_plus::{Key, KeyDeserialize, Prefixer, PrimaryKey};

use crate::{cw20::Cw20ExecuteMsg, error::ContractError};

// Token asset that represents an identifier for a token
#[cw_serde]
#[derive(Eq, PartialOrd, Ord)]
pub struct Token {
    pub id: String,
}

forward_ref_partial_eq!(Token, Token);

impl Token {
    pub fn exists(&self, pool: Pair) -> bool {
        self == pool.token_1 || self == pool.token_2
    }
}

impl<'a> PrimaryKey<'a> for Token {
    type Prefix = ();
    type SubPrefix = ();

    type Suffix = Self;
    type SuperSuffix = Self;

    fn key(&self) -> Vec<Key> {
        vec![Key::Ref(self.id.as_bytes())]
    }
}

impl<'a> Prefixer<'a> for Token {
    fn prefix(&self) -> Vec<Key> {
        vec![Key::Ref(self.id.as_bytes())]
    }
}

impl KeyDeserialize for Token {
    type Output = Token;

    #[inline(always)]
    fn from_vec(value: Vec<u8>) -> StdResult<Self::Output> {
        String::from_utf8(value)
            .map(|id| Token { id })
            .map_err(|e| StdError::generic_err(format!("Invalid UTF-8 sequence: {}", e)))
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.id)
    }
}

// A pair is a set of two tokens
#[cw_serde]
#[derive(Eq, PartialOrd, Ord)]
pub struct Pair {
    pub token_1: Token,
    pub token_2: Token,
}

forward_ref_partial_eq!(Pair, Pair);

impl<'a> PrimaryKey<'a> for Pair {
    type Prefix = Token;
    type SubPrefix = ();

    type Suffix = Token;
    type SuperSuffix = Self;

    fn key(&self) -> Vec<Key> {
        let token_1_key_size = self.token_1.joined_key().len();
        assert!(
            token_1_key_size <= u16::MAX as usize,
            "Key size exceeds u8 limit"
        );
        let mut res = vec![];
        res.push(Key::Val16((token_1_key_size as u16).to_be_bytes()));
        res.extend(self.token_1.key());
        res.extend(self.token_2.key());
        res
    }
}

fn parse_length(value: &[u8]) -> StdResult<usize> {
    Ok(u16::from_be_bytes(
        value
            .try_into()
            .map_err(|_| StdError::generic_err("Could not read 2 byte length"))?,
    )
    .into())
}

impl KeyDeserialize for Pair {
    type Output = Pair;

    #[inline(always)]
    fn from_vec(mut value: Vec<u8>) -> StdResult<Self::Output> {
        println!("Bytes - {value:?}");
        let mut values = value.split_off(2);
        let size_bytes_len = parse_length(&value)?;
        println!("Size of bytes - {size_bytes_len:?}");
        let mut token_1_key_bytes = values.split_off(size_bytes_len);

        // Deserialize token_1
        let token_1_key_len = parse_length(&values)?;
        let token_2_key_bytes = token_1_key_bytes.split_off(token_1_key_len + 2);
        let token_1 = Token::from_vec(token_1_key_bytes[2..].to_vec())?;

        // Deserialize token_2
        let token_2 = Token::from_vec(token_2_key_bytes.to_vec())?;

        Ok(Pair { token_1, token_2 })
    }
}

#[cw_serde]
pub struct TokenInfo {
    token: Token,
    token_type: TokenType,
}
// TokenInfo stores the native or smart contract token information from incoming chain
#[cw_serde]
pub enum TokenType {
    Native { denom: String },
    Smart { contract_address: String },
}

// Helper to Check if Token is Native or Smart
impl TokenInfo {
    pub fn is_native(&self) -> bool {
        match self.token_type {
            TokenType::Native { .. } => true,
            TokenType::Smart { .. } => false,
        }
    }

    pub fn is_smart(&self) -> bool {
        !self.is_native()
    }

    // Helper to get the denom of a native token
    pub fn get_denom(&self) -> String {
        match self.token_type.clone() {
            TokenType::Native { denom } => denom.to_string(),
            TokenType::Smart { contract_address } => contract_address.to_string(),
        }
    }

    // Check if asset exists in a certain pair
    pub fn exists(&self, pair_info: PairInfo) -> bool {
        self == &pair_info.token_1 || self == &pair_info.token_2
    }

    // Get Token Identifier from TokenInfo
    pub fn get_token(&self) -> Token {
        self.token.clone()
    }

    // Create Cosmos Msg depending on type of token
    pub fn create_transfer_msg(
        &self,
        amount: Uint128,
        recipient: String,
    ) -> Result<CosmosMsg, ContractError> {
        let msg = match self.token_type.clone() {
            TokenType::Native { denom } => CosmosMsg::Bank(BankMsg::Send {
                to_address: recipient,
                amount: vec![Coin {
                    denom: denom.to_string(),
                    amount,
                }],
            }),
            TokenType::Smart { contract_address } => CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: contract_address.to_string(),
                msg: to_json_binary(&Cw20ExecuteMsg::Transfer { recipient, amount })?,
                funds: vec![],
            }),
        };
        Ok(msg)
    }
}

// PairInfo stores the pair information of two tokens
#[cw_serde]
pub struct PairInfo {
    pub token_1: TokenInfo,
    pub token_2: TokenInfo,
}

impl PairInfo {
    // Helper function to get the token that is not the current token
    pub fn get_other_token(&self, token: TokenInfo) -> TokenInfo {
        if token == self.token_1 {
            self.token_2.clone()
        } else {
            self.token_1.clone()
        }
    }

    // Helper function to get the token that is not the current token
    pub fn get_pair(&self) -> Pair {
        Pair {
            token_1: self.token_1.token.clone(),
            token_2: self.token_2.token.clone(),
        }
    }
}

// Struct to handle Acknowledgement Response for a Pool Creation Request
#[cw_serde]
pub struct PairRouter {
    pub vlp_contract: Addr,
    pub pair: Pair,
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::testing::mock_dependencies;

    use super::*;

    #[test]
    fn test_tuple_key_serialize_deserialzie() {
        let mut owned_deps = mock_dependencies();
        let deps = owned_deps.as_mut();
        pub const PAIR_MAP: cw_storage_plus::Map<Pair, String> = cw_storage_plus::Map::new("pair");

        let token_1 = Token {
            id: "token_1123".to_string(),
        };
        let token_2 = Token {
            id: "token_2".to_string(),
        };

        let pair = Pair { token_1, token_2 };

        let vlp = "vlp_address".to_string();
        PAIR_MAP.save(deps.storage, pair.clone(), &vlp).unwrap();

        assert_eq!(PAIR_MAP.load(deps.storage, pair.clone()).unwrap(), vlp);

        let list = PAIR_MAP
            .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(list[0], (pair, vlp));
    }
}
