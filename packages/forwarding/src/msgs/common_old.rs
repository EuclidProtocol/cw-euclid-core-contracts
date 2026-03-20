use cosmwasm_schema::cw_serde;
use cosmwasm_std::{coin, to_json_binary, BankMsg, Binary, Event, WasmMsg};
use cosmwasm_std::{ensure, Coin, CosmosMsg, Deps, Uint128, Uint256};

use super::errors_old::ContractError;

#[cw_serde]
pub struct EuclidReceive {
    pub data: Binary,
}

// This is just a helper to properly serialize the above message
#[cw_serde]
pub enum EuclidReceiverMsg {
    EuclidReceive(EuclidReceive),
}

impl EuclidReceive {
    pub fn to_receiver_msg(&self) -> Result<Binary, ContractError> {
        Ok(to_json_binary(&EuclidReceiverMsg::EuclidReceive(
            self.clone(),
        ))?)
    }
    pub fn from_msg(binary: &Binary) -> Self {
        Self {
            data: binary.clone(),
        }
    }
}

pub fn simple_event() -> Event {
    Event::new("euclid").add_attribute("version", "1.0.0")
}

#[cw_serde]
pub enum TokenType {
    Native { denom: String },
    Smart { contract_address: String },
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
            TokenType::Smart { contract_address } => Ok(contract_address.clone()),
            _ => Err(ContractError::new("Token is not smart")),
        }
    }

    pub fn is_voucher(&self) -> bool {
        matches!(self, TokenType::Voucher { .. })
    }

    /// Validates smart contract addresses, checks against empty denom and zero supply
    pub fn validate(&self, deps: &Deps) -> Result<(), ContractError> {
        if let Self::Native { denom } = &self {
            let potential_supply = deps.querier.query_supply(denom.clone())?;
            let non_zero_supply = !potential_supply.amount.is_zero();
            ensure!(
                non_zero_supply,
                ContractError::ZeroAssetSupply {
                    asset: denom.clone()
                }
            );
        } else if let Self::Smart { contract_address } = &self {
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
            TokenType::Native { denom } => {
                let balance = deps.querier.query_balance(address, denom)?;
                Ok(Uint256::from(balance.amount))
            }
            TokenType::Smart { contract_address } => {
                let balance_msg = cw20::Cw20QueryMsg::Balance {
                    address: address.clone(),
                };
                let balance: cw20::BalanceResponse = deps
                    .querier
                    .query_wasm_smart(contract_address, &balance_msg)?;
                Ok(Uint256::from(balance.balance))
            }
            TokenType::Voucher { .. } => Err(ContractError::new(
                "Cannot get balance of voucher using this function",
            )),
        }
    }

    pub fn get_key(&self) -> String {
        match self.clone() {
            TokenType::Native { denom } => format!("native:{denom}"),
            TokenType::Smart { contract_address } => format!("smart:{contract_address}"),
            TokenType::Voucher { .. } => "voucher".to_string(),
        }
    }

    pub fn get_denom(&self) -> Result<String, ContractError> {
        match self.clone() {
            TokenType::Native { denom } => Ok(denom),
            TokenType::Smart { contract_address } => Ok(contract_address),
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
            TokenType::Native { denom } => {
                if let Some(forwarding_message) = forwarding_message {
                    CosmosMsg::Wasm(WasmMsg::Execute {
                        contract_addr: recipient.to_string(),
                        msg: forwarding_message.clone(),
                        funds: vec![coin(
                            Uint128::try_from(amount).unwrap().u128(),
                            denom.clone(),
                        )],
                    })
                } else {
                    let amount_u128 = Uint128::try_from(amount).unwrap();
                    CosmosMsg::Bank(BankMsg::Send {
                        to_address: recipient,
                        amount: vec![Coin {
                            denom: denom.to_string(),
                            amount: amount_u128,
                        }],
                    })
                }
            }
            TokenType::Smart { contract_address } => {
                let amount_u128 = Uint128::try_from(amount).unwrap();
                if let Some(forwarding_message) = forwarding_message {
                    CosmosMsg::Wasm(WasmMsg::Execute {
                        contract_addr: contract_address.to_string(),
                        msg: match allowance {
                            Some(owner) => to_json_binary(&cw20::Cw20ExecuteMsg::SendFrom {
                                owner,
                                amount: amount_u128,
                                contract: recipient.to_string(),
                                msg: forwarding_message.clone(),
                            })?,
                            None => to_json_binary(&cw20::Cw20ExecuteMsg::Send {
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
                            Some(owner) => to_json_binary(&cw20::Cw20ExecuteMsg::TransferFrom {
                                owner,
                                recipient,
                                amount: amount_u128,
                            })?,
                            None => to_json_binary(&cw20::Cw20ExecuteMsg::Transfer {
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
}
