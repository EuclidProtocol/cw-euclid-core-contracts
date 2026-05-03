use cosmwasm_std::{ensure, to_json_binary, Addr, Binary, Deps, Order, Uint256};
use cw_storage_plus::Bound;
use euclid::{
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    error::ContractError,
    msgs::{
        router::{
            AllChainResponse, AllEscrowsResponse, AllTokensResponse, AllVlpResponse, ChainResponse,
            ChainTimeoutResponse, DefaultReleaseFeeResponse, EscrowResponse, FeeStateResponse,
            LockedChainsResponse, QueryRelayerAddressesResponse, QuerySimulateSwap,
            QueryTokenDenomsResponse, ReleaseFee, ReleaseFeesQueryResponse, SimulateSwapResponse,
            StateResponse, TokenEscrowChainResponse, TokenEscrowsResponse, VlpResponse,
        },
        virtual_balance::{GetTokenMetadataByDenomResponse, GetTokenRegisteredResponse},
        vlp::base::VlpSimulateSwapMsg,
    },
    swap::{NextSwapPair, NextSwapVlp},
    token::{Pair, Token, TokenMetadata, TokenType},
    utils::pagination::{Pagination, DEFAULT_PAGINATION_LIMIT, DEFAULT_PAGINATION_SKIP},
};

use crate::state::{
    ADMIN, CHAIN_TIMEOUT_SECONDS, CHAIN_UID_TO_CHAIN, DEFAULT_RELEASE_FEE, ESCROW_BALANCES,
    FEE_STATE, LOCKED_CHAINS, RELAYER_CONTRACT, RELEASE_FEES, STATE, TOKEN_DENOMS,
    VIRTUAL_BALANCE_CONTRACT, VLPS,
};

pub fn query_state(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    let admins = ADMIN.load(deps.storage)?;
    Ok(to_json_binary(&StateResponse {
        admins,
        constant_product_vlp_code_id: state.constant_product_vlp_code_id,
        stable_vlp_code_id: state.stable_vlp_code_id,
        virtual_balance_address: VIRTUAL_BALANCE_CONTRACT.load(deps.storage)?,
        locked: state.locked,
    })?)
}

pub fn query_all_vlps(
    deps: Deps,
    pagination: Pagination<(String, String)>,
) -> Result<Binary, ContractError> {
    let Pagination {
        min: start,
        max: end,
        skip,
        limit,
    } = pagination;

    let start = start.map(Bound::inclusive);
    let end = end.map(Bound::exclusive);

    let vlps: Result<_, ContractError> = VLPS
        .range(deps.storage, start, end, Order::Ascending)
        .skip(skip.unwrap_or(0) as usize)
        .take(limit.unwrap_or(10) as usize)
        .map(|v| {
            let v = v?;
            Ok(VlpResponse {
                vlp: v.1.to_string(),
                token_1: Token::create(v.0 .0)?,
                token_2: Token::create(v.0 .1)?,
            })
        })
        .collect();

    Ok(to_json_binary(&AllVlpResponse { vlps: vlps? })?)
}

pub fn query_vlp(deps: Deps, pair: Pair) -> Result<Binary, ContractError> {
    let key = pair.get_tupple();
    let vlp = VLPS.load(deps.storage, (key.0.to_string(), key.1.to_string()))?;

    Ok(to_json_binary(&VlpResponse {
        vlp: vlp.to_string(),
        token_1: Token::create(key.0)?,
        token_2: Token::create(key.1)?,
    })?)
}

pub fn query_all_chains(deps: Deps) -> Result<Binary, ContractError> {
    let chains: Result<_, ContractError> = CHAIN_UID_TO_CHAIN
        .range(deps.storage, None, None, Order::Ascending)
        .map(|v| {
            let v = v?;
            Ok(ChainResponse {
                chain: v.1,
                chain_uid: v.0,
            })
        })
        .collect();

    Ok(to_json_binary(&AllChainResponse { chains: chains? })?)
}

pub fn query_chain(deps: Deps, chain_uid: ChainUid) -> Result<Binary, ContractError> {
    let chain_uid = chain_uid.validate()?.to_owned();
    let chain = CHAIN_UID_TO_CHAIN.load(deps.storage, chain_uid.clone())?;
    Ok(to_json_binary(&ChainResponse { chain, chain_uid })?)
}

