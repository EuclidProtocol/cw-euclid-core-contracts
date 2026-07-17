use cosmwasm_std::{
    ensure, to_json_binary, Decimal, Deps, DepsMut, Env, Isqrt, MessageInfo, Response, SubMsg,
    Uint256, Uint512, WasmMsg,
};
use cw_storage_plus::{Item, Map};

use euclid::{
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    error::ContractError,
    events::{liquidity_event, tx_event, TxType},
    fee::BPS_50_PERCENT,
    msgs::vlp::base::{
        GetSwapQueryResponse, State, VlpAddLiquidityResponse, VlpSwapMsg, VlpSwapResponse,
        NEXT_SWAP_REPLY_ID,
    },
    swap::NextSwapVlp,
    token::{PairWithAmount, Token},
};

use cosmwasm_std::Decimal256;

use crate::common::{assert_slippage_tolerance, PreSwapResponse, SwapResult, MINIMUM_LIQUIDITY};

/// Constant-product swap math (`x * y = k`). Returns the amount of `token_out`
/// received for a given `swap_amount` of `token_in`, plus the spread the
/// swapper pays vs. the ideal proportional return.
pub fn calculate_cp_swap(
    swap_amount: Uint256,
    reserve_in: Uint256,
    reserve_out: Uint256,
) -> Result<SwapResult, ContractError> {
    let reserve_in = Uint512::from(reserve_in);
    let reserve_out = Uint512::from(reserve_out);
    // Calculate the k constant product
    let k = reserve_in.checked_mul(reserve_out)?;
    // Calculate the new reserve of token 1
    let new_reserve_in = reserve_in.checked_add(swap_amount.into())?;
    // Calculate the new reserve of token 2
    let new_reserve_out = k.checked_div(new_reserve_in)?;

    // Calculate the amount of token 2 to be recieved
    let token_2_recieved = reserve_out.checked_sub(new_reserve_out)?;
    let mut token_2_recieved =
        Uint256::try_from(token_2_recieved).map_err(|_| ContractError::new("Overflow"))?;

    let ideal_return_amount = reserve_out
        .checked_mul(swap_amount.into())?
        .checked_div(reserve_in)?;
    let ideal_return_amount =
        Uint256::try_from(ideal_return_amount).map_err(|_| ContractError::new("Overflow"))?;

    if ideal_return_amount < token_2_recieved {
        // If ideal return amount is less than actual return amount, then set the spread amount to 0 and return the ideal return amount as the return amount
        // This is to prevent the spread amount from being negative which was caused due to precision loss in the calculation
        token_2_recieved = ideal_return_amount;
    }
    let spread_amount = ideal_return_amount.checked_sub(token_2_recieved)?;

    Ok(SwapResult {
        return_amount: token_2_recieved,
        spread_amount,
    })
}

/// CP LP-allocation: geometric mean for first deposit, proportional thereafter.
pub fn calculate_lp_allocation(
    token_1_amount: Uint256,
    token_2_amount: Uint256,
    total_liquidity_1: Uint256,
    total_liquidity_2: Uint256,
    total_lp_supply: Uint256,
) -> Result<Uint256, ContractError> {
    let token_1_amount = Uint512::from(token_1_amount);
    let token_2_amount = Uint512::from(token_2_amount);
    let total_liquidity_1 = Uint512::from(total_liquidity_1);
    let total_liquidity_2 = Uint512::from(total_liquidity_2);
    let total_lp_supply = Uint512::from(total_lp_supply);

    // IF LP supply is 0 use original function
    if total_lp_supply.is_zero() {
        let sq_root = Uint256::try_from(Isqrt::isqrt(token_1_amount.checked_mul(token_2_amount)?))
            .map_err(|_| ContractError::new("Overflow total supply"))?;
        return Ok(sq_root);
    }
    let lp_alloc_1 = safe_lp_math(token_1_amount, total_liquidity_1, total_lp_supply)?;
    let lp_alloc_2 = safe_lp_math(token_2_amount, total_liquidity_2, total_lp_supply)?;

    let lp_allocation = lp_alloc_1.min(lp_alloc_2);

    Uint256::try_from(lp_allocation).map_err(|_| ContractError::new("Overflow lp allocation"))
}

fn safe_lp_math(
    amount: Uint512,
    liquidity: Uint512,
    total_lp_supply: Uint512,
) -> Result<Uint512, ContractError> {
    let lp_allocation = amount
        .checked_mul(total_lp_supply)?
        .checked_div(liquidity)?;
    Ok(lp_allocation)
}

