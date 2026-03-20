use cosmwasm_std::{ensure, to_json_binary, Binary, Deps, StdError, Uint256};
use cw_storage_plus::Bound;
use euclid::{
    chain::ChainUid,
    error::ContractError,
    msgs::virtual_balance::{
        msg::{
            GetAllEscrowBalancesResponse, GetAllEscrowBalancesResponseItem,
            GetAllTokenMetadataResponse, GetBalanceResponse, GetEscrowBalanceResponse,
            GetTokenEscrowsResponse, GetTokenEscrowsResponseItem, GetTokenMetadataResponse,
            GetUserBalancesResponse, GetUserBalancesResponseItem,
        },
        GetAllBalancesResponse, GetAllBalancesResponseItem, GetTokenBalancesResponse,
        GetTokenBalancesResponseItem, GetTokenMetadataByDenomResponse, GetTokenRegisteredResponse,
    },
    token::TokenType,
    utils::pagination::Pagination,
    voucher::BalanceKey,
};
use std::collections::BTreeMap;

use crate::state::{
    get_escrow_balance_key, ADMIN, ESCROW_BALANCES, STATE, TOKEN_METADATA, VOUCHER_BALANCES,
};

pub fn query_state(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&state)?)
}

pub fn query_admin(deps: Deps) -> Result<Binary, ContractError> {
    let admin = ADMIN.load(deps.storage)?;
    Ok(to_json_binary(&admin)?)
}

pub fn query_balance(deps: Deps, balance_key: BalanceKey) -> Result<Binary, ContractError> {
    let balance = VOUCHER_BALANCES.may_load(
        deps.storage,
        balance_key.clone().to_serialized_balance_key(),
    )?;
    Ok(to_json_binary(&GetBalanceResponse {
        amount: balance.unwrap_or(Uint256::zero()),
    })?)
}

pub fn query_user_balances(
    deps: Deps,
    chain_uid: ChainUid,
    address: String,
    pagination: Option<Pagination<Uint256>>,
) -> Result<Binary, ContractError> {
    let Pagination {
        min,
        max,
        skip,
        limit,
    } = pagination.unwrap_or_default();

    let min = min.map(Bound::inclusive);
    let max = max.map(Bound::exclusive);

    let balances: Result<_, ContractError> = VOUCHER_BALANCES
        .prefix((chain_uid.clone(), address.clone()))
        .range(deps.storage, min, max, cosmwasm_std::Order::Ascending)
        .skip(skip.unwrap_or(0) as usize)
        .take(limit.unwrap_or(10) as usize)
        .map(|res| {
            let res = res?;
            Ok(GetUserBalancesResponseItem {
                token_id: res.0,
                amount: res.1,
            })
        })
        .collect();

    Ok(to_json_binary(&GetUserBalancesResponse {
        balances: balances?,
    })?)
}

pub fn query_all_balances(
    deps: Deps,
    pagination: Option<Pagination<Uint256>>,
) -> Result<Binary, ContractError> {
    let Pagination {
        min,
        max,
        skip,
        limit,
    } = pagination.unwrap_or_default();

    ensure!(
        min.is_none(),
        ContractError::Generic {
            err: "Min is not supported".to_string()
        }
    );
    ensure!(
        max.is_none(),
        ContractError::Generic {
            err: "Max is not supported".to_string()
        }
    );

    let balances: Result<_, ContractError> = VOUCHER_BALANCES
        .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .skip(skip.unwrap_or(0) as usize)
        .take(limit.unwrap_or(10) as usize)
        .map(|res| {
            let ((chain_uid, address, token_id), balance) = res?;
            Ok(GetAllBalancesResponseItem {
                chain_uid,
                address,
                token_id,
                balance,
            })
        })
        .collect();

    Ok(to_json_binary(&GetAllBalancesResponse {
        balances: balances?,
    })?)
}

pub fn query_token_balances(
    deps: Deps,
    token_id: String,
    pagination: Option<Pagination<Uint256>>,
) -> Result<Binary, ContractError> {
    let Pagination {
        min,
        max,
        skip,
        limit,
    } = pagination.unwrap_or_default();

    ensure!(
        min.is_none(),
        ContractError::Generic {
            err: "Min is not supported".to_string()
        }
    );
    ensure!(
        max.is_none(),
        ContractError::Generic {
            err: "Max is not supported".to_string()
        }
    );

    let mut token_balances = BTreeMap::<String, Uint256>::new();

    VOUCHER_BALANCES
        .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .filter(|res| res.is_ok() && res.as_ref().unwrap().0 .2 == token_id)
        .skip(skip.unwrap_or(0) as usize)
        .take(limit.unwrap_or(10) as usize)
        .for_each(|res| {
            let ((chain_uid, _, _), balance) = res.unwrap();

            let existing_balance = token_balances.get(&chain_uid.to_string());
            if let Some(existing_balance) = existing_balance {
                token_balances.insert(
                    chain_uid.to_string(),
                    existing_balance.checked_add(balance).unwrap(),
                );
            } else {
                token_balances.insert(chain_uid.to_string(), balance);
            }
        });

    let balances: Vec<GetTokenBalancesResponseItem> = token_balances
        .iter()
        .map(|(chain_uid, balance)| GetTokenBalancesResponseItem {
            balance: *balance,
            chain_uid: ChainUid::create(chain_uid.clone()).unwrap(),
        })
        .collect();

    Ok(to_json_binary(&GetTokenBalancesResponse { balances })?)
}

