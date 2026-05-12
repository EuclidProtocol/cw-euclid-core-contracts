use cosmwasm_std::{ensure, to_json_binary, Binary, Deps, StdError, Uint256};
use cw_storage_plus::Bound;
use euclid::{
    chain::ChainUid,
    error::ContractError,
    msgs::virtual_balance::{
        msg::{
            GetAllEscrowBalancesResponse, GetAllEscrowBalancesResponseItem,
            GetAllTokenMetadataResponse, GetAllowanceResponse, GetBalanceResponse,
            GetEscrowBalanceResponse, GetTokenEscrowsResponse, GetTokenEscrowsResponseItem,
            GetTokenMetadataResponse, GetUserBalancesResponse, GetUserBalancesResponseItem,
        },
        GetAllBalancesResponse, GetAllBalancesResponseItem, GetTokenBalancesResponse,
        GetTokenBalancesResponseItem, GetTokenMetadataByDenomResponse, GetTokenStatusResponse,
    },
    token::TokenType,
    utils::pagination::Pagination,
    voucher::BalanceKey,
};
use std::collections::BTreeMap;

use crate::state::{
    get_escrow_balance_key, ADMIN, ESCROW_BALANCES, STATE, TOKEN_METADATA, VOUCHER_ALLOWANCES,
    VOUCHER_BALANCES,
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

pub fn query_allowance(deps: Deps, balance_key: BalanceKey) -> Result<Binary, ContractError> {
    let allowance = VOUCHER_ALLOWANCES
        .may_load(
            deps.storage,
            balance_key.clone().to_serialized_balance_key(),
        )?
        .ok_or(ContractError::NoAllowance {})?;
    Ok(to_json_binary(&GetAllowanceResponse { allowance })?)
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

    for res in VOUCHER_BALANCES
        .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .skip(skip.unwrap_or(0) as usize)
        .take(limit.unwrap_or(10) as usize)
    {
        let ((chain_uid, _, token), balance) = res?;
        if token != token_id {
            continue;
        }

        let existing_balance = token_balances.get(&chain_uid.to_string());
        if let Some(existing_balance) = existing_balance {
            token_balances.insert(
                chain_uid.to_string(),
                existing_balance.checked_add(balance)?,
            );
        } else {
            token_balances.insert(chain_uid.to_string(), balance);
        }
    }

    let balances: Vec<GetTokenBalancesResponseItem> = token_balances
        .iter()
        .map(|(chain_uid, balance)| {
            Ok(GetTokenBalancesResponseItem {
                balance: *balance,
                chain_uid: ChainUid::create(chain_uid.clone())?,
            })
        })
        .collect::<Result<Vec<_>, ContractError>>()?;

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

pub fn query_token_status(deps: Deps, token_id: String) -> Result<Binary, ContractError> {
    let token_metadatas: Result<Vec<_>, StdError> = TOKEN_METADATA
        .sub_prefix(token_id)
        .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .take(1)
        .collect();
    Ok(to_json_binary(&GetTokenStatusResponse {
        registered: !token_metadatas?.is_empty(),
    })?)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::helpers::{
        init, remote_user, seed_allowance, seed_balance, vsl_user, MockDeps,
    };
    use cosmwasm_std::{from_json, testing::mock_dependencies};
    use euclid::msgs::virtual_balance::msg::{
        GetAllowanceResponse, GetBalanceResponse, GetUserBalancesResponse,
    };
    use euclid::utils::pagination::Pagination;
    use euclid::voucher::BalanceKey;

    // -----------------------------------------------------------------------
    // query_state
    // -----------------------------------------------------------------------

    #[test]
    fn test_query_state_returns_router() {
        let mut deps: MockDeps = mock_dependencies();
        init(&mut deps);
        let router = deps.api.addr_make("router");
        let bin = query_state(deps.as_ref()).unwrap();
        let state: euclid::msgs::virtual_balance::msg::State = from_json(&bin).unwrap();
        assert_eq!(state.router, router);
    }

    // -----------------------------------------------------------------------
    // query_admin
    // -----------------------------------------------------------------------

    #[test]
    fn test_query_admin_returns_default_admin() {
        let mut deps: MockDeps = mock_dependencies();
        init(&mut deps);
        let router = deps.api.addr_make("router");
        let bin = query_admin(deps.as_ref()).unwrap();
        let admin: euclid::admin::EuclidAdmin = from_json(&bin).unwrap();
        assert_eq!(admin, euclid::admin::EuclidAdmin::default(router));
    }

    // -----------------------------------------------------------------------
    // query_balance
    // -----------------------------------------------------------------------

    #[test]
    fn test_query_balance_returns_seeded_amount() {
        let mut deps: MockDeps = mock_dependencies();
        init(&mut deps);
        let user = remote_user("1", "cosmos1alice");
        seed_balance(&mut deps, user.clone(), "eucl", 750);

        let bk = BalanceKey {
            cross_chain_user: user,
            token_id: "eucl".to_string(),
        };
        let bin = query_balance(deps.as_ref(), bk).unwrap();
        let resp: GetBalanceResponse = from_json(&bin).unwrap();
        assert_eq!(resp.amount, cosmwasm_std::Uint256::from(750u128));
    }

    #[test]
    fn test_query_balance_returns_zero_when_no_entry() {
        let mut deps: MockDeps = mock_dependencies();
        init(&mut deps);
        let bk = BalanceKey {
            cross_chain_user: remote_user("1", "cosmos1nobody"),
            token_id: "eucl".to_string(),
        };
        let bin = query_balance(deps.as_ref(), bk).unwrap();
        let resp: GetBalanceResponse = from_json(&bin).unwrap();
        assert_eq!(resp.amount, cosmwasm_std::Uint256::zero());
    }

    // -----------------------------------------------------------------------
    // query_allowance
    // -----------------------------------------------------------------------

    #[test]
    fn test_query_allowance_returns_allowance() {
        let mut deps: MockDeps = mock_dependencies();
        init(&mut deps);
        let owner = vsl_user("alice");
        let spender = vsl_user("bob");
        seed_allowance(&mut deps, owner.clone(), "eucl", spender.clone(), 300);

        let bk = BalanceKey {
            cross_chain_user: owner,
            token_id: "eucl".to_string(),
        };
        let bin = query_allowance(deps.as_ref(), bk).unwrap();
        let resp: GetAllowanceResponse = from_json(&bin).unwrap();
        assert_eq!(resp.allowance.amount, cosmwasm_std::Uint256::from(300u128));
        assert_eq!(resp.allowance.spender, spender);
    }

    #[test]
    fn test_query_allowance_no_allowance_returns_error() {
        let mut deps: MockDeps = mock_dependencies();
        init(&mut deps);
        let bk = BalanceKey {
            cross_chain_user: vsl_user("nobody"),
            token_id: "eucl".to_string(),
        };
        let err = query_allowance(deps.as_ref(), bk).unwrap_err();
        assert!(matches!(err, ContractError::NoAllowance {}));
    }

    // -----------------------------------------------------------------------
    // query_user_balances
    // -----------------------------------------------------------------------

    #[test]
    fn test_query_user_balances_returns_all_tokens_for_user() {
        let mut deps: MockDeps = mock_dependencies();
        init(&mut deps);
        let user = vsl_user("alice");
        seed_balance(&mut deps, user.clone(), "eucl", 100);
        seed_balance(&mut deps, user.clone(), "usdc", 200);
        // Different user — should not appear
        seed_balance(&mut deps, vsl_user("bob"), "eucl", 999);

        let bin = query_user_balances(
            deps.as_ref(),
            user.chain_uid.clone(),
            user.address.clone(),
            None,
        )
        .unwrap();
        let resp: GetUserBalancesResponse = from_json(&bin).unwrap();
        assert_eq!(resp.balances.len(), 2);
        // Items are in ascending token_id order ("eucl" < "usdc")
        assert_eq!(resp.balances[0].token_id, "eucl");
        assert_eq!(
            resp.balances[0].amount,
            cosmwasm_std::Uint256::from(100u128)
        );
        assert_eq!(resp.balances[1].token_id, "usdc");
        assert_eq!(
            resp.balances[1].amount,
            cosmwasm_std::Uint256::from(200u128)
        );
    }

    #[test]
    fn test_query_user_balances_empty_for_unknown_user() {
        let mut deps: MockDeps = mock_dependencies();
        init(&mut deps);
        let user = vsl_user("nobody");
        let bin = query_user_balances(deps.as_ref(), user.chain_uid, user.address, None).unwrap();
        let resp: GetUserBalancesResponse = from_json(&bin).unwrap();
        assert!(resp.balances.is_empty());
    }

    #[test]
    fn test_query_user_balances_pagination_limit() {
        let mut deps: MockDeps = mock_dependencies();
        init(&mut deps);
        let user = vsl_user("alice");
        seed_balance(&mut deps, user.clone(), "aaaa", 10);
        seed_balance(&mut deps, user.clone(), "bbbb", 20);
        seed_balance(&mut deps, user.clone(), "cccc", 30);

        let pagination = Some(Pagination {
            min: None,
            max: None,
            skip: None,
            limit: Some(2),
        });
        let bin =
            query_user_balances(deps.as_ref(), user.chain_uid, user.address, pagination).unwrap();
        let resp: GetUserBalancesResponse = from_json(&bin).unwrap();
        assert_eq!(resp.balances.len(), 2);
    }

    // -----------------------------------------------------------------------
    // query_all_balances
    // -----------------------------------------------------------------------

    #[test]
    fn test_query_all_balances_returns_all_entries() {
        let mut deps: MockDeps = mock_dependencies();
        init(&mut deps);
        seed_balance(&mut deps, vsl_user("alice"), "eucl", 100);
        seed_balance(&mut deps, remote_user("1", "cosmos1eve"), "usdc", 200);

        let bin = query_all_balances(deps.as_ref(), None).unwrap();
        let resp: GetAllBalancesResponse = from_json(&bin).unwrap();
        // Default limit is 10, we have 2 entries
        assert_eq!(resp.balances.len(), 2);
    }

    #[test]
    fn test_query_all_balances_min_returns_error() {
        let mut deps: MockDeps = mock_dependencies();
        init(&mut deps);
        let pagination = Some(Pagination {
            min: Some(cosmwasm_std::Uint256::zero()),
            max: None,
            skip: None,
            limit: None,
        });
        let err = query_all_balances(deps.as_ref(), pagination).unwrap_err();
        assert!(matches!(err, ContractError::Generic { .. }));
    }

    #[test]
    fn test_query_all_balances_max_returns_error() {
        let mut deps: MockDeps = mock_dependencies();
        init(&mut deps);
        let pagination = Some(Pagination {
            min: None,
            max: Some(cosmwasm_std::Uint256::from(100u128)),
            skip: None,
            limit: None,
        });
        let err = query_all_balances(deps.as_ref(), pagination).unwrap_err();
        assert!(matches!(err, ContractError::Generic { .. }));
    }

    #[test]
    fn test_query_all_balances_empty_when_no_entries() {
        let mut deps: MockDeps = mock_dependencies();
        init(&mut deps);
        let bin = query_all_balances(deps.as_ref(), None).unwrap();
        let resp: GetAllBalancesResponse = from_json(&bin).unwrap();
        assert!(resp.balances.is_empty());
    }

    // -----------------------------------------------------------------------
    // query_token_balances
    // -----------------------------------------------------------------------

    #[test]
    fn test_query_token_balances_aggregates_by_chain() {
        let mut deps: MockDeps = mock_dependencies();
        init(&mut deps);
        // Two users on different chains holding the same token
        seed_balance(&mut deps, vsl_user("alice"), "eucl", 100);
        seed_balance(&mut deps, remote_user("1", "cosmos1eve"), "eucl", 200);
        // A different token — must not appear
        seed_balance(&mut deps, vsl_user("bob"), "usdc", 999);

        let bin = query_token_balances(deps.as_ref(), "eucl".to_string(), None).unwrap();
        let resp: GetTokenBalancesResponse = from_json(&bin).unwrap();

        // Two chains: vsl and 1
        assert_eq!(resp.balances.len(), 2);
        let total: cosmwasm_std::Uint256 = resp.balances.iter().map(|b| b.balance).sum();
        assert_eq!(total, cosmwasm_std::Uint256::from(300u128));
    }

    #[test]
    fn test_query_token_balances_empty_for_unknown_token() {
        let mut deps: MockDeps = mock_dependencies();
        init(&mut deps);
        seed_balance(&mut deps, vsl_user("alice"), "eucl", 100);

        let bin = query_token_balances(deps.as_ref(), "unknown".to_string(), None).unwrap();
        let resp: GetTokenBalancesResponse = from_json(&bin).unwrap();
        assert!(resp.balances.is_empty());
    }

    #[test]
    fn test_query_token_balances_aggregates_multiple_users_same_chain() {
        let mut deps: MockDeps = mock_dependencies();
        init(&mut deps);
        // Two vsl-chain users holding eucl
        seed_balance(&mut deps, vsl_user("alice"), "eucl", 300);
        seed_balance(&mut deps, vsl_user("bob"), "eucl", 200);

        let bin = query_token_balances(deps.as_ref(), "eucl".to_string(), None).unwrap();
        let resp: GetTokenBalancesResponse = from_json(&bin).unwrap();
        // Only one chain (vsl) with combined balance
        assert_eq!(resp.balances.len(), 1);
        assert_eq!(
            resp.balances[0].balance,
            cosmwasm_std::Uint256::from(500u128)
        );
    }

    #[test]
    fn test_query_token_balances_min_returns_error() {
        let mut deps: MockDeps = mock_dependencies();
        init(&mut deps);
        let pagination = Some(Pagination {
            min: Some(cosmwasm_std::Uint256::zero()),
            max: None,
            skip: None,
            limit: None,
        });
        let err = query_token_balances(deps.as_ref(), "eucl".to_string(), pagination).unwrap_err();
        assert!(matches!(err, ContractError::Generic { .. }));
    }
}
