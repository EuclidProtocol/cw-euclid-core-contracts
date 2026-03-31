use crate::interface::ContractInterface;
use crate::token::TokenType;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};

/// Interface for [`super::msg::ExecuteMsg::AddAllowedDenom`]
#[cw_serde]
pub enum AddAllowedDenomMsg {
    AddAllowedDenom { denom: TokenType },
}
impl ContractInterface for AddAllowedDenomMsg {}

/// Interface for [`super::msg::ExecuteMsg::DisallowDenom`]
#[cw_serde]
pub enum DisallowDenomMsg {
    DisallowDenom { denom: TokenType },
}
impl ContractInterface for DisallowDenomMsg {}

/// Interface for [`super::msg::ExecuteMsg::DepositNative`]
#[cw_serde]
pub enum DepositNativeMsg {
    DepositNative {},
}
impl ContractInterface for DepositNativeMsg {}

/// Interface for [`super::msg::ExecuteMsg::Withdraw`]
#[cw_serde]
pub enum WithdrawMsg {
    Withdraw {
        recipient: Addr,
        amount: Uint128,
        denom: TokenType,
        forwarding_message: Option<String>,
    },
}
impl ContractInterface for WithdrawMsg {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interface::assert_interface_eq;
    use crate::msgs::escrow::msg::ExecuteMsg;

    #[test]
    fn add_allowed_denom_interface_matches_parent() {
        let denom = TokenType::Native {
            denom: "uatom".into(),
        };
        let interface = AddAllowedDenomMsg::AddAllowedDenom {
            denom: denom.clone(),
        };
        let parent = ExecuteMsg::AddAllowedDenom { denom };
        assert_interface_eq(&interface, &parent);
    }

    #[test]
    fn disallow_denom_interface_matches_parent() {
        let denom = TokenType::Native {
            denom: "uatom".into(),
        };
        let interface = DisallowDenomMsg::DisallowDenom {
            denom: denom.clone(),
        };
        let parent = ExecuteMsg::DisallowDenom { denom };
        assert_interface_eq(&interface, &parent);
    }

    #[test]
    fn withdraw_interface_matches_parent() {
        let interface = WithdrawMsg::Withdraw {
            recipient: Addr::unchecked("recipient"),
            amount: 100u128.into(),
            denom: TokenType::Native {
                denom: "uatom".into(),
            },
            forwarding_message: Some("fwd".into()),
        };
        let parent = ExecuteMsg::Withdraw {
            recipient: Addr::unchecked("recipient"),
            amount: 100u128.into(),
            denom: TokenType::Native {
                denom: "uatom".into(),
            },
            forwarding_message: Some("fwd".into()),
        };
        assert_interface_eq(&interface, &parent);
    }

    #[test]
    fn deposit_native_interface_matches_parent() {
        let interface = DepositNativeMsg::DepositNative {};
        let parent = ExecuteMsg::DepositNative {};
        assert_interface_eq(&interface, &parent);
    }
}
