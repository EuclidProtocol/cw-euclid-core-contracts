use cosmwasm_std::{DepsMut, Uint256};

use crate::{
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    error::ContractError,
    msgs::virtual_balance::msg::{
        GetBalanceResponse, GetTokenMetadataResponse, QueryMsg as VirtualBalanceQueryMsg,
    },
    token::Token,
    utils::pagination::Pagination,
    voucher::BalanceKey,
};

pub fn query_token_decimals(
    deps: &DepsMut,
    virtual_balance_addr: &str,
    token: &Token,
) -> Result<u32, ContractError> {
    let resp: GetTokenMetadataResponse = deps.querier.query_wasm_smart(
        virtual_balance_addr,
        &VirtualBalanceQueryMsg::GetTokenMetadata {
            token_id: token.to_string(),
            pagination: Some(Pagination::new(None, None, None, Some(u64::MAX))),
        },
    )?;
    let first = resp.metadata.first().ok_or_else(|| {
        ContractError::new(&format!(
            "No token metadata found for '{}' in virtual_balance. Migrate virtual_balance first.",
            token
        ))
    })?;
    let decimals = first.token_type.get_decimals()?;
    for entry in resp.metadata.iter().skip(1) {
        let entry_decimals = entry.token_type.get_decimals()?;
        if entry_decimals != decimals {
            return Err(ContractError::new(&format!(
                "Token '{}' has inconsistent decimals across chains: {} vs {}",
                token, decimals, entry_decimals
            )));
        }
    }
    Ok(decimals)
}

pub fn query_voucher_balance(
    deps: &DepsMut,
    virtual_balance_addr: &str,
    holder_address: &str,
    token: &Token,
) -> Result<Uint256, ContractError> {
    let balance_key = BalanceKey {
        cross_chain_user: CrossChainUser::new(
            ChainUid::vsl_chain_uid()?,
            holder_address.to_string(),
        ),
        token_id: token.to_string(),
    };
    let resp: GetBalanceResponse = deps.querier.query_wasm_smart(
        virtual_balance_addr,
        &VirtualBalanceQueryMsg::GetBalance { balance_key },
    )?;
    Ok(resp.amount)
}
