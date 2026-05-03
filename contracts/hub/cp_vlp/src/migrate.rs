use cosmwasm_std::{ensure, entry_point, DepsMut, Env, Response};
use cw2::set_contract_version;
use euclid::{
    error::ContractError,
    msgs::vlp::cp::msg::MigrateMsg,
    normalize::normalize_token_to_voucher,
    utils::migration::{query_token_decimals, query_voucher_balance},
};

use crate::state::{BALANCES, STATE};

const CONTRACT_NAME: &str = "crates.io:vlp";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    let pair_tokens = [state.pair.token_1.clone(), state.pair.token_2.clone()];
    let vb_addr = state.virtual_balance_contract.to_string();
    let vlp_addr = env.contract.address.to_string();

    let mut reserves_normalized = 0u64;
    for token in &pair_tokens {
        if let Ok(old_balance) = BALANCES.load(deps.storage, token.clone()) {
            let decimals = query_token_decimals(&deps, &vb_addr, token)?;
            let normalized = normalize_token_to_voucher(old_balance, decimals)?;

            let voucher_balance = query_voucher_balance(&deps, &vb_addr, &vlp_addr, token)?;
            ensure!(
                normalized == voucher_balance,
                ContractError::new(&format!(
                    "Reserve/voucher mismatch for '{}': normalized_reserve={}, voucher_balance={}",
                    token, normalized, voucher_balance
                ))
            );

            BALANCES.save(deps.storage, token.clone(), &normalized)?;
            reserves_normalized += 1;
        }
    }

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::new()
        .add_attribute("method", "migrate")
        .add_attribute("reserves_normalized", reserves_normalized.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{ADMIN, CHAIN_LP_TOKENS};
    use cosmwasm_std::testing::{mock_env, MockQuerier};
    use cosmwasm_std::{
        from_json, to_json_binary, Addr, ContractResult, QuerierResult, SystemResult, Uint256,
        WasmQuery,
    };
    use cw2::get_contract_version;
    use euclid::admin::EuclidAdmin;
    use euclid::chain::ChainUid;
    use euclid::cross_chain_user::CrossChainUser;
    use euclid::fee::{DenomFees, Fee, TotalFees};
    use euclid::msgs::virtual_balance::msg::{
        GetBalanceResponse, GetTokenMetadataResponse, QueryMsg as VirtualBalanceQueryMsg,
    };
    use euclid::msgs::vlp::base::State;
    use euclid::token::{Pair, Token, TokenMetadata, TokenType};

    fn token1() -> Token {
        Token::create("asset1".to_string()).unwrap()
    }
    fn token2() -> Token {
        Token::create("asset2".to_string()).unwrap()
    }
    fn default_pair() -> Pair {
        Pair {
            token_1: token1(),
            token_2: token2(),
        }
    }
    fn default_fee() -> Fee {
        Fee {
            lp_fee_bps: 30,
            euclid_fee_bps: 10,
            recipient: CrossChainUser::new(
                ChainUid::create("andr".to_string()).unwrap(),
                "recipient".to_string(),
            ),
        }
    }
    fn empty_msg() -> MigrateMsg {
        MigrateMsg {}
    }

    fn mock_querier(
        decimals_1: u32,
        decimals_2: u32,
        vb_balance_1: Uint256,
        vb_balance_2: Uint256,
    ) -> MockQuerier {
        let mut querier = MockQuerier::default();
        let chain = ChainUid::create("chain1".to_string()).unwrap();
        let t1 = token1();
        let t2 = token2();
        querier.update_wasm(move |query| -> QuerierResult {
            match query {
                WasmQuery::Smart { msg, .. } => {
                    let query_msg: VirtualBalanceQueryMsg = from_json(msg).unwrap();
                    match query_msg {
                        VirtualBalanceQueryMsg::GetTokenMetadata { token_id, .. } => {
                            let decimals = if token_id == t1.to_string() {
                                decimals_1
                            } else if token_id == t2.to_string() {
                                decimals_2
                            } else {
                                return SystemResult::Ok(ContractResult::Err(
                                    "Token not found".to_string(),
                                ));
                            };
                            let token_type = TokenType::Native {
                                denom: format!("u{}", token_id),
                                decimals: Some(decimals),
                            };
                            let metadata = TokenMetadata::new(
                                Token::create(token_id).unwrap(),
                                chain.clone(),
                                token_type,
                            );
                            let resp = GetTokenMetadataResponse {
                                metadata: vec![metadata],
                            };
                            SystemResult::Ok(ContractResult::Ok(to_json_binary(&resp).unwrap()))
                        }
                        VirtualBalanceQueryMsg::GetBalance { balance_key } => {
                            let amount = if balance_key.token_id == t1.to_string() {
                                vb_balance_1
                            } else if balance_key.token_id == t2.to_string() {
                                vb_balance_2
                            } else {
                                Uint256::zero()
                            };
                            let resp = GetBalanceResponse { amount };
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
        decimals_1: u32,
        decimals_2: u32,
        vb_balance_1: Uint256,
        vb_balance_2: Uint256,
    ) -> cosmwasm_std::OwnedDeps<
        cosmwasm_std::MemoryStorage,
        cosmwasm_std::testing::MockApi,
        MockQuerier,
    > {
        let querier = mock_querier(decimals_1, decimals_2, vb_balance_1, vb_balance_2);
        let mut deps = cosmwasm_std::OwnedDeps {
            storage: cosmwasm_std::MemoryStorage::default(),
            api: cosmwasm_std::testing::MockApi::default(),
            querier,
            custom_query_type: std::marker::PhantomData,
        };
        let admin = deps.api.addr_make("admin");
        let state = State {
            pair: default_pair(),
            router: Addr::unchecked("router"),
            virtual_balance_contract: Addr::unchecked("virtual_balance"),
            fee: default_fee(),
            total_fees_collected: TotalFees {
                lp_fees: DenomFees {
                    totals: std::collections::HashMap::new(),
                },
                euclid_fees: DenomFees {
                    totals: std::collections::HashMap::new(),
                },
            },
            last_updated: 0,
            total_lp_tokens: Uint256::from(1_000_000u128),
        };
        STATE.save(deps.as_mut().storage, &state).unwrap();
        ADMIN
            .save(deps.as_mut().storage, &EuclidAdmin::default(admin))
            .unwrap();
        deps
    }

    #[test]
    fn test_migrate_sets_contract_version() {
        let mut deps = make_deps(6, 6, Uint256::zero(), Uint256::zero());
        migrate(deps.as_mut(), mock_env(), empty_msg()).unwrap();
        let version = get_contract_version(deps.as_ref().storage).unwrap();
        assert_eq!(version.contract, CONTRACT_NAME);
        assert_eq!(version.version, CONTRACT_VERSION);
    }

    #[test]
    fn test_migrate_normalizes_and_verifies_voucher_balance() {
        let expected_1 = Uint256::from(1_000_000_000_000_000_000_000_000u128);
        let expected_2 = Uint256::from(2_000_000_000_000_000_000_000_000u128);
        let mut deps = make_deps(6, 6, expected_1, expected_2);

        BALANCES
            .save(
                deps.as_mut().storage,
                token1(),
                &Uint256::from(1_000_000u128),
            )
            .unwrap();
        BALANCES
            .save(
                deps.as_mut().storage,
                token2(),
                &Uint256::from(2_000_000u128),
            )
            .unwrap();

        let res = migrate(deps.as_mut(), mock_env(), empty_msg()).unwrap();
        assert_eq!(
            res.attributes
                .iter()
                .find(|a| a.key == "reserves_normalized")
                .map(|a| a.value.as_str()),
            Some("2")
        );
        assert_eq!(
            BALANCES.load(deps.as_ref().storage, token1()).unwrap(),
            expected_1
        );
        assert_eq!(
            BALANCES.load(deps.as_ref().storage, token2()).unwrap(),
            expected_2
        );
    }

    #[test]
    fn test_migrate_fails_voucher_balance_mismatch() {
        let wrong_balance = Uint256::from(999u128);
        let expected = Uint256::from(1_000_000_000_000_000_000_000_000u128);
        let mut deps = make_deps(6, 6, wrong_balance, expected);

        BALANCES
            .save(
                deps.as_mut().storage,
                token1(),
                &Uint256::from(1_000_000u128),
            )
            .unwrap();
        BALANCES
            .save(
                deps.as_mut().storage,
                token2(),
                &Uint256::from(1_000_000u128),
            )
            .unwrap();

        assert!(migrate(deps.as_mut(), mock_env(), empty_msg()).is_err());
    }

    #[test]
    fn test_migrate_lp_supply_unchanged() {
        let expected = Uint256::from(1_000_000_000_000_000_000_000_000u128);
        let mut deps = make_deps(6, 6, expected, expected);

        let chain = ChainUid::create("osmosis".to_string()).unwrap();
        CHAIN_LP_TOKENS
            .save(
                deps.as_mut().storage,
                chain.clone(),
                &Uint256::from(500_000u128),
            )
            .unwrap();
        BALANCES
            .save(
                deps.as_mut().storage,
                token1(),
                &Uint256::from(1_000_000u128),
            )
            .unwrap();
        BALANCES
            .save(
                deps.as_mut().storage,
                token2(),
                &Uint256::from(1_000_000u128),
            )
            .unwrap();

        migrate(deps.as_mut(), mock_env(), empty_msg()).unwrap();

        let state = STATE.load(deps.as_ref().storage).unwrap();
        assert_eq!(state.total_lp_tokens, Uint256::from(1_000_000u128));
        assert_eq!(
            CHAIN_LP_TOKENS.load(deps.as_ref().storage, chain).unwrap(),
            Uint256::from(500_000u128)
        );
    }

    #[test]
    fn test_migrate_different_decimals_per_token() {
        let expected_1 = Uint256::from(1_000_000_000_000_000_000_000_000u128);
        let expected_2 = Uint256::from(1_000_000_000_000_000_000_000_000u128);
        let mut deps = make_deps(6, 18, expected_1, expected_2);

        BALANCES
            .save(
                deps.as_mut().storage,
                token1(),
                &Uint256::from(1_000_000u128),
            )
            .unwrap();
        BALANCES
            .save(
                deps.as_mut().storage,
                token2(),
                &Uint256::from(1_000_000_000_000_000_000u128),
            )
            .unwrap();

        migrate(deps.as_mut(), mock_env(), empty_msg()).unwrap();

        assert_eq!(
            BALANCES.load(deps.as_ref().storage, token1()).unwrap(),
            expected_1
        );
        assert_eq!(
            BALANCES.load(deps.as_ref().storage, token2()).unwrap(),
            expected_2
        );
    }
}