pub fn query_escrow_balance(
    deps: Deps,
    token_id: String,
    chain_uid: ChainUid,
    token_type: TokenType,
) -> Result<Binary, ContractError> {
    let escrow_key = get_escrow_balance_key(token_id, chain_uid, token_type);
    let balance = escrow_key
        .may_load(deps.storage)?
        .unwrap_or(Uint256::zero());
    Ok(to_json_binary(&GetEscrowBalanceResponse { balance })?)
}

pub fn query_token_escrows(
    deps: Deps,
    token_id: String,
    pagination: Option<Pagination<(ChainUid, String)>>,
) -> Result<Binary, ContractError> {
    let Pagination {
        skip,
        limit,
        min,
        max,
    } = pagination.unwrap_or_default();

    let escrows: Result<Vec<_>, ContractError> = ESCROW_BALANCES
        .sub_prefix(token_id)
        .range(
            deps.storage,
            min.map(Bound::inclusive),
            max.map(Bound::exclusive),
            cosmwasm_std::Order::Ascending,
        )
        .skip(skip.unwrap_or(0) as usize)
        .take(limit.unwrap_or(10) as usize)
        .map(|res| {
            let (key, balance) = res?;
            let token_type = TokenType::from_key(key.1)?;
            Ok(GetTokenEscrowsResponseItem {
                balance,
                chain_uid: key.0,
                token_type,
            })
        })
        .collect();

    Ok(to_json_binary(&GetTokenEscrowsResponse {
        escrows: escrows?,
    })?)
}

pub fn query_all_escrow_balances(
    deps: Deps,
    pagination: Option<Pagination<(String, ChainUid, String)>>,
) -> Result<Binary, ContractError> {
    let Pagination { skip, limit, .. } = pagination.unwrap_or_default();

    let escrows: Result<Vec<_>, ContractError> = ESCROW_BALANCES
        .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .skip(skip.unwrap_or(0) as usize)
        .take(limit.unwrap_or(10) as usize)
        .map(|res| {
            let (key, balance) = res?;
            let (token_id, chain_uid, token_type) = key;
            Ok(GetAllEscrowBalancesResponseItem {
                token_id,
                chain_uid,
                token_type: TokenType::from_key(token_type)?,
                balance,
            })
        })
        .collect();

    Ok(to_json_binary(&GetAllEscrowBalancesResponse {
        escrows: escrows?,
    })?)
}

pub fn query_token_metadata_by_denom(
    deps: Deps,
    token_id: String,
    chain_uid: ChainUid,
    token_type: TokenType,
) -> Result<Binary, ContractError> {
    let metadata = TOKEN_METADATA
        .load(deps.storage, (token_id, chain_uid, token_type.get_key()))
        .map_err(|e| ContractError::new(&format!("Failed to load token metadata: {}", e)))?;
    Ok(to_json_binary(&GetTokenMetadataByDenomResponse {
        metadata,
    })?)
}

pub fn query_token_metadata(
    deps: Deps,
    token_id: String,
    pagination: Option<Pagination<(ChainUid, TokenType)>>,
) -> Result<Binary, ContractError> {
    let Pagination {
        skip,
        limit,
        min,
        max,
    } = pagination.unwrap_or_default();

    let min = min.map(|(chain_uid, token_type)| (chain_uid, token_type.get_key()));
    let max = max.map(|(chain_uid, token_type)| (chain_uid, token_type.get_key()));

    let metadata: Result<Vec<_>, ContractError> = TOKEN_METADATA
        .sub_prefix(token_id)
        .range(
            deps.storage,
            min.map(Bound::inclusive),
            max.map(Bound::exclusive),
            cosmwasm_std::Order::Ascending,
        )
        .skip(skip.unwrap_or(0) as usize)
        .take(limit.unwrap_or(10) as usize)
        .map(|res| {
            let (_, metadata) = res?;
            Ok(metadata)
        })
        .collect();

    Ok(to_json_binary(&GetTokenMetadataResponse {
        metadata: metadata?,
    })?)
}

pub fn query_all_token_metadata(
    deps: Deps,
    pagination: Option<Pagination<(String, ChainUid, String)>>,
) -> Result<Binary, ContractError> {
    let Pagination { skip, limit, .. } = pagination.unwrap_or_default();

    let metadata: Result<Vec<_>, ContractError> = TOKEN_METADATA
        .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .skip(skip.unwrap_or(0) as usize)
        .take(limit.unwrap_or(10) as usize)
        .map(|res| {
            let (_, metadata) = res?;
            Ok(metadata)
        })
        .collect();

    Ok(to_json_binary(&GetAllTokenMetadataResponse {
        metadata: metadata?,
    })?)
}

pub fn query_token_registered(deps: Deps, token_id: String) -> Result<Binary, ContractError> {
    let token_metadatas: Result<Vec<_>, StdError> = TOKEN_METADATA
        .sub_prefix(token_id)
        .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .take(1)
        .collect();
    Ok(to_json_binary(&GetTokenRegisteredResponse {
        token_registered: !token_metadatas?.is_empty(),
    })?)
}
