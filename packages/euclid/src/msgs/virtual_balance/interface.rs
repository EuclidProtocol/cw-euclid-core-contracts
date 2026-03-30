use crate::interface::ContractInterface;
use cosmwasm_schema::cw_serde;

use super::msg::{ExecuteApprove, ExecuteBurn, ExecuteMint, ExecuteTransfer};

/// Interface for [`super::msg::ExecuteMsg::Mint`]
#[cw_serde]
pub enum MintMsg {
    Mint(ExecuteMint),
}
impl ContractInterface for MintMsg {}

/// Interface for [`super::msg::ExecuteMsg::Transfer`]
#[cw_serde]
pub enum TransferMsg {
    Transfer(ExecuteTransfer),
}
impl ContractInterface for TransferMsg {}

/// Interface for [`super::msg::ExecuteMsg::Burn`]
#[cw_serde]
pub enum BurnMsg {
    Burn(ExecuteBurn),
}
impl ContractInterface for BurnMsg {}

/// Interface for [`super::msg::ExecuteMsg::Approve`]
#[cw_serde]
pub enum ApproveMsg {
    Approve(ExecuteApprove),
}
impl ContractInterface for ApproveMsg {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::ChainUid;
    use crate::cross_chain_user::CrossChainUser;
    use crate::interface::assert_interface_eq;
    use crate::msgs::virtual_balance::msg::ExecuteMsg;
    use crate::voucher::BalanceKey;
    use cosmwasm_std::Uint128;

    fn test_chain_uid() -> ChainUid {
        ChainUid::create("chain1".to_string()).unwrap()
    }

    fn test_user(addr: &str) -> CrossChainUser {
        CrossChainUser::new(test_chain_uid(), addr.to_string())
    }

    #[test]
    fn mint_interface_matches_parent() {
        let mint = ExecuteMint {
            amount: 1000u128.into(),
            balance_key: BalanceKey {
                cross_chain_user: test_user("addr1"),
                token_id: "token1".into(),
            },
        };
        let interface = MintMsg::Mint(mint.clone());
        let parent = ExecuteMsg::Mint(mint);
        assert_interface_eq(&interface, &parent);
    }

    #[test]
    fn transfer_interface_matches_parent() {
        let transfer = ExecuteTransfer {
            amount: 500u128.into(),
            token_id: "token1".into(),
            sender: None,
            to: test_user("addr2"),
            from: None,
            msg: None,
        };
        let interface = TransferMsg::Transfer(transfer.clone());
        let parent = ExecuteMsg::Transfer(transfer);
        assert_interface_eq(&interface, &parent);
    }

    #[test]
    fn burn_interface_matches_parent() {
        let burn = ExecuteBurn {
            amount: 200u128.into(),
            balance_key: BalanceKey {
                cross_chain_user: test_user("addr1"),
                token_id: "token1".into(),
            },
        };
        let interface = BurnMsg::Burn(burn.clone());
        let parent = ExecuteMsg::Burn(burn);
        assert_interface_eq(&interface, &parent);
    }

    #[test]
    fn approve_interface_matches_parent() {
        let approve = ExecuteApprove {
            amount: Uint128::new(100),
            token_id: "token1".into(),
            spender: test_user("spender"),
            owner: test_user("owner"),
        };
        let interface = ApproveMsg::Approve(approve.clone());
        let parent = ExecuteMsg::Approve(approve);
        assert_interface_eq(&interface, &parent);
    }
}
