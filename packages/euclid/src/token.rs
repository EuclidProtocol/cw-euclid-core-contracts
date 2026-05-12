use std::fmt;
use std::ops::Deref;

use crate::cw20_types::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, TokenInfoResponse};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    coin, ensure, to_json_binary, Addr, BankMsg, Binary, Coin, CosmosMsg, Deps, StdError,
    StdResult, Uint128, Uint256, WasmMsg,
};
use cw_storage_plus::{Key, KeyDeserialize, Prefixer, PrimaryKey};

use crate::chain::ChainUid;
use crate::cross_chain_user::CrossChainUser;
use crate::error::ContractError;
use crate::voucher::VOUCHER_DECIMAL;

// Token asset that represents an identifier for a token
#[cw_serde]
pub struct Token(String);
// forward_ref_partial_eq!(Token, Token);
impl PartialEq<Token> for &Token {
    fn eq(&self, other: &Token) -> bool {
        **self == *other
    }
}

impl PartialEq<&Token> for Token {
    fn eq(&self, other: &&Token) -> bool {
        *self == **other
    }
}

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
        ensure!(self.0.len() <= 64, ContractError::InvalidTokenID {});

        for c in self.0.chars() {
            if !c.is_ascii_alphanumeric() && c != '.' {
                return Err(ContractError::new(
                    "Invalid Token Id format: must be lowercase, alphanumeric or '.'",
                ));
            }
        }
        Ok(self)
    }

    pub fn create_voucher_transfer_msg(
        &self,
        virtual_balance_address: String,
        amount: Uint256,
        // Only router should be able to set the sender
        sender: Option<CrossChainUser>,
        to: CrossChainUser,
        // From will trigger allowance
        from: Option<CrossChainUser>,
        // Msg will trigger send variant of transfer
        msg: Option<Binary>,
    ) -> Result<WasmMsg, ContractError> {
        let transfer_msg = crate::msgs::virtual_balance::msg::ExecuteMsg::Transfer(
            crate::msgs::virtual_balance::msg::ExecuteTransfer {
                amount,
                sender,
                token_id: self.0.clone(),
                to,
                from,
                msg,
            },
        );

        let transfer_msg = WasmMsg::Execute {
            contract_addr: virtual_balance_address,
            msg: to_json_binary(&transfer_msg)?,
            funds: vec![],
        };
        Ok(transfer_msg)
    }

    pub fn with_amount(&self, amount: Uint256) -> TokenWithAmount {
        TokenWithAmount {
            token: self.clone(),
            amount,
        }
    }

    pub fn with_type(&self, token_type: TokenType) -> TokenWithDenom {
        TokenWithDenom {
            token: self.clone(),
            token_type,
        }
    }
}

impl PrimaryKey<'_> for Token {
    type Prefix = ();
    type SubPrefix = ();

    type Suffix = Self;
    type SuperSuffix = Self;

    fn key(&self) -> Vec<Key<'_>> {
        vec![Key::Ref(self.as_bytes())]
    }
}

impl Prefixer<'_> for Token {
    fn prefix(&self) -> Vec<Key<'_>> {
        vec![Key::Ref(self.as_bytes())]
    }
}

impl KeyDeserialize for Token {
    type Output = Token;
    const KEY_ELEMS: u16 = 42;

