use std::fmt;
use std::ops::Deref;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    coin, ensure, forward_ref_partial_eq, to_json_binary, Addr, BankMsg, Coin, CosmosMsg, StdError,
    StdResult, Uint128, WasmMsg,
};
use cw_storage_plus::{Key, KeyDeserialize, Prefixer, PrimaryKey};

use crate::chain::CrossChainUser;
use crate::cw20::Cw20HookMsg;
use crate::msgs::vcoin::ExecuteTransfer;
use crate::{error::ContractError, pool::Pool};

// Token asset that represents an identifier for a token
#[cw_serde]
pub struct Token(String);
forward_ref_partial_eq!(Token, Token);

// Implement Deref to allow easy access to the inner type
impl Deref for Token {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Token {
    fn new(id: String) -> Self {
        Self(id)
    }

    pub fn create(id: String) -> Result<Self, ContractError> {
        let token = Self::new(id);
        token.validate()?;
        Ok(token)
    }

    pub fn exists(&self, pair: Pair) -> bool {
        self == pair.token_1 || self == pair.token_2
    }
    pub fn validate(&self) -> Result<&Self, ContractError> {
        ensure!(!self.is_empty(), ContractError::InvalidTokenID {});

        for c in self.0.chars() {
            if !c.is_ascii_alphanumeric() && c != '.' {
                return Err(ContractError::new(
                    "Invalid Token Id format: must be lowercase, alphanumeric or '.'",
                ));
            }
        }
        Ok(self)
    }

    pub fn create_vcoin_transfer_msg(
        &self,
        vcoin_address: String,
        amount: Uint128,
        from: CrossChainUser,
        to: CrossChainUser,
    ) -> Result<WasmMsg, ContractError> {
        let transfer_msg = crate::msgs::vcoin::ExecuteMsg::Transfer(ExecuteTransfer {
            amount,
            token_id: self.0.clone(),
            from,
            to,
        });

        let transfer_msg = WasmMsg::Execute {
            contract_addr: vcoin_address,
            msg: to_json_binary(&transfer_msg)?,
            funds: vec![],
        };
        Ok(transfer_msg)
    }
}

impl<'a> PrimaryKey<'a> for Token {
    type Prefix = ();
    type SubPrefix = ();

    type Suffix = Self;
    type SuperSuffix = Self;

    fn key(&self) -> Vec<Key> {
        vec![Key::Ref(self.as_bytes())]
    }
}

impl<'a> Prefixer<'a> for Token {
    fn prefix(&self) -> Vec<Key> {
        vec![Key::Ref(self.as_bytes())]
    }
}

impl KeyDeserialize for Token {
    type Output = Token;

    #[inline(always)]
    fn from_vec(value: Vec<u8>) -> StdResult<Self::Output> {
        String::from_utf8(value)
            .map(Token)
            .map_err(|e| StdError::generic_err(format!("Invalid UTF-8 sequence: {}", e)))
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cw_serde]
pub struct Pair {
    pub token_1: Token,
    pub token_2: Token,
}
forward_ref_partial_eq!(Pair, Pair);

impl Pair {
    pub fn new(token_1: Token, token_2: Token) -> Result<Self, ContractError> {
        let pair = if token_1.le(&token_2) {
            Self { token_1, token_2 }
        } else {
            Self {
                token_1: token_2,
                token_2: token_1,
            }
        };
        pair.validate()?;
        Ok(pair)
    }
    pub fn validate(&self) -> Result<(), ContractError> {
        // Prevent duplicate tokens
        ensure!(
            self.token_1 != self.token_2,
            ContractError::DuplicateTokens {}
        );
        self.token_1.validate()?;
        self.token_2.validate()?;

        ensure!(
            self.token_1.le(&self.token_2),
            ContractError::new("Token order is wrong")
        );
        Ok(())
    }
    pub fn get_other_token(&self, token: Token) -> Token {
        if token == self.token_1 {
            self.token_2.clone()
        } else {
            self.token_1.clone()
        }
    }

    pub fn get_tupple(&self) -> (Token, Token) {
        if self.token_1.le(&self.token_2) {
            (self.token_1.clone(), self.token_2.clone())
        } else {
            (self.token_2.clone(), self.token_1.clone())
        }
    }

    pub fn get_pool(&self, reserve_1: Uint128, reserve_2: Uint128) -> Pool {
        Pool {
            pair: self.clone(),
            reserve_1,
            reserve_2,
        }
    }

    pub fn get_vec_token(&self) -> Vec<Token> {
        let tokens: Vec<Token> = vec![self.token_1.clone(), self.token_2.clone()];
        tokens
    }
}

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
pub enum TokenType {
    Native { denom: String },
    Smart { contract_address: String },
}

// Helper to Check if Token is Native or Smart
impl TokenType {
    pub fn is_native(&self) -> bool {
        match self {
            TokenType::Native { .. } => true,
            TokenType::Smart { .. } => false,
        }
    }

    pub fn is_smart(&self) -> bool {
        !self.is_native()
    }

    // Helper to get the denom of a native or CW20 token
    pub fn get_denom(&self) -> String {
        match self.clone() {
            TokenType::Native { denom } => denom.to_string(),
            TokenType::Smart { contract_address } => contract_address.to_string(),
        }
    }

    pub fn get_key(&self) -> String {
        match self.clone() {
            TokenType::Native { denom } => format!("native:{denom}"),
            TokenType::Smart { contract_address } => format!("smart:{contract_address}"),
        }
    }