/// Add liquidity to a constant-product VLP pool.
#[allow(clippy::too_many_arguments)]
pub fn add_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    state_storage: &Item<State>,
    balances_storage: &Map<Token, Uint256>,
    chain_lp_tokens_storage: &Map<ChainUid, Uint256>,
    collateral_lp_tokens_storage: &Item<Uint256>,
    sender: CrossChainUser,
    liquidity: PairWithAmount,
    slippage_tolerance_bps: u64,
    tx_id: String,
) -> Result<Response, ContractError> {
    let mut state = state_storage.load(deps.storage)?;
    ensure!(info.sender == state.router, ContractError::Unauthorized {});
    let mut response = Response::new();

    // Ensure tokens are received by VLP
    for token in liquidity.get_vec_token() {
        // Contract should have approval to use voucher tokens on behalf of sender
        let virtual_balance_transfer_msg = token.token.create_voucher_transfer_msg(
            state.virtual_balance_contract.to_string(),
            token.amount,
            None,
            CrossChainUser {
                address: env.contract.address.to_string(),
                chain_uid: ChainUid::vsl_chain_uid()?,
            },
            // We have approval to use voucher tokens on behalf of sender
            Some(sender.clone()),
            None,
        )?;
        response = response.add_message(virtual_balance_transfer_msg);
    }

    let mut chain_lp_tokens =
        chain_lp_tokens_storage.load(deps.storage, sender.chain_uid.clone())?;

    let pair = state.pair.clone();

    let token_1_liquidity = if liquidity.token_1.token == pair.token_1 {
        liquidity.token_1.amount
    } else {
        liquidity.token_2.amount
    };

    let token_2_liquidity = if liquidity.token_2.token == pair.token_2 {
        liquidity.token_2.amount
    } else {
        liquidity.token_1.amount
    };

    // Verify that ratio of assets provided is equal to the ratio of assets in the pool
    let ratio =
        Decimal256::checked_from_ratio(token_1_liquidity, token_2_liquidity).map_err(|err| {
            ContractError::Generic {
                err: err.to_string(),
            }
        })?;

    let mut total_reserve_1 = balances_storage.load(deps.storage, pair.token_1.clone())?;
    let mut total_reserve_2 = balances_storage.load(deps.storage, pair.token_2.clone())?;

    // Lets get lq ratio, it will be the current ratio of token reserves or if its first time then it will be ratio of tokens provided
    let lq_ratio =
        Decimal256::checked_from_ratio(total_reserve_1, total_reserve_2).unwrap_or(ratio);

    // Verify slippage tolerance is between 0 and 50
    ensure!(
        slippage_tolerance_bps.le(&BPS_50_PERCENT),
        ContractError::InvalidSlippageTolerance {}
    );

    assert_slippage_tolerance(ratio, lq_ratio, slippage_tolerance_bps)?;

    let lp_allocation = calculate_lp_allocation(
        token_1_liquidity,
        token_2_liquidity,
        total_reserve_1,
        total_reserve_2,
        state.total_lp_tokens,
    )?;

    let is_new_pool = state.total_lp_tokens.is_zero();
    state.total_lp_tokens = state.total_lp_tokens.checked_add(lp_allocation)?;

    let lp_allocation = if is_new_pool {
        collateral_lp_tokens_storage.save(deps.storage, &Uint256::from(MINIMUM_LIQUIDITY))?;
        lp_allocation
            .checked_sub(Uint256::from(MINIMUM_LIQUIDITY))
            .map_err(|e: cosmwasm_std::OverflowError| {
                ContractError::Generic {
                    err: format!("Min liquidity check failed with error: {e}. Got {lp_allocation} LP but minimum is {MINIMUM_LIQUIDITY} LP."),
                }
            })?
    } else {
        lp_allocation
    };

    ensure!(
        !lp_allocation.is_zero(),
        ContractError::Generic {
            err: "LP Allocation cannot be zero".to_string()
        }
    );

    chain_lp_tokens = chain_lp_tokens.checked_add(lp_allocation)?;
    chain_lp_tokens_storage.save(deps.storage, sender.chain_uid.clone(), &chain_lp_tokens)?;

    // Add to total liquidity and total lp allocation
    total_reserve_1 = total_reserve_1.checked_add(token_1_liquidity)?;
    total_reserve_2 = total_reserve_2.checked_add(token_2_liquidity)?;

    state_storage.save(deps.storage, &state)?;

    balances_storage.save(deps.storage, pair.token_1.clone(), &total_reserve_1)?;
    balances_storage.save(deps.storage, pair.token_2.clone(), &total_reserve_2)?;

    // Prepare Liquidity Response
    let liquidity_response = VlpAddLiquidityResponse {
        liquidity_added: liquidity.clone(),
        mint_lp_tokens: lp_allocation,
        vlp_address: env.contract.address.to_string(),
        tx_id: tx_id.clone(),
        sender: sender.clone(),
    };

    // Prepare acknowledgement
    let acknowledgement = to_json_binary(&liquidity_response)?;

    Ok(response
        .add_event(tx_event(
            &tx_id,
            &sender.to_sender_string(),
            TxType::AddLiquidity,
        ))
        .add_event(liquidity_event(
            &pair
                .get_pair_with_amount(total_reserve_1, total_reserve_2)?
                .get_vec_token(),
            &liquidity.get_vec_token(),
            &tx_id,
        ))
        .add_attribute("action", "add_liquidity")
        .add_attribute("sender", sender.to_sender_string())
        .add_attribute("lp_allocation", lp_allocation)
        .add_attribute("liquidity_1_added", token_1_liquidity)
        .add_attribute("liquidity_2_added", token_2_liquidity)
        .set_data(acknowledgement))
}

