use cosmwasm_std::{ensure, to_json_binary, Binary, Deps, Order};
use cw_storage_plus::{Bound, PrefixBound};
use euclid::{
    chain::{ChainUid, CrossChainUserWithLimit},
    error::ContractError,
    msgs::router::{
        AllChainResponse, AllTokensResponse, AllVlpResponse, ChainResponse, QuerySimulateSwap,
        SimulateEscrowReleaseResponse, SimulateSwapResponse, StateResponse, TokenEscrowsResponse,
        TokenResponse, VlpResponse,
    },
    swap::{NextSwapPair, NextSwapVlp},
    token::{Pair, Token},
};

use crate::state::{CHAIN_UID_TO_CHAIN, ESCROW_BALANCES, STATE, VLPS};

pub fn query_state(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&StateResponse {
        admin: state.admin,
        vlp_code_id: state.vlp_code_id,
        vcoin_address: state.vcoin_address,
        locked: state.locked,
    })?)
}

pub fn query_all_vlps(
    deps: Deps,
    start: Option<(Token, Token)>,
    end: Option<(Token, Token)>,
    skip: Option<usize>,
    limit: Option<usize>,
) -> Result<Binary, ContractError> {
    let start = start.map(Bound::inclusive);
    let end: Option<Bound<(Token, Token)>> = end.map(Bound::exclusive);

    let vlps: Result<_, ContractError> = VLPS
        .range(deps.storage, start, end, Order::Ascending)
        .skip(skip.unwrap_or(0))
        .take(limit.unwrap_or(10))
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

pub fn query_vlp(deps: Deps, pair: Pair) -> Result<Binary, ContractError> {
    let key = pair.get_tupple();
    let vlp = VLPS.load(deps.storage, key.clone())?;

    Ok(to_json_binary(&VlpResponse {
        vlp,
        token_1: key.0,
        token_2: key.1,
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
        ContractError::new("Asset IN doen't match router")
    );

    ensure!(
        last_swap.token_out == msg.asset_out,
        ContractError::new("Asset OUT doen't match router")
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

    let simulate_msg = euclid::msgs::vlp::QueryMsg::SimulateSwap {
        asset: msg.asset_in,
        asset_amount: msg.amount_in,
        swaps: next_swaps.to_vec(),
    };

    let simulate_res: euclid::msgs::vlp::GetSwapResponse = deps
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

pub fn query_simulate_escrow_release(
    deps: Deps,
    token: Token,
    cross_chain_addresses: Vec<CrossChainUserWithLimit>,
) -> Result<Binary, ContractError> {
    let mut escrow_balances = Vec::new();

    for cross_chain_address in cross_chain_addresses.into_iter() {
        let chain =
            CHAIN_UID_TO_CHAIN.load(deps.storage, cross_chain_address.user.chain_uid.clone())?;

        let escrow_key =
            ESCROW_BALANCES.key((token.clone(), cross_chain_address.user.chain_uid.clone()));

        let escrow_balance = escrow_key.may_load(deps.storage)?.unwrap_or_default();

        escrow_balances.push((chain, escrow_balance));
    }
    Ok(to_json_binary(&SimulateEscrowReleaseResponse {
        escrow_balances,
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
                vlp_address,
                test_fail: swap.test_fail,
            })
        })
        .collect();
    swap_vlps
}

pub fn query_token_escrows(
    deps: Deps,
    token: Token,
    start: Option<ChainUid>,
    end: Option<ChainUid>,
    skip: Option<usize>,
    limit: Option<usize>,
) -> Result<Binary, ContractError> {
    let start = start.map(Bound::inclusive);
    let end = end.map(Bound::exclusive);

    let chains: Result<_, ContractError> = ESCROW_BALANCES
        .prefix(token)
        .range(deps.storage, start, end, Order::Ascending)
        .skip(skip.unwrap_or(0))
        .take(limit.unwrap_or(10))
        .map(|v| {
            let v = v?;
            Ok(v.0)
        })
        .collect();

    Ok(to_json_binary(&TokenEscrowsResponse { chains: chains? })?)
}

pub fn query_all_tokens(
    deps: Deps,
    start: Option<Token>,
    end: Option<Token>,
    skip: Option<usize>,
    limit: Option<usize>,
) -> Result<Binary, ContractError> {
    let start = start.map(PrefixBound::inclusive);
    let end = end.map(PrefixBound::exclusive);

    let tokens: Result<_, ContractError> = ESCROW_BALANCES
        .prefix_range(deps.storage, start, end, Order::Ascending)
        .skip(skip.unwrap_or(0))
        .take(limit.unwrap_or(10))
        .map(|v| {
            let v = v?;
            Ok(TokenResponse {
                token: v.0 .0,
                chain_uid: v.0 .1,
            })
        })
        .collect();

    Ok(to_json_binary(&AllTokensResponse { tokens: tokens? })?)
}
