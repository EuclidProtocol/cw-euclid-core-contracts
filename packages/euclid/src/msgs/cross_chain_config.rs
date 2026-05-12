use cosmwasm_schema::cw_serde;
use cosmwasm_std::Binary;

#[cw_serde]
#[derive(Default)]
pub struct CrossChainConfig {
    // Timeout for the cross chain message
    pub timeout: Option<u64>,
    // Ack response for the cross chain message. Will be used to trigger a euclid receive message on sender when ack is received.
    pub ack_response: Option<Binary>,
    // Meta data for the cross chain message. Will be used to store any additional data needed for the cross chain message as event attributes.
    pub meta: Option<String>,
}

impl CrossChainConfig {
    #[must_use]
    pub fn new(timeout: Option<u64>, ack_response: Option<Binary>, meta: Option<String>) -> Self {
        Self {
            timeout,
            ack_response,
            meta,
        }
    }
}
