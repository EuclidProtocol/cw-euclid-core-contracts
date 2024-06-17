use cosmwasm_std::{ensure, to_json_binary, Binary, Deps, Order, Uint128};
use euclid::{
    error::ContractError,
    msgs::router::{
        AllChainResponse, AllVlpResponse, ChainResponse, QuerySimulateSwap, SimulateSwapResponse,
        StateResponse, SwapOutChain, VlpResponse,
    },
    swap::NextSwap,
    token::Token,
};

use crate::state::{CHAIN_ID_TO_CHAIN, ESCROW_BALANCES, STATE, VLPS};

pub fn query_state(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&StateResponse {
        admin: state.admin,
        vlp_code_id: state.vlp_code_id,
        vcoin_address: state.vcoin_address,
    })?)
}

pub fn query_all_vlps(deps: Deps) -> Result<Binary, ContractError> {
    let vlps: Result<_, ContractError> = VLPS
        .range(deps.storage, None, None, Order::Ascending)
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

pub fn query_vlp(deps: Deps, token_1: Token, token_2: Token) -> Result<Binary, ContractError> {
    let vlp = VLPS.load(deps.storage, (token_1.clone(), token_2.clone()))?;

    Ok(to_json_binary(&VlpResponse {
        vlp,
        token_1,
        token_2,
    })?)
}

pub fn query_all_chains(deps: Deps) -> Result<Binary, ContractError> {
    let chains: Result<_, ContractError> = CHAIN_ID_TO_CHAIN
        .range(deps.storage, None, None, Order::Ascending)
        .map(|v| {
            let v = v?;
            Ok(ChainResponse { chain: v.1 })
        })
        .collect();

    Ok(to_json_binary(&AllChainResponse { chains: chains? })?)
}

pub fn query_chain(deps: Deps, chain_id: String) -> Result<Binary, ContractError> {
    let chain = CHAIN_ID_TO_CHAIN.load(deps.storage, chain_id)?;
    Ok(to_json_binary(&ChainResponse { chain })?)
}

pub fn query_simulate_swap(deps: Deps, msg: QuerySimulateSwap) -> Result<Binary, ContractError> {
    ensure!(
        validate_swap_vlps(deps, &msg.swaps).is_ok(),
        ContractError::Generic {
            err: "VLPS listed in swaps are not registered".to_string()
        }
    );
    let (first_swap, next_swaps) = msg.swaps.split_first().ok_or(ContractError::Generic {
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

    let token_out_escrow_key = (simulate_res.asset_out.clone(), msg.to_chain_id.clone());

    let token_out_escrow_balance = ESCROW_BALANCES
        .may_load(deps.storage, token_out_escrow_key.clone())?
        .unwrap_or(Uint128::zero());

    ensure!(
        token_out_escrow_balance.ge(&simulate_res.amount_out),
        ContractError::Generic {
            err: "Insufficient Escrow Balance on out chain".to_string()
        }
    );
    let chain = CHAIN_ID_TO_CHAIN.load(deps.storage, msg.to_chain_id)?;

    let out_chain = SwapOutChain {
        amount: simulate_res.amount_out,
        chain,
    };

    Ok(to_json_binary(&SimulateSwapResponse {
        amount_out: simulate_res.amount_out,
        asset_out: simulate_res.asset_out,
        out_chains: vec![out_chain],
    })?)
}

pub fn validate_swap_vlps(deps: Deps, swaps: &[NextSwap]) -> Result<(), ContractError> {
    let all_vlps: Result<Vec<String>, ContractError> = VLPS
        .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .map(|item| {
            let item = item?;
            Ok(item.1)
        })
        .collect();

    let all_vlps = all_vlps?;
    // Do an early check that all vlps are present
    for swap in swaps {
        ensure!(
            all_vlps.contains(&swap.vlp_address),
            ContractError::UnsupportedOperation {}
        );
    }
    Ok(())
}
