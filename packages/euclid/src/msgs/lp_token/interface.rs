use crate::interface::ContractInterface;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;

/// Interface for [`super::msg::ExecuteMsg::Mint`]
#[cw_serde]
pub enum MintMsg {
    Mint { recipient: String, amount: Uint128 },
}
impl ContractInterface for MintMsg {}

/// Interface for [`super::msg::ExecuteMsg::Burn`]
#[cw_serde]
pub enum BurnMsg {
    Burn { amount: Uint128 },
}
impl ContractInterface for BurnMsg {}

/// Interface for [`super::msg::ExecuteMsg::Transfer`]
#[cw_serde]
pub enum TransferMsg {
    Transfer { recipient: String, amount: Uint128 },
}
impl ContractInterface for TransferMsg {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interface::assert_interface_eq;
    use crate::msgs::lp_token::msg::ExecuteMsg;

    #[test]
    fn mint_interface_matches_parent() {
        let interface = MintMsg::Mint {
            recipient: "addr1".into(),
            amount: 1000u128.into(),
        };
        let parent = ExecuteMsg::Mint {
            recipient: "addr1".into(),
            amount: 1000u128.into(),
        };
        assert_interface_eq(&interface, &parent);
    }

    #[test]
    fn burn_interface_matches_parent() {
        let interface = BurnMsg::Burn {
            amount: 500u128.into(),
        };
        let parent = ExecuteMsg::Burn {
            amount: 500u128.into(),
        };
        assert_interface_eq(&interface, &parent);
    }

    #[test]
    fn transfer_interface_matches_parent() {
        let interface = TransferMsg::Transfer {
            recipient: "addr2".into(),
            amount: 200u128.into(),
        };
        let parent = ExecuteMsg::Transfer {
            recipient: "addr2".into(),
            amount: 200u128.into(),
        };
        assert_interface_eq(&interface, &parent);
    }
}
