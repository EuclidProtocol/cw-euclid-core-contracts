use cosmwasm_std::{to_json_binary, Binary, Deps};
use euclid::{
    error::ContractError,
    msgs::pool_factory::{GetLpTokenResponse, GetVlpResponse, MainFactoryAddressResponse},
    token::Pair,
};

use crate::state::{MAIN_FACTORY_ADDRESS, PAIR_TO_VLP, VLP_TO_LP_TOKEN};

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
