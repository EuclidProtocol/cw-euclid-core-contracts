use crate::interface::ContractInterface;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Binary};

/// Interface for [`super::msg::ExecuteMsg::ReceivePacketInternalCallback`]
#[cw_serde]
pub enum ReceivePacketInternalCallbackMsg {
    ReceivePacketInternalCallback { msg: Binary, timeout: u64 },
}
impl ContractInterface for ReceivePacketInternalCallbackMsg {}

/// Interface for [`super::msg::ExecuteMsg::SendPacket`]
#[cw_serde]
pub enum SendPacketMsg {
    SendPacket {
        msg: Binary,
        timeout: Option<u64>,
        ack_response: Option<Binary>,
        sender: Addr,
    },
}
impl ContractInterface for SendPacketMsg {}

/// Interface for [`super::msg::ExecuteMsg::NativeReceiveCallback`]
#[cw_serde]
pub enum NativeReceiveCallbackMsg {
    NativeReceiveCallback { msg: Binary },
}
impl ContractInterface for NativeReceiveCallbackMsg {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interface::assert_interface_eq;
    use crate::msgs::factory::msg::ExecuteMsg;

    #[test]
    fn receive_packet_internal_callback_interface_matches_parent() {
        let msg = Binary::from(b"test_data");
        let timeout = 100u64;
        let interface = ReceivePacketInternalCallbackMsg::ReceivePacketInternalCallback {
            msg: msg.clone(),
            timeout,
        };
        let parent = ExecuteMsg::ReceivePacketInternalCallback { msg, timeout };
        assert_interface_eq(&interface, &parent);
    }

    #[test]
    fn send_packet_interface_matches_parent() {
        let msg = Binary::from(b"test_data");
        let sender = Addr::unchecked("sender1");
        let interface = SendPacketMsg::SendPacket {
            msg: msg.clone(),
            timeout: Some(100),
            ack_response: None,
            sender: sender.clone(),
        };
        let parent = ExecuteMsg::SendPacket {
            msg,
            timeout: Some(100),
            ack_response: None,
            sender,
        };
        assert_interface_eq(&interface, &parent);
    }

    #[test]
    fn native_receive_callback_interface_matches_parent() {
        let msg = Binary::from(b"test_data");
        let interface = NativeReceiveCallbackMsg::NativeReceiveCallback { msg: msg.clone() };
        let parent = ExecuteMsg::NativeReceiveCallback { msg };
        assert_interface_eq(&interface, &parent);
    }
}