    #[inline(always)]
    fn from_vec(value: Vec<u8>) -> StdResult<Self::Output> {
        String::from_utf8(value)
            .map(Token)
            .map_err(|e| StdError::msg(format!("Invalid UTF-8 sequence: {}", e)))
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

// Compare Token == &str
impl PartialEq<&str> for Token {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

// Compare &str == Token
impl PartialEq<Token> for &str {
    fn eq(&self, other: &Token) -> bool {
        *self == other.0
    }
}

// Compare Token == String
impl PartialEq<String> for Token {
    fn eq(&self, other: &String) -> bool {
        &self.0 == other
    }
}

// Compare String == Token
impl PartialEq<Token> for String {
    fn eq(&self, other: &Token) -> bool {
        self == &other.0
    }
}

impl Pair {
    pub fn new(token_1: Token, token_2: Token) -> Result<Self, ContractError> {
        let pair = if token_1.le(&token_2.to_string()) {
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
            self.token_1.le(&self.token_2.to_string()),
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

    pub fn get_tupple(&self) -> (String, String) {
        if self.token_1.le(&self.token_2.to_string()) {
            (self.token_1.to_string(), self.token_2.to_string())
        } else {
            (self.token_2.to_string(), self.token_1.to_string())
        }
    }

    pub fn get_vec_token(&self) -> Vec<Token> {
        let tokens: Vec<Token> = vec![self.token_1.clone(), self.token_2.clone()];
        tokens
    }

    pub fn get_pair_with_amount(
        &self,
        reserve_1: Uint256,
        reserve_2: Uint256,
    ) -> Result<PairWithAmount, ContractError> {
        PairWithAmount::new(
            self.token_1.with_amount(reserve_1),
            self.token_2.with_amount(reserve_2),
        )
    }
}

impl PrimaryKey<'_> for Pair {
    type Prefix = Token;
    type SubPrefix = ();

    type Suffix = Token;
    type SuperSuffix = Self;

    fn key(&self) -> Vec<Key<'_>> {
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
            .map_err(|_| StdError::msg("Could not read 2 byte length"))?,
    )
    .into())
}

impl KeyDeserialize for Pair {
    type Output = Pair;
    const KEY_ELEMS: u16 = 42;

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
    Native {
        denom: String,
        decimals: Option<u32>,
    },
    Smart {
        contract_address: String,
        decimals: Option<u32>,
    },
    Voucher {},
}

// Helper to Check if Token is Native or Smart
impl TokenType {
    pub fn is_native(&self) -> bool {
        matches!(self, TokenType::Native { .. })
    }

    pub fn is_smart(&self) -> bool {
        matches!(self, TokenType::Smart { .. })
    }

    pub fn get_smart_contract_address(&self) -> Result<String, ContractError> {
        match self {
            TokenType::Smart {
                contract_address, ..
            } => Ok(contract_address.clone()),
            _ => Err(ContractError::new("Token is not smart")),
        }
    }

    pub fn get_decimals(&self) -> Result<u32, ContractError> {
        match self {
            TokenType::Smart { decimals, .. } => Ok(decimals.ok_or(ContractError::new(
                "Decimal field is required for smart tokens",
            ))?),
            TokenType::Native { decimals, .. } => Ok(decimals.ok_or(ContractError::new(
                "Decimal field is required for native tokens",
            ))?),
            TokenType::Voucher { .. } => Ok(VOUCHER_DECIMAL),
        }
    }

    pub fn is_voucher(&self) -> bool {
        matches!(self, TokenType::Voucher { .. })
    }

    pub fn query_decimals(&self, deps: &Deps) -> Result<u32, ContractError> {
        match self {
            TokenType::Native { .. } => {
                // let denom_metadata = deps.querier.query_denom_metadata(denom.clone())?;
                // let matched_unit = denom_metadata
                //     .denom_units
                //     .iter()
                //     .find(|unit| unit.denom == denom.clone())
                //     .ok_or(ContractError::new("Denom metadata not found"))?;
                // Ok(matched_unit.exponent)
                Err(ContractError::new("Native tokens are not supported"))
            }
            TokenType::Smart {
                contract_address, ..
            } => {
                let token_info: TokenInfoResponse = deps.querier.query_wasm_smart(
                    contract_address.clone(),
                    &Cw20QueryMsg::TokenInfo {},
                )?;
                Ok(token_info.decimals.into())
            }
            TokenType::Voucher { .. } => Ok(VOUCHER_DECIMAL),
        }
    }

    /// Validates smart contract addresses, checks against empty denom and zero supply
    pub fn validate(&self, deps: &Deps) -> Result<(), ContractError> {
        if let Self::Native { denom, .. } = &self {
            let potential_supply = deps.querier.query_supply(denom.clone())?;
            let non_zero_supply = !potential_supply.amount.is_zero();
            ensure!(
                non_zero_supply,
                ContractError::ZeroAssetSupply {
                    asset: denom.clone()
                }
            );
        } else if let Self::Smart {
            contract_address, ..
        } = &self
        {
            let contract = deps
                .querier
                .query_wasm_contract_info(contract_address.clone());
            ensure!(
                contract.is_ok(),
                ContractError::InvalidAsset {
                    asset: contract_address.clone()
                }
            );
        }

        // Vouchers will be validated in VSL
        Ok(())
    }

    pub fn get_balance(&self, deps: Deps, address: String) -> Result<Uint256, ContractError> {
        match self.clone() {
            TokenType::Native { denom, .. } => {
                let balance = deps.querier.query_balance(address, denom)?;
                Ok(Uint256::from(balance.amount))
            }
            TokenType::Smart {
                contract_address, ..
            } => {
                let balance_msg = Cw20QueryMsg::Balance {
                    address: address.clone(),
                };
                let balance: BalanceResponse = deps
                    .querier
                    .query_wasm_smart(contract_address, &balance_msg)?;
                Ok(balance.balance.into())
            }
            TokenType::Voucher { .. } => Err(ContractError::new(
                "Cannot get balance of voucher using this function",
            )),
        }
    }

    pub fn get_key(&self) -> String {
        match self.clone() {
            TokenType::Native { denom, .. } => format!("native:{denom}"),
            TokenType::Smart {
                contract_address, ..
            } => format!("smart:{contract_address}"),
            TokenType::Voucher { .. } => "voucher".to_string(),
        }
    }

    pub fn get_type(&self) -> &str {
        match self {
            TokenType::Native { .. } => "native",
            TokenType::Smart { .. } => "smart",
            TokenType::Voucher { .. } => "voucher",
        }
    }

    pub fn is_same_type(&self, other: &TokenType) -> bool {
        self.get_type() == other.get_type()
    }

    pub fn from_key(key: String) -> Result<Self, ContractError> {
        match key.split(":").collect::<Vec<&str>>().as_slice() {
            ["native", denom] => Ok(TokenType::Native {
                denom: denom.to_string(),
                decimals: None,
            }),
            ["smart", contract_address] => Ok(TokenType::Smart {
                contract_address: contract_address.to_string(),
                decimals: None,
            }),
            ["voucher"] => Ok(TokenType::Voucher {}),
            _ => Err(ContractError::new("Invalid token type key")),
        }
    }

    pub fn get_denom(&self) -> Result<String, ContractError> {
        match self.clone() {
            TokenType::Native { denom, .. } => Ok(denom),
            TokenType::Smart {
                contract_address, ..
            } => Ok(contract_address),
            TokenType::Voucher { .. } => Err(ContractError::new("Voucher has no denom")),
        }
    }

    // Create Cosmos Msg depending on type of token
    pub fn create_transfer_msg(
        &self,
        amount: Uint256,
        recipient: String,
        allowance: Option<String>,
        forwarding_message: Option<Binary>,
    ) -> Result<CosmosMsg, ContractError> {
        let msg = match self.clone() {
            TokenType::Native { denom, .. } => {
                let amount_u128 = Uint128::try_from(amount)
                    .map_err(|_| ContractError::new("Amount exceeds Uint128 maximum"))?;
                if let Some(forwarding_message) = forwarding_message {
                    CosmosMsg::Wasm(WasmMsg::Execute {
                        contract_addr: recipient.to_string(),
                        msg: forwarding_message.clone(),
                        funds: vec![coin(amount_u128.u128(), denom.clone())],
                    })
                } else {
                    CosmosMsg::Bank(BankMsg::Send {
                        to_address: recipient,
                        amount: vec![Coin {
                            denom: denom.to_string(),
                            amount,
                        }],
                    })
                }
            }
            TokenType::Smart {
                contract_address, ..
            } => {
                let amount_u128 = Uint128::try_from(amount)
                    .map_err(|_| ContractError::new("Amount exceeds Uint128 maximum"))?;
                if let Some(forwarding_message) = forwarding_message {
                    CosmosMsg::Wasm(WasmMsg::Execute {
                        contract_addr: contract_address.to_string(),
                        msg: match allowance {
                            Some(owner) => to_json_binary(&Cw20ExecuteMsg::SendFrom {
                                owner,
                                amount: amount_u128,
                                contract: recipient.to_string(),
                                msg: forwarding_message.clone(),
                            })?,
                            None => to_json_binary(&Cw20ExecuteMsg::Send {
                                contract: recipient.to_string(),
                                msg: forwarding_message.clone(),
                                amount: amount_u128,
                            })?,
                        },
                        funds: vec![],
                    })
                } else {
                    CosmosMsg::Wasm(WasmMsg::Execute {
                        contract_addr: contract_address.to_string(),
                        msg: match allowance {
                            Some(owner) => to_json_binary(&Cw20ExecuteMsg::TransferFrom {
                                owner,
                                recipient,
                                amount: amount_u128,
                            })?,
                            None => to_json_binary(&Cw20ExecuteMsg::Transfer {
                                recipient,
                                amount: amount_u128,
                            })?,
                        },
                        funds: vec![],
                    })
                }
            }
            TokenType::Voucher { .. } => {
                return Err(ContractError::new("Voucher can only be transferred in vsl"));
            }
        };
        Ok(msg)
    }

    pub fn create_escrow_msg(
        &self,
        amount: Uint256,
        escrow_contract: Addr,
    ) -> Result<CosmosMsg, ContractError> {
        let amount_u128 = Uint128::try_from(amount)
            .map_err(|_| ContractError::new("Amount exceeds Uint128 maximum"))?;
        let msg: CosmosMsg = match self {
            Self::Native { denom, .. } => CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: escrow_contract.into_string(),
                msg: to_json_binary(&crate::msgs::escrow::ExecuteMsg::DepositNative {})?,
                funds: vec![coin(amount_u128.u128(), denom)],
            }),
            Self::Smart {
                contract_address, ..
            } => CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: contract_address.clone(),
                msg: to_json_binary(&Cw20ExecuteMsg::Send {
                    contract: escrow_contract.to_string(),
                    amount: amount_u128,
                    msg: to_json_binary(&crate::msgs::escrow::cw20::EscrowCw20HookMsg::Deposit {})?,
                })?,
                funds: vec![],
            }),
            TokenType::Voucher { .. } => {
                return Err(ContractError::new("Voucher is already in escrow"));
            }
        };
        Ok(msg)
    }
}

