use cosmwasm_std::{to_json_binary, Binary, Deps};
use euclid::{
    error::ContractError,
    msgs::{
        pool_factory::{
            GetConcentratedVlpResponse, GetLpTokenResponse, GetVlpResponse,
            MainFactoryAddressResponse, PositionTokenContractResponse,
        },
        vlp::base::PoolKey,
    },
    token::Pair,
};

use crate::state::{
    CONCENTRATED_VLPS, MAIN_FACTORY_ADDRESS, PAIR_TO_VLP, POSITION_TOKEN_CONTRACT, VLP_TO_LP_TOKEN,
};

pub fn get_vlp(deps: Deps, pair: Pair) -> Result<Binary, ContractError> {
    let vlp_address = PAIR_TO_VLP.may_load(deps.storage, pair.get_tupple())?;
    Ok(to_json_binary(&GetVlpResponse { vlp_address })?)
}

pub fn get_lp_token(deps: Deps, vlp: String) -> Result<Binary, ContractError> {
    let token_address = VLP_TO_LP_TOKEN.may_load(deps.storage, vlp)?;
    Ok(to_json_binary(&GetLpTokenResponse { token_address })?)
}

pub fn get_main_factory_address(deps: Deps) -> Result<Binary, ContractError> {
    let main_factory_address = MAIN_FACTORY_ADDRESS.load(deps.storage)?;
    Ok(to_json_binary(&MainFactoryAddressResponse {
        main_factory_address,
    })?)
}

pub fn get_concentrated_vlp(deps: Deps, pool_key: PoolKey) -> Result<Binary, ContractError> {
    let vlp_address = CONCENTRATED_VLPS.may_load(deps.storage, pool_key.to_map_key())?;
    Ok(to_json_binary(&GetConcentratedVlpResponse {
        vlp_address,
        pool_key,
    })?)
}

pub fn get_position_token_contract(deps: Deps) -> Result<Binary, ContractError> {
    let position_token_contract = POSITION_TOKEN_CONTRACT.may_load(deps.storage)?;
    Ok(to_json_binary(&PositionTokenContractResponse {
        position_token_contract,
    })?)
}