pub fn query_simulate_swap(deps: Deps, msg: QuerySimulateSwap) -> Result<Binary, ContractError> {
    let first_swap = msg.swaps.first().ok_or(ContractError::Generic {
        err: "Swaps cannot be empty".to_string(),
    })?;

    let last_swap = msg.swaps.last().ok_or(ContractError::Generic {
        err: "Swaps cannot be empty".to_string(),
    })?;

    ensure!(
        first_swap.token_in == msg.asset_in,
        ContractError::new("Asset IN does not match router")
    );

    ensure!(
        last_swap.token_out == msg.asset_out,
        ContractError::new("Asset OUT does not match router")
    );

    let swap_vlps = validate_swap_pairs(deps, &msg.swaps);
    ensure!(
        swap_vlps.is_ok(),
        ContractError::Generic {
            err: "VLPS listed in swaps are not registered".to_string()
        }
    );
    let swap_vlps = swap_vlps?;
    let (first_swap, next_swaps) = swap_vlps.split_first().ok_or(ContractError::Generic {
        err: "Swaps cannot be empty".to_string(),
    })?;

    let simulate_msg = euclid::msgs::vlp::base::QueryMsg::SimulateSwap(VlpSimulateSwapMsg {
        asset: msg.asset_in,
        asset_amount: msg.amount_in,
        swaps: next_swaps.to_vec(),
    });

    let simulate_res: euclid::msgs::vlp::base::GetSwapQueryResponse = deps
        .querier
        .query_wasm_smart(first_swap.vlp_address.clone(), &simulate_msg)?;

    ensure!(
        simulate_res.asset_out == msg.asset_out,
        ContractError::new("Invalid Asset OUT after swap")
    );

    Ok(to_json_binary(&SimulateSwapResponse {
        amount_out: simulate_res.amount_out,
        asset_out: simulate_res.asset_out,
    })?)
}

pub fn validate_swap_pairs(
    deps: Deps,
    swaps: &[NextSwapPair],
) -> Result<Vec<NextSwapVlp>, ContractError> {
    let swap_vlps: Result<_, ContractError> = swaps
        .iter()
        .map(|swap| -> Result<_, ContractError> {
            let pair = Pair::new(swap.token_in.clone(), swap.token_out.clone())?;
            let vlp_address = VLPS.load(deps.storage, pair.get_tupple())?;
            Ok(NextSwapVlp {
                vlp_address: vlp_address.to_string(),
                test_fail: swap.test_fail,
            })
        })
        .collect();
    swap_vlps
}

pub fn verify_cross_chain_addresses(
    deps: Deps,
    users: Vec<CrossChainUser>,
) -> Result<(), ContractError> {
    for user in users.iter() {
        ensure!(
            !user.address.is_empty(),
            ContractError::Generic {
                err: "Address cannot be empty".to_string()
            }
        );
        let chain_uid = user.chain_uid.clone();
        ensure!(
            CHAIN_UID_TO_CHAIN.has(deps.storage, chain_uid.clone()),
            ContractError::Generic {
                err: "Chain UID not registered".to_string()
            }
        );
    }
    Ok(())
}

pub fn query_relayer_addresses(deps: Deps) -> Result<Binary, ContractError> {
    let relayer_addresses = RELAYER_CONTRACT.load(deps.storage)?;
    Ok(to_json_binary(&QueryRelayerAddressesResponse {
        relayer_contract: relayer_addresses,
    })?)
}

pub fn query_release_fees(
    deps: Deps,
    pagination: Pagination<(Token, ChainUid)>,
) -> Result<Binary, ContractError> {
    let Pagination {
        min: start,
        max: end,
        skip,
        limit,
    } = pagination;
    let start = start.map(Bound::inclusive);
    let end = end.map(Bound::exclusive);
    let order = Order::Ascending;
    let release_fees = RELEASE_FEES
        .range(deps.storage, start, end, order)
        .map(|v| {
            let ((token, chain_uid), fee) = v?;
            Ok(ReleaseFee {
                token,
                chain_uid,
                fee,
            })
        })
        .skip(skip.unwrap_or(DEFAULT_PAGINATION_SKIP) as usize)
        .take(limit.unwrap_or(DEFAULT_PAGINATION_LIMIT) as usize)
        .collect::<Result<_, ContractError>>()?;
    Ok(to_json_binary(&ReleaseFeesQueryResponse {
        fees: release_fees,
    })?)
}