/// CP pre-swap: fees and the constant-product return calculation.
pub fn pre_swap(
    deps: &Deps,
    state_storage: &Item<State>,
    balances_storage: &Map<Token, Uint256>,
    asset_in: &Token,
    amount_in: Uint256,
    test_fail: Option<bool>,
    euclid_fee_override: Option<u64>,
) -> Result<PreSwapResponse, ContractError> {
    ensure!(
        !test_fail.unwrap_or(false),
        ContractError::new("Force fail flag")
    );
    // Verify that the asset amount is non-zero
    ensure!(!amount_in.is_zero(), ContractError::ZeroAssetAmount {});

    let state = state_storage.load(deps.storage)?;

    let pair = state.pair.clone();

    ensure!(asset_in.exists(pair), ContractError::AssetDoesNotExist {});
    let asset_out = state.pair.get_other_token(asset_in.clone());

    let token_in_reserve = balances_storage.load(deps.storage, asset_in.clone())?;
    let token_out_reserve = balances_storage.load(deps.storage, asset_out.clone())?;

    // Get Fee from the state
    let fee = state.clone().fee;

    // The LP fee is always charged at the pool's configured rate. The Euclid
    // fee uses the per-wallet override when present, otherwise the pool rate.
    //
    // This applies the override for the constant-product (`Regular`) and
    // `Stable` curves, where the Euclid fee is an *additive* trader fee carved
    // out of `amount_in` — so reducing it directly improves the wallet's quote.
    //
    // NOTE(SC-23): concentrated (CLP) pools do NOT go through `pre_swap`; they
    // have their own swap math under `contracts/hub/concentrated_vlp`. A CLP
    // must NOT treat the Euclid value as an additive trader fee — there it is
    // the protocol's *cut* of the LP swap fee, so zeroing it here would only
    // shift the protocol's share to LPs, not improve the wallet's quote. The
    // CLP applies the override by lowering its structural fee tier instead, in
    // `concentrated_vlp::contract::resolve_effective_fee` (SC-23 Issue 8): the
    // LP keeps its absolute pip share `lp_pips = tier_pips - tier_pips*d/10_000`
    // and the protocol's slice scales as `tier_pips * X / 10_000`, so
    // `effective_fee_pips = lp_pips + tier_pips*X/10_000`. Do not branch on
    // `euclid_fee_override` for a CLP leg in this function.
    let euclid_fee_bps = euclid_fee_override.unwrap_or(fee.euclid_fee_bps);

    let lp_fee = amount_in.checked_mul_floor(Decimal::bps(fee.lp_fee_bps))?;
    let euclid_fee = amount_in.checked_mul_floor(Decimal::bps(euclid_fee_bps))?;

    let swap_amount = amount_in.checked_sub(lp_fee.checked_add(euclid_fee)?)?;

    let swap_result = calculate_cp_swap(swap_amount, token_in_reserve, token_out_reserve)?;

    Ok(PreSwapResponse {
        lp_fee,
        euclid_fee,
        swap_amount,
        receive_amount: swap_result.return_amount,
        asset_out,
        spread_amount: swap_result.spread_amount,
    })
}

