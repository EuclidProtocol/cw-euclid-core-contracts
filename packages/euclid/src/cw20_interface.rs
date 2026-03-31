//! Single-variant interface enums for `cw20_base::msg::ExecuteMsg` variants.
//!
//! These avoid pulling in serde code for the full cw20 ExecuteMsg (which has ~15 variants)
//! when only a few are used for cross-contract calls.

use crate::interface::ContractInterface;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Binary, Uint128};

/// Interface for `cw20_base::msg::ExecuteMsg::Send`
#[cw_serde]
pub enum Cw20SendMsg {
    Send {
        contract: String,
        amount: Uint128,
        msg: Binary,
    },
}
impl ContractInterface for Cw20SendMsg {}

/// Interface for `cw20_base::msg::ExecuteMsg::SendFrom`
#[cw_serde]
pub enum Cw20SendFromMsg {
    SendFrom {
        owner: String,
        contract: String,
        amount: Uint128,
        msg: Binary,
    },
}
impl ContractInterface for Cw20SendFromMsg {}

/// Interface for `cw20_base::msg::ExecuteMsg::Transfer`
#[cw_serde]
pub enum Cw20TransferMsg {
    Transfer { recipient: String, amount: Uint128 },
}
impl ContractInterface for Cw20TransferMsg {}

/// Interface for `cw20_base::msg::ExecuteMsg::TransferFrom`
#[cw_serde]
pub enum Cw20TransferFromMsg {
    TransferFrom {
        owner: String,
        recipient: String,
        amount: Uint128,
    },
}
impl ContractInterface for Cw20TransferFromMsg {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interface::assert_interface_eq;

    #[test]
    fn cw20_send_interface_matches_parent() {
        let interface = Cw20SendMsg::Send {
            contract: "contract1".to_string(),
            amount: Uint128::new(100),
            msg: Binary::from(b"hook"),
        };
        let parent = cw20_base::msg::ExecuteMsg::Send {
            contract: "contract1".to_string(),
            amount: Uint128::new(100),
            msg: Binary::from(b"hook"),
        };
        assert_interface_eq(&interface, &parent);
    }

    #[test]
    fn cw20_send_from_interface_matches_parent() {
        let interface = Cw20SendFromMsg::SendFrom {
            owner: "owner1".to_string(),
            contract: "contract1".to_string(),
            amount: Uint128::new(100),
            msg: Binary::from(b"hook"),
        };
        let parent = cw20_base::msg::ExecuteMsg::SendFrom {
            owner: "owner1".to_string(),
            contract: "contract1".to_string(),
            amount: Uint128::new(100),
            msg: Binary::from(b"hook"),
        };
        assert_interface_eq(&interface, &parent);
    }

    #[test]
    fn cw20_transfer_interface_matches_parent() {
        let interface = Cw20TransferMsg::Transfer {
            recipient: "recipient1".to_string(),
            amount: Uint128::new(100),
        };
        let parent = cw20_base::msg::ExecuteMsg::Transfer {
            recipient: "recipient1".to_string(),
            amount: Uint128::new(100),
        };
        assert_interface_eq(&interface, &parent);
    }

    #[test]
    fn cw20_transfer_from_interface_matches_parent() {
        let interface = Cw20TransferFromMsg::TransferFrom {
            owner: "owner1".to_string(),
            recipient: "recipient1".to_string(),
            amount: Uint128::new(100),
        };
        let parent = cw20_base::msg::ExecuteMsg::TransferFrom {
            owner: "owner1".to_string(),
            recipient: "recipient1".to_string(),
            amount: Uint128::new(100),
        };
        assert_interface_eq(&interface, &parent);
    }
}