pub fn query_token_metadata_by_denom(
    deps: Deps,
    virtual_balance_address: &Addr,
    token: &Token,
    chain_uid: &ChainUid,
    token_type: &TokenType,
) -> Result<TokenMetadata, ContractError> {
    let response: GetTokenMetadataByDenomResponse = deps.querier.query_wasm_smart(
        virtual_balance_address.to_string(),
        &euclid::msgs::virtual_balance::msg::QueryMsg::GetTokenMetadataByDenom {
            token_id: token.to_string(),
            chain_uid: chain_uid.clone(),
            token_type: token_type.clone(),
        },
    )?;
    Ok(response.metadata)
}

pub fn query_token_registered(deps: Deps, token_id: &Token) -> Result<bool, ContractError> {
    let response: GetTokenRegisteredResponse = deps.querier.query_wasm_smart(
        VIRTUAL_BALANCE_CONTRACT.load(deps.storage)?,
        &euclid::msgs::virtual_balance::msg::QueryMsg::GetTokenRegistered {
            token_id: token_id.to_string(),
        },
    )?;
    Ok(response.token_registered)
}

#[allow(deprecated)]
pub fn query_all_escrows(deps: Deps) -> Result<Binary, ContractError> {
    use crate::state::ESCROW_BALANCES;
    let escrows: Vec<EscrowResponse> = ESCROW_BALANCES
        .range(deps.storage, None, None, Order::Ascending)
        .map(|item| {
            let ((token_id, chain_uid), balance) = item?;
            Ok(EscrowResponse {
                token: Token::create(token_id)?,
                chain_uid,
                balance,
            })
        })
        .collect::<Result<_, ContractError>>()?;
    Ok(to_json_binary(&AllEscrowsResponse { escrows })?)
}
pub fn query_locked_chains(deps: Deps) -> Result<Binary, ContractError> {
    let chains = LOCKED_CHAINS.load(deps.storage)?;
    Ok(to_json_binary(&LockedChainsResponse { chains })?)
}

pub fn query_fee_state(deps: Deps) -> Result<Binary, ContractError> {
    let fee_state = FEE_STATE.load(deps.storage)?;
    Ok(to_json_binary(&FeeStateResponse {
        release_fee_recipient: fee_state.release_fee_recipient,
        default_fee_recipient: fee_state.default_fee_recipient,
    })?)
}

pub fn query_default_release_fee(deps: Deps) -> Result<Binary, ContractError> {
    let fee = DEFAULT_RELEASE_FEE
        .may_load(deps.storage)?
        .unwrap_or_default();
    Ok(to_json_binary(&DefaultReleaseFeeResponse { fee })?)
}

