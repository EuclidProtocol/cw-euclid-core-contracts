use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Binary};

/// Typed payload that `pool_factory` returns via `Response::data` on the
/// handlers main factory dispatched as `SubMsg::reply_on_success`.
///
/// Replaces the previous `factory::ExecuteMsg::ProxySendPacket` round-trip:
/// pool_factory no longer calls main factory to request an outbound IBC
/// packet; instead it sets this payload, and main factory's reply handler
/// consumes it. The trust boundary is structural (CosmWasm reply scoping
/// guarantees the data comes from a submsg main factory itself dispatched)
/// rather than a runtime sender check.
///
/// Single variant today; deliberately an enum so future reply-driven actions
/// (e.g. `MintLpToken`, `BurnLpToken`, `ReleaseEscrow`) can be added without
/// changing the wire shape.
#[cw_serde]
pub enum PoolFactoryReply {
    /// Requests main factory to dispatch a `RouterCrossChainExecuteMsg`
    /// through its existing `execute_send_packet` flow.
    ///
    /// `msg` is the serialised `RouterCrossChainExecuteMsg`; main factory
    /// MUST decode it and reject any non-pool variant before dispatching.
    SendPacket {
        msg: Binary,
        timeout: Option<u64>,
        ack_response: Option<Binary>,
        sender: Addr,
    },
}
