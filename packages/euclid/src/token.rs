use std::fmt;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{to_json_binary, BankMsg, Coin, CosmosMsg, StdError, StdResult, Uint128, WasmMsg};
use cw_storage_plus::{Key, KeyDeserialize, Prefixer, PrimaryKey};

use crate::cw20::Cw20ExecuteMsg;


// Token asset that represents an identifier for a token
#[cw_serde]
#[derive(Hash,Eq)]
pub struct Token {
    pub id: String,   
}

impl Token { 
    pub fn exists(&self, pool: Pair) -> bool {
        if self == &pool.token_1 || self == &pool.token_2 {
            return true
        }
        else {
            return false
        }
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
pub struct Pair {
    pub token_1: Token,
    pub token_2: Token,
}

// TokenInfo stores the native or smart contract token information from incoming chain
#[cw_serde]
pub enum TokenInfo {
    Native {
        denom: String,
        token: Token,
    },
    Smart {
        contract_address: String,
        token: Token,
    },
    
}

// Helper to Check if Token is Native or Smart
impl TokenInfo {
    pub fn is_native(&self) -> bool {
        match self {
            TokenInfo::Native { .. } => true,
            TokenInfo::Smart { .. } => false,
        }
    }

    pub fn is_smart(&self) -> bool {
        !self.is_native()
    }

    // Helper to get the denom of a native token
    pub fn get_denom(&self) -> String {
        match self {
            TokenInfo::Native { denom, token: _ } => denom.to_string(),
            TokenInfo::Smart { .. } => panic!("This is not a native token"),
        }
    }

    // Helper to get the contract address of a smart token
    pub fn get_contract_address(&self) -> String {
        match self {
            TokenInfo::Smart { contract_address, token: _  } => contract_address.to_string(),
            TokenInfo::Native { .. } => panic!("This is not a smart token"),
        }
    }

    // Check if asset exists in a certain pair
    pub fn exists(&self, pair_info: PairInfo) -> bool {
        if self == &pair_info.token_1 || self == &pair_info.token_2 {
            return true
        } else {
            return false
        }
    }

    // Get Token Identifier from TokenInfo
    pub fn get_token(&self) -> Token {
        match self {
            TokenInfo::Native { token, .. } => token.clone(),
            TokenInfo::Smart { token, .. } => token.clone(),
        }
    }

    // Create Cosmos Msg depending on type of token
    pub fn create_transfer_msg(&self, amount: Uint128, recipient: String) -> CosmosMsg {
        match self {
            TokenInfo::Native { denom, .. } => CosmosMsg::Bank(BankMsg::Send {
                to_address: recipient,
                amount: vec![Coin {
                    denom: denom.to_string(),
                    amount: amount,
                }],
            }),
            TokenInfo::Smart { contract_address, .. } => CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: contract_address.to_string(),
                msg: to_json_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: recipient,
                    amount: amount,
                }).unwrap(),
                funds: vec![],
            }),
        }
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
            return self.token_2.clone()
        } else {
            return self.token_1.clone()
        }
    }

 
}