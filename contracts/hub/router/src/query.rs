use cosmwasm_std::{ensure, to_json_binary, Binary, Deps, Order};
use cw_storage_plus::{Bound, PrefixBound};
use euclid::{
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    error::ContractError,
    msgs::{
        router::{
            AllChainResponse, AllEscrowsResponse, AllTokensResponse, AllVlpResponse, ChainResponse, EscrowResponse,
            PoolKeyVlpResponse, QueryRelayerAddressesResponse, QuerySimulateSwap,
            QueryTokenDenomsResponse, ReleaseFee, ReleaseFeesQueryResponse, SimulateSwapResponse,
            StateResponse, TokenEscrowChainResponse, TokenEscrowsResponse, VlpResponse,
        },
        vlp::base::{PoolKey, PoolType, VlpSimulateSwapMsg},
    },
    swap::{NextSwapPair, NextSwapVlp},
    token::{Pair, Token},
    utils::pagination::{Pagination, DEFAULT_PAGINATION_LIMIT, DEFAULT_PAGINATION_SKIP},
};

use crate::state::{
    map_key_to_pool_parts, pool_key_to_map_key, CHAIN_UID_TO_CHAIN, CONCENTRATED_VLPS,
    ESCROW_BALANCES, RELAYER_CONTRACT, RELEASE_FEES, STATE, TOKEN_DENOMS,
    VIRTUAL_BALANCE_CONTRACT, VLPS,
};

pub fn query_state(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&StateResponse {
        admin: state.admin,
        constant_product_vlp_code_id: state.constant_product_vlp_code_id,
        stable_vlp_code_id: state.stable_vlp_code_id,
        concentrated_vlp_code_id: state.concentrated_vlp_code_id,
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

    let mut vlps: Vec<VlpResponse> = VLPS
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
        .collect::<Result<_, ContractError>>()?;

    let concentrated_vlps: Result<Vec<_>, ContractError> = CONCENTRATED_VLPS
        .range(deps.storage, None, None, Order::Ascending)
        .map(|v| {
            let v = v?;
            let (token_1, token_2, _, _) = map_key_to_pool_parts(&v.0).ok_or(
                ContractError::new("invalid concentrated pool key in state"),
            )?;
            Ok(VlpResponse {
                vlp: v.1.to_string(),
                token_1: Token::create(token_1)?,
                token_2: Token::create(token_2)?,
            })
        })
        .collect();
    vlps.extend(concentrated_vlps?);

    Ok(to_json_binary(&AllVlpResponse { vlps })?)
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

pub fn query_vlp_by_pool_key(deps: Deps, pool_key: PoolKey) -> Result<Binary, ContractError> {
    let key = pool_key_to_map_key(&pool_key);
    let vlp = CONCENTRATED_VLPS.load(deps.storage, key)?;
    Ok(to_json_binary(&PoolKeyVlpResponse {
        vlp: vlp.to_string(),
        pool_key,
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
            let vlp_address = if let Some(pool_key) = &swap.pool_key {
                ensure!(
                    matches!(pool_key.pool_type, PoolType::Concentrated { .. }),
                    ContractError::new("pool_key must reference a concentrated pool")
                );
                ensure!(
                    pair.get_tupple() == pool_key.pair.get_tupple(),
                    ContractError::new("swap hop tokens do not match pool_key pair")
                );

                let key = pool_key_to_map_key(pool_key);
                CONCENTRATED_VLPS
                    .load(deps.storage, key)
                    .map_err(|_| ContractError::new("concentrated pool for provided pool_key is not registered"))?
            } else {
                VLPS.load(deps.storage, pair.get_tupple())?
            };

            Ok(NextSwapVlp {
                vlp_address: vlp_address.to_string(),
                test_fail: swap.test_fail,
            })
        })
        .collect();
    swap_vlps
}

pub fn query_token_escrows(
    deps: Deps,
    token: Token,
    pagination: Pagination<ChainUid>,
) -> Result<Binary, ContractError> {
    let Pagination {
        min: start,
        max: end,
        skip,
        limit,
    } = pagination;

    let start = start.map(Bound::inclusive);
    let end = end.map(Bound::exclusive);

    let chains: Result<_, ContractError> = ESCROW_BALANCES
        .prefix(token.to_string())
        .range(deps.storage, start, end, Order::Ascending)
        .skip(skip.unwrap_or(0) as usize)
        .take(limit.unwrap_or(10) as usize)
        .map(|v| {
            let v = v?;
            Ok(TokenEscrowChainResponse {
                balance: v.1,
                chain_uid: v.0,
            })
        })
        .collect();

    Ok(to_json_binary(&TokenEscrowsResponse { chains: chains? })?)
}

pub fn query_all_escrows(
    deps: Deps,
    pagination: Pagination<String>,
) -> Result<Binary, ContractError> {
    let Pagination {
        min: start,
        max: end,
        skip,
        limit,
    } = pagination;
    let start = start.map(PrefixBound::inclusive);
    let end = end.map(PrefixBound::exclusive);

    let escrows: Result<_, ContractError> = ESCROW_BALANCES
        .prefix_range(deps.storage, start, end, Order::Ascending)
        .skip(skip.unwrap_or(DEFAULT_PAGINATION_SKIP) as usize)
        .take(limit.unwrap_or(DEFAULT_PAGINATION_LIMIT) as usize)
        .map(|v| {
            let v = v?;
            Ok(EscrowResponse {
                token: Token::create(v.0 .0)?,
                chain_uid: v.0 .1,
                balance: v.1,
            })
        })
        .collect();

    Ok(to_json_binary(&AllEscrowsResponse { escrows: escrows? })?)
}

pub fn query_all_tokens(
    deps: Deps,
    pagination: Pagination<Token>,
) -> Result<Binary, ContractError> {
    let Pagination {
        min: start,
        max: end,
        skip,
        limit,
    } = pagination;

    let start = start.map(Bound::inclusive);
    let end = end.map(Bound::exclusive);
    let tokens = TOKEN_DENOMS
        .keys(deps.storage, start, end, Order::Ascending)
        .skip(skip.unwrap_or(DEFAULT_PAGINATION_SKIP) as usize)
        .take(limit.unwrap_or(DEFAULT_PAGINATION_LIMIT) as usize)
        .flatten()
        .collect();

    Ok(to_json_binary(&AllTokensResponse { tokens })?)
}

pub fn query_token_denoms(deps: Deps, token: Token) -> Result<Binary, ContractError> {
    ensure!(
        !TOKEN_DENOMS.is_empty(deps.storage),
        ContractError::Generic {
            err: "Token denoms are not registered".to_string()
        }
    );
    let denoms = TOKEN_DENOMS.load(deps.storage, token)?;
    Ok(to_json_binary(&QueryTokenDenomsResponse { denoms })?)
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
