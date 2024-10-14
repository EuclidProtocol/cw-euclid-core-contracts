use crate::contract::{execute, instantiate, query};
use cw_orch::{interface, prelude::*};
use euclid::msgs::router::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
pub const CONTRACT_ID: &str = "router_contract";

#[interface(InstantiateMsg, ExecuteMsg, QueryMsg, MigrateMsg, id = CONTRACT_ID)]
pub struct RouterContract<Chain: CwEnv>;

// Implement the Uploadable trait so it can be uploaded to the mock.
impl<Chain> Uploadable for RouterContract<Chain> {
    fn wrapper() -> Box<dyn MockContract<Empty>> {
        Box::new(
            ContractWrapper::new_with_empty(execute, instantiate, query)
                .with_reply(crate::contract::reply)
                .with_ibc(
                    crate::ibc::channel::ibc_channel_open,
                    crate::ibc::channel::ibc_channel_connect,
                    crate::ibc::channel::ibc_channel_close,
                    crate::ibc::receive::ibc_packet_receive,
                    crate::ibc::ack_and_timeout::ibc_packet_ack,
                    crate::ibc::ack_and_timeout::ibc_packet_timeout,
                ),
        )
    }
}
