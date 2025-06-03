use crate::contract::{execute, instantiate, query};
use cw_orch::{interface, prelude::*};
use euclid::msgs::migrator::{ExecuteMsg, InstantiateMsg, QueryMsg};
pub const CONTRACT_ID: &str = "migration_contract";

#[interface(InstantiateMsg, ExecuteMsg, QueryMsg, Empty, id = CONTRACT_ID)]
pub struct MigrationContract<Chain: CwEnv>;

// Implement the Uploadable trait so it can be uploaded to the mock.
impl<Chain> Uploadable for MigrationContract<Chain> {
    fn wrapper() -> Box<dyn MockContract<Empty>> {
        Box::new(
            ContractWrapper::new_with_empty(execute, instantiate, query)
                .with_reply(crate::contract::reply)
                .with_ibc(
                    crate::ibc::ibc_channel_open,
                    crate::ibc::ibc_channel_connect,
                    crate::ibc::ibc_channel_close,
                    crate::ibc::ibc_packet_receive,
                    crate::ibc::ibc_packet_ack,
                    crate::ibc::ibc_packet_timeout,
                ),
        )
    }
}