#[cw_serde]
pub struct TokenWithAmount {
    pub token: Token,
    pub amount: Uint256,
}

#[cw_serde]
pub struct TokenWithDenomAndAmount {
    pub token: Token,
    pub amount: Uint256,
    pub token_type: TokenType,
}

impl TokenWithDenomAndAmount {
    pub fn to_token_with_amount(&self) -> TokenWithAmount {
        TokenWithAmount {
            token: self.token.clone(),
            amount: self.amount,
        }
    }

    pub fn to_token_with_denom(&self) -> TokenWithDenom {
        TokenWithDenom {
            token: self.token.clone(),
            token_type: self.token_type.clone(),
        }
    }
}

#[cw_serde]
pub struct TokenWithDenom {
    pub token: Token,
    pub token_type: TokenType,
}

impl TokenWithDenom {
    pub fn create_transfer_msg(
        &self,
        amount: Uint256,
        recipient: String,
        allowance: Option<String>,
        forwarding_message: Option<Binary>,
    ) -> Result<CosmosMsg, ContractError> {
        self.token_type
            .create_transfer_msg(amount, recipient, allowance, forwarding_message)
    }

    pub fn create_escrow_msg(
        &self,
        amount: Uint256,
        escrow_contract: Addr,
    ) -> Result<CosmosMsg, ContractError> {
        self.token_type.create_escrow_msg(amount, escrow_contract)
    }