    // Create Cosmos Msg depending on type of token
    pub fn create_transfer_msg(
        &self,
        amount: Uint128,
        recipient: String,
        allowance: Option<String>,
    ) -> Result<CosmosMsg, ContractError> {
        let msg = match self.clone() {
            TokenType::Native { denom } => CosmosMsg::Bank(BankMsg::Send {
                to_address: recipient,
                amount: vec![Coin {
                    denom: denom.to_string(),
                    amount,
                }],
            }),
            TokenType::Smart { contract_address } => CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: contract_address.to_string(),
                msg: match allowance {
                    Some(owner) => to_json_binary(&cw20_base::msg::ExecuteMsg::TransferFrom {
                        owner,
                        recipient,
                        amount,
                    })?,
                    None => {
                        to_json_binary(&cw20_base::msg::ExecuteMsg::Transfer { recipient, amount })?
                    }
                },
                funds: vec![],
            }),
        };
        Ok(msg)
    }

    pub fn create_escrow_msg(
        &self,
        amount: Uint128,
        escrow_contract: Addr,
    ) -> Result<CosmosMsg, ContractError> {
        let msg: CosmosMsg = match self {
            Self::Native { denom } => CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: escrow_contract.into_string(),
                msg: to_json_binary(&crate::msgs::escrow::ExecuteMsg::DepositNative {})?,
                funds: vec![coin(amount.u128(), denom)],
            }),
            Self::Smart { contract_address } => CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: contract_address.clone(),
                msg: to_json_binary(&cw20_base::msg::ExecuteMsg::Send {
                    contract: escrow_contract.to_string(),
                    amount,
                    msg: to_json_binary(&Cw20HookMsg::Deposit {})?,
                })?,
                funds: vec![],
            }),
        };
        Ok(msg)
    }
}

#[cw_serde]
pub struct TokenWithDenom {
    pub token: Token,
    pub token_type: TokenType,
}

impl TokenWithDenom {
    pub fn get_denom(&self) -> String {
        self.token_type.get_denom()
    }

    pub fn create_transfer_msg(
        &self,
        amount: Uint128,
        recipient: String,
        allowance: Option<String>,
    ) -> Result<CosmosMsg, ContractError> {
        self.token_type
            .create_transfer_msg(amount, recipient, allowance)
    }

    pub fn create_escrow_msg(
        &self,
        amount: Uint128,
        escrow_contract: Addr,
    ) -> Result<CosmosMsg, ContractError> {
        self.token_type.create_escrow_msg(amount, escrow_contract)
    }
}

#[cw_serde]
pub struct PairWithDenom {
    pub token_1: TokenWithDenom,
    pub token_2: TokenWithDenom,
}

impl PairWithDenom {
    pub fn get_pair(&self) -> Result<Pair, ContractError> {
        Pair::new(self.token_1.token.clone(), self.token_2.token.clone())
    }

    pub fn get_vec_token_info(&self) -> Vec<TokenWithDenom> {
        let tokens: Vec<TokenWithDenom> = vec![self.token_1.clone(), self.token_2.clone()];
        tokens
    }
}

#[cfg(test)]
use cosmwasm_std::testing::mock_dependencies;

#[cfg(test)]
mod tests {
    use super::*;

    struct TestToken {
        name: &'static str,
        token: Token,
        expected_error: Option<ContractError>,
    }

    struct TestTokenPair {
        name: &'static str,
        pair: Pair,
        expected_error: Option<ContractError>,
    }

    #[test]
    fn test_tuple_key_serialize_deserialzie() {
        let mut owned_deps = mock_dependencies();
        let deps = owned_deps.as_mut();
        pub const PAIR_MAP: cw_storage_plus::Map<Pair, String> = cw_storage_plus::Map::new("pair");

        let token_1 = Token("token_1123".to_string());
        let token_2 = Token("token_2".to_string());
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

    #[test]
    fn test_token_validation() {
        let test_cases = vec![
            TestToken {
                name: "Empty token ID",
                token: Token("".to_string()),
                expected_error: Some(ContractError::InvalidTokenID {}),
            },
            TestToken {
                name: "Non-empty token ID",
                token: Token("NotEmpty".to_string()),
                expected_error: None,
            },
        ];

        for test in test_cases {
            let res = test.token.validate();

            if let Some(err) = test.expected_error {
                assert_eq!(res.unwrap_err(), err, "{}", test.name);
                continue;
            } else {
                assert!(res.is_ok())
            }
        }
    }

    #[test]
    fn test_pair_validation() {
        let test_cases = vec![
            TestTokenPair {
                name: "Duplicate tokens",
                pair: Pair {
                    token_1: Token("ABC".to_string()),
                    token_2: Token("ABC".to_string()),
                },
                expected_error: Some(ContractError::DuplicateTokens {}),
            },
            TestTokenPair {
                name: "Different tokens",
                pair: Pair {
                    token_1: Token("ABC".to_string()),
                    token_2: Token("DEF".to_string()),
                },
                expected_error: None,
            },
            TestTokenPair {
                name: "Same letters but with different case",
                pair: Pair {
                    token_1: Token("ABC".to_string()),
                    token_2: Token("AbC".to_string()),
                },
                expected_error: None,
            },
            TestTokenPair {
                name: "One invalid token",
                pair: Pair {
                    token_1: Token("ABC".to_string()),
                    token_2: Token("".to_string()),
                },
                expected_error: Some(ContractError::InvalidTokenID {}),
            },
        ];

        for test in test_cases {
            let res = test.pair.validate();

            if let Some(err) = test.expected_error {
                assert_eq!(res.unwrap_err(), err, "{}", test.name);
                continue;
            } else {
                assert!(res.is_ok())
            }
        }
    }
}
