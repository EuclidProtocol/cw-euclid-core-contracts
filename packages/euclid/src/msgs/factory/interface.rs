use crate::interface::ContractInterface;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::Binary;

/// Interface for [`super::msg::ExecuteMsg::ReceivePacketInternalCallback`]
#[cw_serde]
pub enum ReceivePacketInternalCallbackMsg {
    ReceivePacketInternalCallback { msg: Binary, timeout: u64 },
}
impl ContractInterface for ReceivePacketInternalCallbackMsg {}

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
}
