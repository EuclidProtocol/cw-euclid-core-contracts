use cosmwasm_std::{to_json_binary, Binary, Deps, Order};
use euclid::{
    error::ContractError,
    msgs::router::{AllChainResponse, AllVlpResponse, ChainResponse, StateResponse, VlpResponse},
    token::Token,
};

use crate::state::{CHAIN_ID_TO_CHAIN, STATE, VLPS};

pub fn query_state(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&StateResponse {
        admin: state.admin,
        vlp_code_id: state.vlp_code_id,
        vcoin_address: state.vcoin_address,
    })?)
}

pub fn query_all_vlps(deps: Deps) -> Result<Binary, ContractError> {
    let vlps: Result<_, ContractError> = VLPS
        .range(deps.storage, None, None, Order::Ascending)
        .map(|v| {
            let v = v?;
            Ok(VlpResponse {
                vlp: v.1,
                token_1: v.0 .0,
                token_2: v.0 .1,
            })
        })
        .collect();

    Ok(to_json_binary(&AllVlpResponse { vlps: vlps? })?)
}

pub fn query_vlp(deps: Deps, token_1: Token, token_2: Token) -> Result<Binary, ContractError> {
    let vlp = VLPS.load(deps.storage, (token_1.clone(), token_2.clone()))?;

    Ok(to_json_binary(&VlpResponse {
        vlp,
        token_1,
        token_2,
    })?)
}

pub fn query_all_chains(deps: Deps) -> Result<Binary, ContractError> {
    let chains: Result<_, ContractError> = CHAIN_ID_TO_CHAIN
        .range(deps.storage, None, None, Order::Ascending)
        .map(|v| {
            let v = v?;
            Ok(ChainResponse { chain: v.1 })
        })
        .collect();

    Ok(to_json_binary(&AllChainResponse { chains: chains? })?)
}

pub fn query_chain(deps: Deps, chain_id: String) -> Result<Binary, ContractError> {
    let chain = CHAIN_ID_TO_CHAIN.load(deps.storage, chain_id)?;
    Ok(to_json_binary(&ChainResponse { chain })?)
}