    pub fn with_amount(&self, amount: Uint256) -> TokenWithDenomAndAmount {
        TokenWithDenomAndAmount {
            token: self.token.clone(),
            amount,
            token_type: self.token_type.clone(),
        }
    }
}

#[cw_serde]
pub struct PairWithAmount {
    pub token_1: TokenWithAmount,
    pub token_2: TokenWithAmount,
}

impl PairWithAmount {
    pub fn new(token_1: TokenWithAmount, token_2: TokenWithAmount) -> Result<Self, ContractError> {
        let pair_with_amount = if token_1.token.le(&token_2.token.to_string()) {
            Self { token_1, token_2 }
        } else {
            Self {
                token_1: token_2,
                token_2: token_1,
            }
        };
        pair_with_amount.get_pair()?.validate()?;
        Ok(pair_with_amount)
    }

    pub fn get_pair(&self) -> Result<Pair, ContractError> {
        Pair::new(self.token_1.token.clone(), self.token_2.token.clone())
    }

    pub fn get_vec_token(&self) -> Vec<TokenWithAmount> {
        let tokens: Vec<TokenWithAmount> = vec![self.token_1.clone(), self.token_2.clone()];
        tokens
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

    pub fn get_pair_with_amount(
        &self,
        token_1_amount: Uint256,
        token_2_amount: Uint256,
    ) -> Result<PairWithAmount, ContractError> {
        PairWithAmount::new(
            self.token_1.token.with_amount(token_1_amount),
            self.token_2.token.with_amount(token_2_amount),
        )
    }

    pub fn with_amount(
        &self,
        token_1_amount: Uint256,
        token_2_amount: Uint256,
    ) -> Result<PairWithDenomAndAmount, ContractError> {
        Ok(PairWithDenomAndAmount {
            token_1: self.token_1.with_amount(token_1_amount),
            token_2: self.token_2.with_amount(token_2_amount),
        })
    }

    pub fn get_vec_token_info(&self) -> Vec<TokenWithDenom> {
        let tokens = vec![self.token_1.clone(), self.token_2.clone()];
        tokens
    }
}

#[cw_serde]
pub struct PairWithDenomAndAmount {
    pub token_1: TokenWithDenomAndAmount,
    pub token_2: TokenWithDenomAndAmount,
}

impl PairWithDenomAndAmount {
    pub fn get_pair(&self) -> Result<Pair, ContractError> {
        Pair::new(self.token_1.token.clone(), self.token_2.token.clone())
    }

