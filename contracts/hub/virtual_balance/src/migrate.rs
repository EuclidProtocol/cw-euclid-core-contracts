use crate::contract::{CONTRACT_NAME, CONTRACT_VERSION};
#[allow(deprecated)]
use crate::state::{
    get_escrow_balance_key, get_token_metadata_key, ALLOWANCES, BALANCES, STATE,
    VOUCHER_ALLOWANCES, VOUCHER_BALANCES,
};
use cosmwasm_std::{ensure, entry_point, DepsMut, Env, Order, Response, Uint256};
use cw2::set_contract_version;
use euclid::{
    error::ContractError,
    msgs::{
        router::{AllEscrowsResponse, QueryMsg as RouterQueryMsg},
        virtual_balance::msg::{MigrateMsg, VoucherAllowance},
    },
    normalize::normalize_token_to_voucher,
    token::TokenMetadata,
};
use std::collections::HashMap;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, msg: MigrateMsg) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;

    // Phase 1: Seed TOKEN_METADATA and build token_id → decimals lookup
    let mut decimals_map: HashMap<String, u32> = HashMap::new();
    let mut metadata_count = 0u64;
    for metadata in &msg.token_metadata {
        let key = get_token_metadata_key(
            metadata.token.to_string(),
            metadata.chain_uid.clone(),
            metadata.token_type.clone(),
        );
        key.save(deps.storage, metadata)?;
        let decimals = metadata.token_type.get_decimals()?;
        match decimals_map.entry(metadata.token.to_string()) {
            std::collections::hash_map::Entry::Occupied(e) => {
                ensure!(
                    *e.get() == decimals,
                    ContractError::new(&format!(
                        "Token '{}' has conflicting decimals: {} vs {}",
                        metadata.token,
                        e.get(),
                        decimals
                    ))
                );
            }
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(decimals);
            }
        }
        metadata_count += 1;
    }

    // Build in-memory lookup from Phase 1 data to avoid repeated storage range scans
    let mut metadata_lookup: HashMap<(String, String), Vec<TokenMetadata>> = HashMap::new();
    for metadata in &msg.token_metadata {
        metadata_lookup
            .entry((metadata.token.to_string(), metadata.chain_uid.to_string()))
            .or_default()
            .push(metadata.clone());
    }

    // Phase 2: Query router for escrow balances and seed ESCROW_BALANCES
    #[allow(deprecated)]
    let escrow_resp: AllEscrowsResponse = deps
        .querier
        .query_wasm_smart(state.router.to_string(), &RouterQueryMsg::GetAllEscrows {})?;

    let mut escrow_count = 0u64;
    for escrow in &escrow_resp.escrows {
        let token_id = escrow.token.to_string();
        let chain_uid = escrow.chain_uid.clone();

        let lookup_key = (token_id.clone(), chain_uid.to_string());
        let metadata_entries = metadata_lookup.get(&lookup_key);

        let entries = metadata_entries.ok_or_else(|| {
            ContractError::new(&format!(
                "No TOKEN_METADATA for token '{}' on chain '{:?}'",
                token_id, chain_uid
            ))
        })?;

        ensure!(
            entries.len() == 1,
            ContractError::new(&format!(
                "Multiple TOKEN_METADATA for token '{}' on chain '{:?}' (found {}). Cannot determine which token_type owns the escrow balance.",
                token_id, chain_uid, entries.len()
            ))
        );
        let metadata = &entries[0];

        let key = get_escrow_balance_key(token_id, chain_uid, metadata.token_type.clone());
        if key.may_load(deps.storage)?.is_some() {
            continue;
        }
        key.save(deps.storage, &escrow.balance)?;
        escrow_count += 1;
    }

    // Phase 3: Migrate BALANCES → VOUCHER_BALANCES (with normalization)
    #[allow(deprecated)]
    let old_balances: Vec<_> = BALANCES
        .range(deps.storage, None, None, Order::Ascending)
        .collect::<Result<Vec<_>, _>>()?;

    let mut balances_migrated = 0u64;
    for (key, old_amount) in old_balances {
        let token_id = &key.2;
        let decimals = decimals_map.get(token_id).ok_or_else(|| {
            ContractError::new(&format!(
                "Missing decimals for token '{}' in token_metadata",
                token_id
            ))
        })?;
        let normalized = normalize_token_to_voucher(Uint256::from(old_amount), *decimals)?;
        VOUCHER_BALANCES.save(deps.storage, key.clone(), &normalized)?;
        #[allow(deprecated)]
        BALANCES.remove(deps.storage, key);
        balances_migrated += 1;
    }

    // Phase 4: Migrate ALLOWANCES → VOUCHER_ALLOWANCES (normalize + remove old)
    #[allow(deprecated)]
    let old_allowances: Vec<_> = ALLOWANCES
        .range(deps.storage, None, None, Order::Ascending)
        .collect::<Result<Vec<_>, _>>()?;

    let mut allowances_migrated = 0u64;
    for (key, old_allowance) in old_allowances {
        let token_id = &key.2;
        let decimals = decimals_map.get(token_id).ok_or_else(|| {
            ContractError::new(&format!(
                "Missing decimals for token '{}' in token_metadata (allowance migration)",
                token_id
            ))
        })?;
        let normalized_amount =
            normalize_token_to_voucher(Uint256::from(old_allowance.amount), *decimals)?;
        VOUCHER_ALLOWANCES.save(
            deps.storage,
            key.clone(),
            &VoucherAllowance {
                spender: old_allowance.spender,
                amount: normalized_amount,
                expires_at: None,
            },
        )?;
        #[allow(deprecated)]
        ALLOWANCES.remove(deps.storage, key);
        allowances_migrated += 1;
    }

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::new()
        .add_attribute("method", "migrate")
        .add_attribute("metadata_seeded", metadata_count.to_string())
        .add_attribute("escrow_seeded", escrow_count.to_string())
        .add_attribute("balances_migrated", balances_migrated.to_string())
        .add_attribute("allowances_migrated", allowances_migrated.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(deprecated)]
    use crate::state::{get_escrow_balance_key, Allowance, ADMIN, ALLOWANCES, BALANCES};
    use cosmwasm_std::testing::{mock_env, MockQuerier};
    use cosmwasm_std::{
        from_json, to_json_binary, Addr, ContractResult, QuerierResult, SystemResult, Uint128,
        WasmQuery,
    };
    use cw2::get_contract_version;
    use euclid::{
        admin::EuclidAdmin,
        chain::ChainUid,
        cross_chain_user::CrossChainUser,
        msgs::{
            router::{AllEscrowsResponse, EscrowResponse, QueryMsg as RouterQueryMsg},
            virtual_balance::msg::State,
        },
        token::{Token, TokenMetadata, TokenType},
        voucher::BalanceKey,
    };

    fn mock_querier_with_escrows(escrows: Vec<EscrowResponse>) -> MockQuerier {
        let mut querier = MockQuerier::default();
        querier.update_wasm(move |query| -> QuerierResult {
            match query {
                WasmQuery::Smart { msg, .. } => {
                    let query_msg: RouterQueryMsg = from_json(msg).unwrap();
                    #[allow(deprecated)]
                    match query_msg {
                        RouterQueryMsg::GetAllEscrows {} => {
                            let resp = AllEscrowsResponse {
                                escrows: escrows.clone(),
                            };
                            SystemResult::Ok(ContractResult::Ok(to_json_binary(&resp).unwrap()))
                        }
                        _ => SystemResult::Ok(ContractResult::Err("Unknown query".to_string())),
                    }
                }
                _ => SystemResult::Ok(ContractResult::Err("Unknown query type".to_string())),
            }
        });
        querier
    }

    fn make_deps(
        escrows: Vec<EscrowResponse>,
    ) -> cosmwasm_std::OwnedDeps<
        cosmwasm_std::testing::MockStorage,
        cosmwasm_std::testing::MockApi,
        MockQuerier,
    > {
        let querier = mock_querier_with_escrows(escrows);
        let mut deps = cosmwasm_std::OwnedDeps {
            storage: cosmwasm_std::testing::MockStorage::default(),
            api: cosmwasm_std::testing::MockApi::default(),
            querier,
            custom_query_type: std::marker::PhantomData,
        };
        let admin = deps.api.addr_make("admin");
        STATE
            .save(
                deps.as_mut().storage,
                &State {
                    router: Addr::unchecked("router"),
                },
            )
            .unwrap();
        ADMIN
            .save(deps.as_mut().storage, &EuclidAdmin::default(admin))
            .unwrap();
        deps
    }

    fn empty_msg() -> MigrateMsg {
        MigrateMsg {
            token_metadata: vec![],
        }
    }

    #[test]
    fn test_migrate_sets_contract_version() {
        let mut deps = make_deps(vec![]);
        migrate(deps.as_mut(), mock_env(), empty_msg()).unwrap();
        let version = get_contract_version(deps.as_ref().storage).unwrap();
        assert_eq!(version.contract, CONTRACT_NAME);
        assert_eq!(version.version, CONTRACT_VERSION);
    }

    #[test]
    fn test_migrate_seeds_token_metadata() {
        let mut deps = make_deps(vec![]);

        let token = Token::create("usdc".to_string()).unwrap();
        let chain = ChainUid::create("osmosis".to_string()).unwrap();
        let token_type = TokenType::Native {
            denom: "uusdc".to_string(),
            decimals: Some(6),
        };
        let metadata = TokenMetadata::new(token.clone(), chain.clone(), token_type.clone());

        let msg = MigrateMsg {
            token_metadata: vec![metadata.clone()],
        };
        let res = migrate(deps.as_mut(), mock_env(), msg).unwrap();
        assert_eq!(
            res.attributes
                .iter()
                .find(|a| a.key == "metadata_seeded")
                .map(|a| a.value.as_str()),
            Some("1")
        );

        let key = get_token_metadata_key(token.to_string(), chain, token_type);
        assert_eq!(key.load(deps.as_ref().storage).unwrap(), metadata);
    }

    #[test]
    fn test_migrate_queries_router_for_escrow_balances() {
        let token = Token::create("usdc".to_string()).unwrap();
        let chain = ChainUid::create("osmosis".to_string()).unwrap();
        let token_type = TokenType::Native {
            denom: "uusdc".to_string(),
            decimals: Some(6),
        };

        let mut deps = make_deps(vec![EscrowResponse {
            token: token.clone(),
            chain_uid: chain.clone(),
            balance: Uint256::from(1_000_000u128),
        }]);

        let metadata = TokenMetadata::new(token.clone(), chain.clone(), token_type.clone());
        let msg = MigrateMsg {
            token_metadata: vec![metadata],
        };
        let res = migrate(deps.as_mut(), mock_env(), msg).unwrap();
        assert_eq!(
            res.attributes
                .iter()
                .find(|a| a.key == "escrow_seeded")
                .map(|a| a.value.as_str()),
            Some("1")
        );

        let key = get_escrow_balance_key("usdc".to_string(), chain, token_type);
        assert_eq!(
            key.load(deps.as_ref().storage).unwrap(),
            Uint256::from(1_000_000u128)
        );
    }

    #[test]
    fn test_migrate_fails_escrow_without_metadata() {
        let token = Token::create("usdc".to_string()).unwrap();
        let chain = ChainUid::create("osmosis".to_string()).unwrap();

        let mut deps = make_deps(vec![EscrowResponse {
            token,
            chain_uid: chain,
            balance: Uint256::from(1_000_000u128),
        }]);

        // No metadata provided, but router returns escrow for this token+chain
        assert!(migrate(deps.as_mut(), mock_env(), empty_msg()).is_err());
    }

    #[test]
    #[allow(deprecated)]
    fn test_migrate_normalizes_balances() {
        let mut deps = make_deps(vec![]);

        let chain = ChainUid::create("osmosis".to_string()).unwrap();
        let user = CrossChainUser::new(chain, "user1".to_string());
        let balance_key = BalanceKey {
            cross_chain_user: user,
            token_id: "usdc".to_string(),
        }
        .to_serialized_balance_key();

        BALANCES
            .save(
                deps.as_mut().storage,
                balance_key.clone(),
                &Uint128::from(1_000_000u128),
            )
            .unwrap();

        let msg = MigrateMsg {
            token_metadata: vec![TokenMetadata::new(
                Token::create("usdc".to_string()).unwrap(),
                ChainUid::create("osmosis".to_string()).unwrap(),
                TokenType::Native {
                    denom: "uusdc".to_string(),
                    decimals: Some(6),
                },
            )],
        };
        let res = migrate(deps.as_mut(), mock_env(), msg).unwrap();
        assert_eq!(
            res.attributes
                .iter()
                .find(|a| a.key == "balances_migrated")
                .map(|a| a.value.as_str()),
            Some("1")
        );

        assert!(BALANCES
            .may_load(deps.as_ref().storage, balance_key.clone())
            .unwrap()
            .is_none());

        assert_eq!(
            VOUCHER_BALANCES
                .load(deps.as_ref().storage, balance_key)
                .unwrap(),
            Uint256::from(1_000_000_000_000_000_000_000_000u128)
        );
    }

    #[test]
    #[allow(deprecated)]
    fn test_migrate_fails_missing_decimals_for_balance() {
        let mut deps = make_deps(vec![]);

        let chain = ChainUid::create("osmosis".to_string()).unwrap();
        let user = CrossChainUser::new(chain, "user1".to_string());
        let balance_key = BalanceKey {
            cross_chain_user: user,
            token_id: "unknown_token".to_string(),
        }
        .to_serialized_balance_key();

        BALANCES
            .save(deps.as_mut().storage, balance_key, &Uint128::from(100u128))
            .unwrap();

        assert!(migrate(deps.as_mut(), mock_env(), empty_msg()).is_err());
    }

    #[test]
    #[allow(deprecated)]
    fn test_migrate_idempotent() {
        let mut deps = make_deps(vec![]);

        let chain = ChainUid::create("osmosis".to_string()).unwrap();
        let user = CrossChainUser::new(chain, "user1".to_string());
        let balance_key = BalanceKey {
            cross_chain_user: user,
            token_id: "usdc".to_string(),
        }
        .to_serialized_balance_key();

        BALANCES
            .save(
                deps.as_mut().storage,
                balance_key.clone(),
                &Uint128::from(1_000_000u128),
            )
            .unwrap();

        let msg = MigrateMsg {
            token_metadata: vec![TokenMetadata::new(
                Token::create("usdc".to_string()).unwrap(),
                ChainUid::create("osmosis".to_string()).unwrap(),
                TokenType::Native {
                    denom: "uusdc".to_string(),
                    decimals: Some(6),
                },
            )],
        };
        migrate(deps.as_mut(), mock_env(), msg.clone()).unwrap();

        let res = migrate(deps.as_mut(), mock_env(), msg).unwrap();
        assert_eq!(
            res.attributes
                .iter()
                .find(|a| a.key == "balances_migrated")
                .map(|a| a.value.as_str()),
            Some("0")
        );

        assert_eq!(
            VOUCHER_BALANCES
                .load(deps.as_ref().storage, balance_key)
                .unwrap(),
            Uint256::from(1_000_000_000_000_000_000_000_000u128)
        );
    }

    #[test]
    fn test_migrate_fails_conflicting_decimals() {
        let mut deps = make_deps(vec![]);

        let msg = MigrateMsg {
            token_metadata: vec![
                TokenMetadata::new(
                    Token::create("usdc".to_string()).unwrap(),
                    ChainUid::create("osmosis".to_string()).unwrap(),
                    TokenType::Native {
                        denom: "uusdc".to_string(),
                        decimals: Some(6),
                    },
                ),
                TokenMetadata::new(
                    Token::create("usdc".to_string()).unwrap(),
                    ChainUid::create("ethereum".to_string()).unwrap(),
                    TokenType::Native {
                        denom: "usdc".to_string(),
                        decimals: Some(18),
                    },
                ),
            ],
        };
        let err = migrate(deps.as_mut(), mock_env(), msg).unwrap_err();
        assert!(err.to_string().contains("conflicting decimals"));
    }

    #[test]
    #[allow(deprecated)]
    fn test_migrate_normalizes_allowances_and_removes_old() {
        let mut deps = make_deps(vec![]);

        let chain = ChainUid::create("osmosis".to_string()).unwrap();
        let owner = CrossChainUser::new(chain.clone(), "owner1".to_string());
        let spender = CrossChainUser::new(chain, "spender1".to_string());
        let balance_key = BalanceKey {
            cross_chain_user: owner,
            token_id: "usdc".to_string(),
        }
        .to_serialized_balance_key();

        ALLOWANCES
            .save(
                deps.as_mut().storage,
                balance_key.clone(),
                &Allowance {
                    spender: spender.clone(),
                    amount: Uint128::from(500_000u128),
                },
            )
            .unwrap();

        let msg = MigrateMsg {
            token_metadata: vec![TokenMetadata::new(
                Token::create("usdc".to_string()).unwrap(),
                ChainUid::create("osmosis".to_string()).unwrap(),
                TokenType::Native {
                    denom: "uusdc".to_string(),
                    decimals: Some(6),
                },
            )],
        };
        let res = migrate(deps.as_mut(), mock_env(), msg).unwrap();
        assert_eq!(
            res.attributes
                .iter()
                .find(|a| a.key == "allowances_migrated")
                .map(|a| a.value.as_str()),
            Some("1")
        );

        assert!(ALLOWANCES
            .may_load(deps.as_ref().storage, balance_key.clone())
            .unwrap()
            .is_none());

        let voucher_allowance = VOUCHER_ALLOWANCES
            .load(deps.as_ref().storage, balance_key)
            .unwrap();
        assert_eq!(
            voucher_allowance.amount,
            Uint256::from(500_000_000_000_000_000_000_000u128)
        );
        assert_eq!(voucher_allowance.spender, spender);
        assert_eq!(voucher_allowance.expires_at, None);
    }
}
