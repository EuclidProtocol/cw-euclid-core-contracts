use cosmwasm_std::{to_json_binary, Coin, CosmosMsg, StdResult, WasmMsg};
use serde::Serialize;

/// Trait for single-variant interface enums used in cross-contract calls.
///
/// Each implementor represents exactly one variant of a parent ExecuteMsg,
/// producing identical JSON serialization with minimal serde footprint.
/// This avoids pulling in serde monomorphizations for the entire parent enum
/// when only one variant is needed for a cross-contract call.
pub trait ContractInterface: Serialize + Sized {
    /// Wraps this message as a `CosmosMsg::Wasm(WasmMsg::Execute)` with no funds.
    fn into_cosmos_msg(self, contract_addr: impl Into<String>) -> StdResult<CosmosMsg> {
        self.into_cosmos_msg_with_funds(contract_addr, vec![])
    }

    /// Wraps this message as a `CosmosMsg::Wasm(WasmMsg::Execute)` with funds.
    fn into_cosmos_msg_with_funds(
        self,
        contract_addr: impl Into<String>,
        funds: Vec<Coin>,
    ) -> StdResult<CosmosMsg> {
        Ok(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: contract_addr.into(),
            msg: to_json_binary(&self)?,
            funds,
        }))
    }
}

/// Test helper: asserts that an interface message serializes identically to its parent enum variant.
#[cfg(test)]
pub fn assert_interface_eq<I: Serialize, P: Serialize>(interface_msg: &I, parent_msg: &P) {
    let interface_json = to_json_binary(interface_msg).unwrap();
    let parent_json = to_json_binary(parent_msg).unwrap();
    assert_eq!(
        interface_json, parent_json,
        "Interface must serialize identically to parent enum variant.\n  interface: {}\n  parent:    {}",
        String::from_utf8_lossy(interface_json.as_slice()),
        String::from_utf8_lossy(parent_json.as_slice()),
    );
}