    pub fn get_pair_with_denom(&self) -> Result<PairWithDenom, ContractError> {
        Ok(PairWithDenom {
            token_1: self.token_1.to_token_with_denom(),
            token_2: self.token_2.to_token_with_denom(),
        })
    }

    pub fn get_pair_with_amount(&self) -> Result<PairWithAmount, ContractError> {
        PairWithAmount::new(
            self.token_1.to_token_with_amount(),
            self.token_2.to_token_with_amount(),
        )
    }

    pub fn get_vec_token_info(&self) -> Vec<TokenWithDenomAndAmount> {
        let tokens: Vec<TokenWithDenomAndAmount> = vec![self.token_1.clone(), self.token_2.clone()];
        tokens
    }
}

#[cw_serde]
pub struct TokenMetadata {
    pub token: Token,
    pub chain_uid: ChainUid,
    pub token_type: TokenType,
    // If false, the token is not allowed to be used in the contract
    pub allowed: bool,
}

impl TokenMetadata {
    pub fn new(token: Token, chain_uid: ChainUid, token_type: TokenType) -> Self {
        Self {
            token,
            chain_uid,
            token_type,
            allowed: true,
        }
    }
}

#[cfg(test)]
use cosmwasm_std::testing::mock_dependencies;

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

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

    struct TestPairWithDenom {
        name: &'static str,
        pair_with_denom: PairWithDenomAndAmount, // Ensure PairWithDenom is also defined
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

