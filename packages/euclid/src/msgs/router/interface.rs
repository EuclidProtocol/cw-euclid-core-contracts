use crate::chain::ChainUid;
use crate::interface::ContractInterface;
use crate::recipient::Recipient;
use crate::token::Token;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Binary, Uint128};

/// Interface for [`super::execute::ExecuteMsg::ReceivePacketInternalCallback`]
#[cw_serde]
pub enum ReceivePacketInternalCallbackMsg {
    ReceivePacketInternalCallback {
        msg: Binary,
        chain_uid: ChainUid,
        timeout: u64,
    },
}
impl ContractInterface for ReceivePacketInternalCallbackMsg {}

/// Interface for [`super::execute::ExecuteMsg::TransferVoucher`]
#[cw_serde]
pub enum TransferVoucherMsg {
    TransferVoucher {
        token: Token,
        amount: Uint128,
        recipient: Vec<Recipient>,
    },
}
impl ContractInterface for TransferVoucherMsg {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cross_chain_user::CrossChainUser;
    use crate::interface::assert_interface_eq;
    use crate::limit::Limit;
    use crate::msgs::router::execute::ExecuteMsg;
    use crate::token::TokenType;

    #[test]
    fn receive_packet_internal_callback_interface_matches_parent() {
        let msg = Binary::from(b"test_data");
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let timeout = 100u64;
        let interface = ReceivePacketInternalCallbackMsg::ReceivePacketInternalCallback {
            msg: msg.clone(),
            chain_uid: chain_uid.clone(),
            timeout,
        };
        let parent = ExecuteMsg::ReceivePacketInternalCallback {
            msg,
            chain_uid,
            timeout,
        };
        assert_interface_eq(&interface, &parent);
    }

    #[test]
    fn transfer_voucher_interface_matches_parent() {
        let token = Token::create("token1".to_string()).unwrap();
        let amount = Uint128::new(1000);
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let recipient = vec![Recipient {
            recipient: CrossChainUser::new(chain_uid, "addr1".to_string()),
            amount: Limit::Equal(Uint128::new(1000)),
            denom: TokenType::Voucher {},
            forwarding_message: None,
            unsafe_refund_as_voucher: None,
        }];
        let interface = TransferVoucherMsg::TransferVoucher {
            token: token.clone(),
            amount,
            recipient: recipient.clone(),
        };
        let parent = ExecuteMsg::TransferVoucher {
            token,
            amount,
            recipient,
        };
        assert_interface_eq(&interface, &parent);
    }
}
