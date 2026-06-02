use cosmwasm_std::{ensure, to_json_binary, DepsMut, Env, Response, SubMsg, WasmMsg};
use euclid::{
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    error::ContractError,
    events::{tx_event, TxType},
    msgs::{
        self,
        virtual_balance::msg::{ExecuteApprove, ExecuteMint, ExecuteTransfer},
        vlp::base::{VlpSimulateSwapMsg, VlpSwapMsg},
    },
    normalize::normalize_token_to_voucher,
    voucher::BalanceKey,
};
use euclid_ibc::router_ibc::RouterCrossChainSwapExecuteMsg;

use crate::{
    helpers::euclid_fee_override::get_euclid_fee_override,
    query::{query_token_metadata_by_denom, validate_swap_pairs},
    reply::SWAP_REPLY_ID,
    state::{PENDING_SWAPS, VIRTUAL_BALANCE_CONTRACT},
};

pub fn ibc_execute_swap(
    deps: DepsMut,
    _env: Env,
    msg: RouterCrossChainSwapExecuteMsg,
) -> Result<Response, ContractError> {
    let first_swap = msg.swaps.first().ok_or(ContractError::Generic {
        err: "Swaps cannot be empty".to_string(),
    })?;

    let last_swap = msg.swaps.last().ok_or(ContractError::Generic {
        err: "Swaps cannot be empty".to_string(),
    })?;

    ensure!(
        first_swap.token_in == msg.asset_in.token,
        ContractError::new("Asset IN does not match router")
    );

    ensure!(
        last_swap.token_out == msg.asset_out,
        ContractError::new("Asset OUT does not match router")
    );

    let req_key = msg.tx_id.clone();

    ensure!(
        !PENDING_SWAPS.has(deps.storage, req_key.clone()),
        ContractError::TxAlreadyExist {}
    );

    PENDING_SWAPS.save(deps.storage, req_key, &msg)?;

    let mut response = Response::new().add_event(
        tx_event(&msg.tx_id, &msg.sender.to_sender_string(), TxType::Swap)
            .add_attribute("tx_id", msg.tx_id.clone()),
    );

    let sender = msg.sender;

    // Resolve the swapping wallet's Euclid-fee override once, at this single
    // chokepoint, and stamp it onto both the slippage pre-check simulation and
    // the outgoing swap message. Native and IBC swaps both converge here, so
    // both inherit the override identically — and the pre-check sees the same
    // fee execution will charge.
    let euclid_fee_override = get_euclid_fee_override(deps.storage, &sender)?;

    let virtual_balance_address = VIRTUAL_BALANCE_CONTRACT.load(deps.storage)?;

    let swap_vlps = validate_swap_pairs(deps.as_ref(), &msg.swaps);
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

    // Normalize amount_in to voucher decimals
    let normalized_amount_in = if msg.asset_in.token_type.is_voucher() {
        msg.amount_in
    } else {
        let metadata_in = query_token_metadata_by_denom(
            deps.as_ref(),
            &virtual_balance_address,
            &msg.asset_in.token,
            &sender.chain_uid,
            &msg.asset_in.token_type,
        )?;
        normalize_token_to_voucher(msg.amount_in, metadata_in.token_type.get_decimals()?)?
    };

    // min_amount_out is already in voucher units (24 decimals)
    let normalized_min_amount_out = msg.min_amount_out;

    // Simulation increases gas, ideally this can be resolved but we are still getting codespace wasm errors so this is added as a temporary fix for better error messages
    let simulate_swap_msg = euclid::msgs::vlp::base::QueryMsg::SimulateSwap(VlpSimulateSwapMsg {
        asset: msg.asset_in.token.clone(),
        asset_amount: normalized_amount_in,
        swaps: next_swaps.to_vec(),
        euclid_fee_override,
    });

    let simulate_swap_res: euclid::msgs::vlp::base::GetSwapQueryResponse = deps
        .querier
        .query_wasm_smart(first_swap.vlp_address.clone(), &simulate_swap_msg)?;

    ensure!(
        simulate_swap_res.amount_out.ge(&normalized_min_amount_out),
        ContractError::SlippageExceeded {
            amount: simulate_swap_res.amount_out,
            min_amount_out: normalized_min_amount_out,
        }
    );

    // Mint voucher token if it is not a voucher token (escrow managed by virtual_balance)
    if !msg.asset_in.token_type.is_voucher() {
        // Mint virtual balance for the first swap vlp so it can start processing tx
        let mint_virtual_balance_msg =
            euclid::msgs::virtual_balance::msg::ExecuteMsg::Mint(ExecuteMint {
                amount: msg.amount_in.into(),
                balance_key: BalanceKey {
                    cross_chain_user: sender.clone(),
                    token_id: msg.asset_in.token.to_string(),
                },
                token_type: msg.asset_in.token_type.clone(),
                token_source_chain_uid: sender.chain_uid.clone(),
            });

        let mint_virtual_balance_msg = WasmMsg::Execute {
            contract_addr: virtual_balance_address.to_string(),
            msg: to_json_binary(&mint_virtual_balance_msg)?,
            funds: vec![],
        };

        // Should reject full execution if failed
        response = response.add_message(mint_virtual_balance_msg);
    }

    if msg.asset_in.token_type.is_voucher() {
        let user_voucher_balance_msg = euclid::msgs::virtual_balance::msg::QueryMsg::GetBalance {
            balance_key: BalanceKey {
                cross_chain_user: sender.clone(),
                token_id: msg.asset_in.token.to_string(),
            },
        };

        let user_voucher_balance_res: euclid::msgs::virtual_balance::msg::GetBalanceResponse =
            deps.querier.query_wasm_smart(
                virtual_balance_address.to_string(),
                &user_voucher_balance_msg,
            )?;

        ensure!(
            user_voucher_balance_res.amount.ge(&normalized_amount_in),
            ContractError::InsufficientAmount {
                min_amount: msg.amount_in,
                amount: user_voucher_balance_res.amount,
            }
        );
    }

    let approve_voucher_msg =
        euclid::msgs::virtual_balance::msg::ExecuteMsg::Approve(ExecuteApprove {
            amount: normalized_amount_in,
            token_id: msg.asset_in.token.to_string(),
            spender: CrossChainUser::new(
                ChainUid::vsl_chain_uid()?,
                first_swap.vlp_address.clone(),
            ),
            owner: sender.clone(),
        });

    let approve_voucher_msg = WasmMsg::Execute {
        contract_addr: virtual_balance_address.to_string(),
        msg: to_json_binary(&approve_voucher_msg)?,
        funds: vec![],
    };

    // Should reject full execution if failed
    response = response.add_message(approve_voucher_msg);

    // Voucher partner fees handled here; native-asset partner fees handled at factory in ack_swap_request
    if msg.asset_in.token_type.is_voucher()
        && !msg.partner_fee_amount.is_zero()
        && msg.partner_fee_recipient != sender
    {
        // Voucher partner fees are already in 24-decimal units
        let normalized_partner_fee = msg.partner_fee_amount;

        let transfer_voucher_msg =
            euclid::msgs::virtual_balance::msg::ExecuteMsg::Transfer(ExecuteTransfer {
                amount: normalized_partner_fee,
                token_id: msg.asset_in.token.to_string(),
                sender: Some(sender.clone()),
                to: msg.partner_fee_recipient.clone(),
                from: None,
                msg: None,
            });

        let transfer_voucher_msg = WasmMsg::Execute {
            contract_addr: virtual_balance_address.to_string(),
            msg: to_json_binary(&transfer_voucher_msg)?,
            funds: vec![],
        };

        // Should reject full execution if failed
        response = response
            .add_message(transfer_voucher_msg)
            .add_attribute("partner_fee_transfer", "true")
            .add_attribute(
                "partner_fee_recipient",
                msg.partner_fee_recipient.to_sender_string(),
            )
            .add_attribute("partner_fee_amount", msg.partner_fee_amount.to_string());
    }

    let swap_msg = msgs::vlp::base::ExecuteMsg::Swap(VlpSwapMsg {
        sender: sender.clone(),
        asset_in: msg.asset_in.token.clone(),
        amount_in: normalized_amount_in,
        min_token_out: normalized_min_amount_out,
        next_swaps: next_swaps.to_vec(),
        tx_id: msg.tx_id.clone(),
        test_fail: first_swap.test_fail,
        euclid_fee_override,
    });

    let msg = WasmMsg::Execute {
        contract_addr: first_swap.vlp_address.clone(),
        msg: to_json_binary(&swap_msg)?,
        funds: vec![],
    };
    Ok(response.add_submessage(SubMsg::reply_always(msg, SWAP_REPLY_ID)))
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::Uint256;
    use euclid::{
        chain::ChainUid,
        cross_chain_user::CrossChainUser,
        error::ContractError,
        swap::NextSwapPair,
        token::{Token, TokenType, TokenWithDenom},
    };
    use euclid_ibc::router_ibc::{RouterCrossChainExecuteMsg, RouterCrossChainSwapExecuteMsg};

    use crate::{
        reply::SWAP_REPLY_ID,
        state::PENDING_SWAPS,
        testing::helpers::{call_reusable, make_swap_deps_with_mock_querier},
    };

    fn make_swap_msg(chain_uid: &ChainUid, tx_id: &str) -> RouterCrossChainExecuteMsg {
        let sender = CrossChainUser::new(chain_uid.clone(), "user".to_string());
        let token_a = Token::create("aaa".to_string()).unwrap();
        let token_b = Token::create("bbb".to_string()).unwrap();
        RouterCrossChainExecuteMsg::Swap(RouterCrossChainSwapExecuteMsg {
            sender: sender.clone(),
            asset_in: TokenWithDenom {
                token: token_a.clone(),
                token_type: TokenType::Native {
                    denom: "uaaa".to_string(),
                    decimals: Some(6),
                },
            },
            amount_in: Uint256::from(100u128),
            asset_out: token_b.clone(),
            min_amount_out: Uint256::from(80u128),
            swaps: vec![NextSwapPair {
                token_in: token_a,
                token_out: token_b,
                test_fail: None,
                pool_key: None,
            }],
            recipients: vec![],
            partner_fee_amount: Uint256::zero(),
            partner_fee_recipient: sender,
            tx_id: tx_id.to_string(),
        })
    }

    #[test]
    fn test_ibc_swap_saves_pending_and_emits_submsg() {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let mut deps = make_swap_deps_with_mock_querier(90);

        let res = call_reusable(
            &mut deps,
            make_swap_msg(&chain_uid, "tx_swap"),
            chain_uid.clone(),
        )
        .unwrap();

        assert_eq!(
            res.messages.last().unwrap().id,
            SWAP_REPLY_ID,
            "expected swap submsg"
        );
        assert!(
            PENDING_SWAPS.has(deps.as_ref().storage, "tx_swap".to_string()),
            "expected PENDING_SWAPS entry"
        );
    }

    #[test]
    fn test_ibc_swap_slippage_exceeded() {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        // amount_out=10 < min_amount_out=80
        let mut deps = make_swap_deps_with_mock_querier(10);

        let result = call_reusable(&mut deps, make_swap_msg(&chain_uid, "tx_slip"), chain_uid);
        assert!(
            matches!(result.unwrap_err(), ContractError::SlippageExceeded { .. }),
            "expected SlippageExceeded"
        );
    }

    // Reorg-replay safety: if a swap packet with the same `tx_id` somehow
    // bypasses the inbound `(chain_uid, sequence)` dedup (e.g. relayer reassigns
    // a fresh sequence to the replayed packet), the application-layer guard at
    // `PENDING_SWAPS.has(tx_id)` must still reject it. Without this guard a
    // reorg could double-process a swap.
    #[test]
    fn test_ibc_swap_same_tx_id_rejected_with_tx_already_exist() {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let mut deps = make_swap_deps_with_mock_querier(90);

        call_reusable(
            &mut deps,
            make_swap_msg(&chain_uid, "tx_dup"),
            chain_uid.clone(),
        )
        .unwrap();
        assert!(PENDING_SWAPS.has(deps.as_ref().storage, "tx_dup".to_string()));

        let err =
            call_reusable(&mut deps, make_swap_msg(&chain_uid, "tx_dup"), chain_uid).unwrap_err();
        assert!(
            matches!(err, ContractError::TxAlreadyExist {}),
            "expected TxAlreadyExist, got: {err:?}"
        );
    }
}