pub fn query_chain_timeout(deps: Deps, chain_uid: ChainUid) -> Result<Binary, ContractError> {
    let chain_uid = chain_uid.validate()?.to_owned();
    let timeout_seconds = CHAIN_TIMEOUT_SECONDS.load(deps.storage, chain_uid.clone())?;
    Ok(to_json_binary(&ChainTimeoutResponse {
        chain_uid,
        timeout_seconds,
    })?)
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{
        from_json,
        testing::{message_info, mock_env},
        Addr, Uint128, Uint256,
    };

    use crate::{
        contract::execute,
        state::TOKEN_DENOMS,
        testing::{
            fixtures::initialized,
            helpers::{
                seed_chain1_native, seed_virtual_balance, MockDeps, TEST_RELAYER,
                TEST_VIRTUAL_BALANCE,
            },
        },
    };
    use euclid::{
        chain::ChainUid,
        msgs::router::{
            AllChainResponse, AllEscrowsResponse, AllTokensResponse, AllVlpResponse, ChainResponse,
            ExecuteMsg, ManageRouterState, QueryRelayerAddressesResponse, QueryTokenDenomsResponse,
            ReleaseFeesQueryResponse, StateResponse, TokenDenom, TokenEscrowsResponse, VlpResponse,
        },
        token::{Pair, Token, TokenType},
        utils::pagination::Pagination,
    };
    use rstest::*;

    use crate::{
        contract::query,
        state::{RELEASE_FEES, VLPS},
    };
    use euclid::msgs::router::QueryMsg;
    // -----------------------------------------------------------------------
    // Queries: not-found / empty error cases (table-driven)
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // Queries: empty collections
    // -----------------------------------------------------------------------
    #[rstest]
    fn test_query_get_all_chains_empty(initialized: MockDeps) {
        let parsed: AllChainResponse =
            from_json(query(initialized.as_ref(), mock_env(), QueryMsg::GetAllChains {}).unwrap())
                .unwrap();
        assert!(parsed.chains.is_empty());
    }

    #[rstest]
    fn test_query_get_all_vlps_empty(initialized: MockDeps) {
        let parsed: AllVlpResponse = from_json(
            query(
                initialized.as_ref(),
                mock_env(),
                QueryMsg::GetAllVlps {
                    pagination: Pagination {
                        min: None,
                        max: None,
                        skip: None,
                        limit: None,
                    },
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert!(parsed.vlps.is_empty());
    }

    // -----------------------------------------------------------------------
    // Queries: with data
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_query_get_state(mut initialized: MockDeps) {
        seed_virtual_balance(&mut initialized);

        let parsed: StateResponse =
            from_json(query(initialized.as_ref(), mock_env(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        let creator = initialized.api.addr_make("creator");

        assert_eq!(parsed.constant_product_vlp_code_id, 1);
        assert_eq!(parsed.stable_vlp_code_id, 3);
        assert!(!parsed.locked);
        assert_eq!(parsed.admins.general_admin, creator);
        assert_eq!(
            parsed.virtual_balance_address,
            Addr::unchecked(TEST_VIRTUAL_BALANCE)
        );
    }

    #[rstest]
    fn test_query_get_all_chains_with_data(mut initialized: MockDeps) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        seed_chain1_native(&mut initialized);

        let parsed: AllChainResponse =
            from_json(query(initialized.as_ref(), mock_env(), QueryMsg::GetAllChains {}).unwrap())
                .unwrap();
        assert_eq!(parsed.chains.len(), 1);
        assert_eq!(parsed.chains[0].chain_uid, chain_uid);
    }

    #[rstest]
    fn test_query_get_chain_success(mut initialized: MockDeps) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        seed_chain1_native(&mut initialized);

        let parsed: ChainResponse = from_json(
            query(
                initialized.as_ref(),
                mock_env(),
                QueryMsg::GetChain {
                    chain_uid: chain_uid.clone(),
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(parsed.chain_uid, chain_uid);
        assert_eq!(parsed.chain.factory_address, "factory1");
    }

    #[rstest]
    fn test_query_get_vlp_success(mut initialized: MockDeps) {
        let token1 = Token::create("token1".to_string()).unwrap();
        let token2 = Token::create("token2".to_string()).unwrap();

        VLPS.save(
            initialized.as_mut().storage,
            (token1.to_string(), token2.to_string()),
            &Addr::unchecked("vlp_addr"),
        )
        .unwrap();

        let parsed: VlpResponse = from_json(
            query(
                initialized.as_ref(),
                mock_env(),
                QueryMsg::GetVlp {
                    pair: Pair::new(token1.clone(), token2.clone()).unwrap(),
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(parsed.vlp, "vlp_addr");
        assert_eq!(parsed.token_1, token1);
        assert_eq!(parsed.token_2, token2);
    }

    #[rstest]
    #[rstest]
    fn test_query_relayer_addresses(initialized: MockDeps) {
        let parsed: QueryRelayerAddressesResponse = from_json(
            query(
                initialized.as_ref(),
                mock_env(),
                QueryMsg::QueryRelayerAddresses {},
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(parsed.relayer_contract, Addr::unchecked(TEST_RELAYER));
    }

    #[rstest]
    fn test_query_get_release_fees_empty(initialized: MockDeps) {
        let parsed: ReleaseFeesQueryResponse = from_json(
            query(
                initialized.as_ref(),
                mock_env(),
                QueryMsg::GetReleaseFees {
                    pagination: Pagination {
                        min: None,
                        max: None,
                        skip: None,
                        limit: None,
                    },
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert!(parsed.fees.is_empty());
    }

    #[rstest]
    fn test_query_get_release_fees_with_data(mut initialized: MockDeps) {
        let creator = initialized.api.addr_make("creator");
        let token = Token::create("usdc".to_string()).unwrap();
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();

        RELEASE_FEES
            .save(
                initialized.as_mut().storage,
                (token.clone(), chain_uid.clone()),
                &Uint256::from(250u128),
            )
            .unwrap();
        assert_eq!(
            RELEASE_FEES
                .load(
                    initialized.as_ref().storage,
                    (token.clone(), chain_uid.clone())
                )
                .unwrap(),
            Uint256::from(250u128)
        );

        execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::UpdateReleaseFee {
                token: token.clone(),
                chain_uid: chain_uid.clone(),
                release_fee: Uint256::from(999u128),
            }),
        )
        .unwrap();
        assert_eq!(
            RELEASE_FEES
                .load(initialized.as_ref().storage, (token, chain_uid))
                .unwrap(),
            Uint256::from(999u128)
        );
    }
}
