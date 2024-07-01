use cosmwasm_std::{
    instantiate2_address, to_json_binary, Addr, Api, Binary, CodeInfoResponse, QuerierWrapper,
    SubMsg, WasmMsg,
};
use cw20::MinterResponse;
use cw20_base::msg::InstantiateMsg as Cw20InstantiateMsg;

use crate::error::ContractError;

pub fn generate_id(sender: &str, count: u128) -> String {
    format!("{sender}-{count}")
}

pub fn get_new_addr(
    api: &dyn Api,
    code_id: u64,
    vlp_address: String,
    querier: &QuerierWrapper,
) -> Result<Option<Addr>, ContractError> {
    let CodeInfoResponse { checksum, .. } = querier.query_wasm_code_info(code_id)?;

    let salt = Binary::from(vlp_address.as_bytes());
    let creator = api.addr_canonicalize(&vlp_address)?;
    let new_addr = instantiate2_address(&checksum, &creator, &salt).unwrap();

    // Instantiate 2 impl uses default cannonical address of 32 bytes (SHA 256). But as mentioned here -
    // https://github.com/cosmos/cosmos-sdk/blob/v0.45.8/docs/architecture/adr-028-public-key-addresses.md
    // chains can use different length for cannonical address, eg, injective uses 20 (eth based).
    // Instead of having fallback for each chain we can use parent address, which itself is a contract.
    // Slice the default 32 bytes canonical address to size of parent cannonical address

    let cannonical_parent_addr = api.addr_canonicalize(&vlp_address)?;
    let new_addr = new_addr
        .as_slice()
        .split_at(cannonical_parent_addr.len())
        .0
        .into();

    Ok(Some(api.addr_humanize(&new_addr)?))
}

pub fn generate_instantiate2_message(
    code_id: u64,
    vlp_address: String,
    idx: u64,
) -> Result<SubMsg, ContractError> {
    let salt = Binary::from(vlp_address.as_bytes());
    let inst_msg = WasmMsg::Instantiate2 {
        admin: Some(vlp_address.clone()),
        code_id,
        // TODO review the below entries
        label: format!("Instantiate: CW20"),
        msg: to_json_binary(&Cw20InstantiateMsg {
            name: "cw20".to_string(),
            symbol: "symbol".to_string(),
            decimals: 6,
            initial_balances: vec![],
            mint: Some(MinterResponse {
                minter: vlp_address,
                cap: None,
            }),
            marketing: None,
        })?,
        funds: vec![],
        salt,
    };
    Ok(SubMsg::reply_always(inst_msg, idx))
}
