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

/// Interface for [`super::execute::ExecuteMsg::SendPacket`]
#[cw_serde]
pub enum SendPacketMsg {
    SendPacket {
        sender: String,
        msg: Binary,
        chain: crate::chain::Chain,
        timeout: Option<u64>,
        ack_response: Option<Binary>,
    },
}
impl ContractInterface for SendPacketMsg {}

/// Interface for [`super::execute::ExecuteMsg::NativeReceiveCallback`]
#[cw_serde]
pub enum NativeReceiveCallbackMsg {
    NativeReceiveCallback { msg: Binary, chain_uid: ChainUid },
}
impl ContractInterface for NativeReceiveCallbackMsg {}

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

    #[test]
    fn send_packet_interface_matches_parent() {
        let msg = Binary::from(b"test_data");
        let chain = crate::chain::Chain {
            chain_uid: ChainUid::create("chain1".to_string()).unwrap(),
            factory_address: "factory1".to_string(),
            chain_type: crate::chain::ChainType::Native {},
        };
        let interface = SendPacketMsg::SendPacket {
            sender: "sender1".to_string(),
            msg: msg.clone(),
            chain: chain.clone(),
            timeout: Some(100),
            ack_response: None,
        };
        let parent = ExecuteMsg::SendPacket {
            sender: "sender1".to_string(),
            msg,
            chain,
            timeout: Some(100),
            ack_response: None,
        };
        assert_interface_eq(&interface, &parent);
    }

    #[test]
    fn native_receive_callback_interface_matches_parent() {
        let msg = Binary::from(b"test_data");
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let interface = NativeReceiveCallbackMsg::NativeReceiveCallback {
            msg: msg.clone(),
            chain_uid: chain_uid.clone(),
        };
        let parent = ExecuteMsg::NativeReceiveCallback { msg, chain_uid };
        assert_interface_eq(&interface, &parent);
    }
}