/// Execute a swap against a constant-product VLP pool.
#[allow(clippy::too_many_arguments)]
pub fn execute_swap(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    state_storage: &Item<State>,
    balances_storage: &Map<Token, Uint256>,
    sender: CrossChainUser,
    asset_in: Token,
    amount_in: Uint256,
    min_token_out: Uint256,
    tx_id: String,
    next_swaps: Vec<NextSwapVlp>,
    test_fail: Option<bool>,
    euclid_fee_override: Option<u64>,
) -> Result<Response, ContractError> {
    let mut state = state_storage.load(deps.storage)?;

    // If the sender is the router, use the sender as the voucher sender
    // Otherwise, use the last contract caller as the voucher sender
    let voucher_sender = if info.sender == state.router {
        sender.clone()
    } else {
        CrossChainUser {
            address: info.sender.to_string(),
            chain_uid: ChainUid::vsl_chain_uid()?,
        }
    };

    // Swap needs approval to use voucher tokens
    let transfer_voucher_msg = euclid::msgs::virtual_balance::msg::ExecuteMsg::Transfer(
        euclid::msgs::virtual_balance::msg::ExecuteTransfer {
            amount: amount_in,
            token_id: asset_in.to_string(),
            from: Some(voucher_sender.clone()),
            to: CrossChainUser {
                address: env.contract.address.to_string(),
                chain_uid: ChainUid::vsl_chain_uid()?,
            },
            sender: None,
            msg: None,
        },
    );

    let transfer_voucher_msg = WasmMsg::Execute {
        contract_addr: state.virtual_balance_contract.to_string(),
        msg: to_json_binary(&transfer_voucher_msg)?,
        funds: vec![],
    };

    let mut response = Response::new();
    // Should reject full execution if failed
    response = response.add_message(transfer_voucher_msg);

    let PreSwapResponse {
        lp_fee,
        euclid_fee,
        swap_amount,
        receive_amount,
        asset_out,
        spread_amount,
    } = pre_swap(
        &deps.as_ref(),
        state_storage,
        balances_storage,
        &asset_in,
        amount_in,
        test_fail,
        euclid_fee_override,
    )?;

    // Add the lp fee to total fees
    state
        .total_fees_collected
        .lp_fees
        .add_fee(asset_in.to_string(), lp_fee);

    // Calcuate the sum of fees
    let total_fee = lp_fee.checked_add(euclid_fee)?;

    // Verify that the receive amount is greater than 0 to be eligible for any swap
    ensure!(
        !receive_amount.is_zero(),
        ContractError::SlippageExceeded {
            amount: receive_amount,
            min_amount_out: min_token_out,
        }
    );

    let mut token_in_reserve = balances_storage.load(deps.storage, asset_in.clone())?;
    let mut token_out_reserve = balances_storage.load(deps.storage, asset_out.clone())?;

    token_in_reserve = token_in_reserve
        .checked_add(swap_amount)?
        .checked_add(lp_fee)?;
    token_out_reserve = token_out_reserve.checked_sub(receive_amount)?;

    ensure!(
        !token_out_reserve.is_zero(),
        ContractError::new("Token out reserve is zero")
    );

    balances_storage.save(deps.storage, asset_in.clone(), &token_in_reserve)?;
    balances_storage.save(deps.storage, asset_out.clone(), &token_out_reserve)?;

    // Finalize ack response to swap pool
    let swap_response = VlpSwapResponse {
        sender: sender.clone(),
        tx_id: tx_id.clone(),
        asset_out: asset_out.clone(),
        amount_out: receive_amount,
    };

    // Prepare acknowledgement
    let acknowledgement = to_json_binary(&swap_response)?;

    if !euclid_fee.is_zero() {
        // Get Fee from the state
        let fee = state.clone().fee;
        // Add the euclid fee to total fees
        state
            .total_fees_collected
            .euclid_fees
            .add_fee(asset_in.to_string(), euclid_fee);

        let euclid_fee_transfer_msg = euclid::msgs::virtual_balance::msg::ExecuteMsg::Transfer(
            euclid::msgs::virtual_balance::msg::ExecuteTransfer {
                amount: euclid_fee,
                token_id: asset_in.to_string(),
                to: fee.recipient,
                from: None,
                sender: None,
                msg: None,
            },
        );

        let euclid_fee_transfer_msg = WasmMsg::Execute {
            contract_addr: state.virtual_balance_contract.to_string(),
            msg: to_json_binary(&euclid_fee_transfer_msg)?,
            funds: vec![],
        };

        response = response.add_message(euclid_fee_transfer_msg);
    }

    match next_swaps.split_first() {
        Some((next_swap, forward_swaps)) => {
            // There are more swaps
            let virtual_balance_approve_msg =
                euclid::msgs::virtual_balance::msg::ExecuteMsg::Approve(
                    euclid::msgs::virtual_balance::msg::ExecuteApprove {
                        amount: swap_response.amount_out,
                        token_id: swap_response.asset_out.to_string(),

                        owner: CrossChainUser {
                            address: env.contract.address.to_string(),
                            chain_uid: ChainUid::vsl_chain_uid()?,
                        },

                        spender: CrossChainUser {
                            address: next_swap.vlp_address.clone(),
                            chain_uid: ChainUid::vsl_chain_uid()?,
                        },
                    },
                );

            let virtual_balance_approve_msg = WasmMsg::Execute {
                contract_addr: state.virtual_balance_contract.to_string(),
                msg: to_json_binary(&virtual_balance_approve_msg)?,
                funds: vec![],
            };

            let next_swap_msg = euclid::msgs::vlp::cp::msg::ExecuteMsg::Swap(VlpSwapMsg {
                sender: sender.clone(),
                tx_id: tx_id.clone(),
                asset_in: swap_response.asset_out,
                amount_in: swap_response.amount_out,
                min_token_out,
                next_swaps: forward_swaps.to_vec(),
                test_fail: next_swap.test_fail,
                // Forward the override so it applies uniformly across every hop.
                euclid_fee_override,
            });
            let next_swap_msg = WasmMsg::Execute {
                contract_addr: next_swap.vlp_address.clone(),
                msg: to_json_binary(&next_swap_msg)?,
                funds: vec![],
            };

            let next_swap_msg = SubMsg::reply_always(next_swap_msg, NEXT_SWAP_REPLY_ID);

            response = response
                .add_attribute("swap_type", "forward_swap")
                .add_attribute("forward_to", next_swap.vlp_address.clone())
                .add_message(virtual_balance_approve_msg)
                .add_submessage(next_swap_msg);
        }
        None => {
            //Its the last swap

            // Verify that the receive amount is >= min amount as its last swap
            ensure!(
                receive_amount.ge(&min_token_out),
                ContractError::SlippageExceeded {
                    amount: receive_amount,
                    min_amount_out: min_token_out,
                }
            );

            let virtual_balance_transfer_msg =
                euclid::msgs::virtual_balance::msg::ExecuteMsg::Transfer(
                    euclid::msgs::virtual_balance::msg::ExecuteTransfer {
                        amount: swap_response.amount_out,
                        token_id: swap_response.asset_out.to_string(),

                        // Destination Address
                        to: sender.clone(),
                        sender: None,
                        from: None,
                        msg: None,
                    },
                );

            let virtual_balance_transfer_msg = WasmMsg::Execute {
                contract_addr: state.virtual_balance_contract.to_string(),
                msg: to_json_binary(&virtual_balance_transfer_msg)?,
                funds: vec![],
            };

            response = response
                .add_attribute("swap_type", "final_swap")
                .add_attribute("receiver_address", sender.address.clone())
                .add_attribute("receiver_chain_id", sender.chain_uid.to_string())
                .add_message(virtual_balance_transfer_msg);
        }
    };

    // Save changes to total fees in state
    state_storage.save(deps.storage, &state)?;

    Ok(response
        .add_event(tx_event(&tx_id, &sender.to_sender_string(), TxType::Swap))
        .add_event(liquidity_event(
            &[
                asset_in.with_amount(token_in_reserve),
                asset_out.with_amount(token_out_reserve),
            ],
            &[
                asset_in.with_amount(swap_amount.checked_add(lp_fee)?),
                asset_out.with_amount(receive_amount),
            ],
            &tx_id,
        ))
        .add_attribute("action", "swap")
        .add_attribute("amount_in", amount_in)
        .add_attribute("asset_in", asset_in.to_string())
        .add_attribute("asset_out", asset_out.to_string())
        .add_attribute("total_fee", total_fee)
        .add_attribute("euclid_fee", euclid_fee)
        .add_attribute("lp_fee", lp_fee)
        .add_attribute("receive_amount", receive_amount)
        .add_attribute("spread_amount", spread_amount)
        .set_data(acknowledgement))
}

/// Simulate a swap against a constant-product VLP pool.
pub fn simulate_swap(
    deps: Deps,
    state_storage: &Item<State>,
    balances_storage: &Map<Token, Uint256>,
    asset_in: Token,
    amount_in: Uint256,
    euclid_fee_override: Option<u64>,
) -> Result<GetSwapQueryResponse, ContractError> {
    let pre_swap_response = pre_swap(
        &deps,
        state_storage,
        balances_storage,
        &asset_in,
        amount_in,
        None,
        euclid_fee_override,
    )?;

    Ok(GetSwapQueryResponse {
        amount_out: pre_swap_response.receive_amount,
        asset_out: pre_swap_response.asset_out,
        spread_amount: pre_swap_response.spread_amount,
        lp_fee: pre_swap_response.lp_fee,
        euclid_fee: pre_swap_response.euclid_fee,
    })
}