    #[test]
    fn test_pair_with_denom_validation() {
        let test_cases = vec![
            TestPairWithDenom {
                name: "Duplicate tokens with denom",
                pair_with_denom: PairWithDenomAndAmount {
                    token_1: TokenWithDenomAndAmount {
                        token: Token("ABC".to_string()),
                        amount: Uint256::from(100u128),
                        token_type: TokenType::Native {
                            denom: "denom1".to_string(),
                            decimals: None,
                        },
                    },
                    token_2: TokenWithDenomAndAmount {
                        token: Token("ABC".to_string()),
                        amount: Uint256::from(100u128),
                        token_type: TokenType::Native {
                            denom: "denom2".to_string(),
                            decimals: None,
                        },
                    },
                },
                expected_error: Some(ContractError::DuplicateTokens {}),
            },
            TestPairWithDenom {
                name: "Different tokens with different denoms",
                pair_with_denom: PairWithDenomAndAmount {
                    token_1: TokenWithDenomAndAmount {
                        token: Token("ABC".to_string()),
                        amount: Uint256::from(100u128),
                        token_type: TokenType::Native {
                            denom: "denom1".to_string(),
                            decimals: None,
                        },
                    },
                    token_2: TokenWithDenomAndAmount {
                        token: Token("DEF".to_string()),
                        amount: Uint256::from(100u128),
                        token_type: TokenType::Native {
                            denom: "denom2".to_string(),
                            decimals: None,
                        },
                    },
                },
                expected_error: None,
            },
            TestPairWithDenom {
                name: "Same letters but with different case and different denoms",
                pair_with_denom: PairWithDenomAndAmount {
                    token_1: TokenWithDenomAndAmount {
                        token: Token("ABC".to_string()),
                        amount: Uint256::from(100u128),
                        token_type: TokenType::Native {
                            denom: "denom1".to_string(),
                            decimals: None,
                        },
                    },
                    token_2: TokenWithDenomAndAmount {
                        token: Token("AbC".to_string()),
                        amount: Uint256::from(100u128),
                        token_type: TokenType::Native {
                            denom: "denom2".to_string(),
                            decimals: None,
                        },
                    },
                },
                expected_error: None,
            },
            TestPairWithDenom {
                name: "One invalid token with denom",
                pair_with_denom: PairWithDenomAndAmount {
                    token_1: TokenWithDenomAndAmount {
                        token: Token("ABC".to_string()),
                        amount: Uint256::from(100u128),
                        token_type: TokenType::Native {
                            denom: "denom1".to_string(),
                            decimals: None,
                        },
                    },
                    token_2: TokenWithDenomAndAmount {
                        token: Token("".to_string()),
                        amount: Uint256::from(100u128),
                        token_type: TokenType::Native {
                            denom: "denom2".to_string(),
                            decimals: None,
                        },
                    },
                },
                expected_error: Some(ContractError::InvalidTokenID {}),
            },
        ];

        for test in test_cases {
            let res = test.pair_with_denom.get_pair();
            if let Some(err) = test.expected_error {
                assert_eq!(res.unwrap_err(), err, "{}", test.name);
                continue;
            } else {
                assert!(res.is_ok())
            }
        }
    }

    #[rstest]
    #[case(TokenType::Native { denom: "uatom".to_string(), decimals: None }, "native:uatom")]
    #[case(TokenType::Native { denom: "ibc/abc123".to_string(), decimals: None }, "native:ibc/abc123")]
    #[case(TokenType::Smart { contract_address: "cosmos1abc".to_string(), decimals: None }, "smart:cosmos1abc")]
    #[case(TokenType::Voucher {}, "voucher")]
    fn test_token_type_get_key(#[case] token_type: TokenType, #[case] expected: &str) {
        assert_eq!(token_type.get_key(), expected);
    }

    #[rstest]
    #[case("native:uatom", Ok(TokenType::Native { denom: "uatom".to_string(), decimals: None }))]
    #[case("native:ibc/abc123", Ok(TokenType::Native { denom: "ibc/abc123".to_string(), decimals: None }))]
    #[case("smart:cosmos1abc", Ok(TokenType::Smart { contract_address: "cosmos1abc".to_string(), decimals: None  }))]
    #[case("voucher", Ok(TokenType::Voucher {}))]
    #[case("invalid", Err(ContractError::new("Invalid token type key")))]
    #[case("", Err(ContractError::new("Invalid token type key")))]
    #[case("unknown:value", Err(ContractError::new("Invalid token type key")))]
    fn test_token_type_from_key(
        #[case] key: &str,
        #[case] expected: Result<TokenType, ContractError>,
    ) {
        assert_eq!(TokenType::from_key(key.to_string()), expected);
    }

    #[rstest]
    #[case(TokenType::Native { denom: "uosmo".to_string(), decimals: None })]
    #[case(TokenType::Smart { contract_address: "cosmos1xyz".to_string(), decimals: None })]
    #[case(TokenType::Voucher {})]
    fn test_token_type_key_roundtrip(#[case] original: TokenType) {
        let recovered = TokenType::from_key(original.get_key()).unwrap();
        assert_eq!(recovered, original);
    }
}
