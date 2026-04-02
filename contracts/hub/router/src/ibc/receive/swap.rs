use cosmwasm_std::{ensure, to_json_binary, DepsMut, Env, Response, SubMsg, Uint128, WasmMsg};
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
    voucher::BalanceKey,
};
use euclid_ibc::router_ibc::RouterCrossChainSwapExecuteMsg;

use crate::{
    query::validate_swap_pairs,
    reply::SWAP_REPLY_ID,
    state::{ESCROW_BALANCES, PENDING_SWAPS, VIRTUAL_BALANCE_CONTRACT},
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

    let virtual_balance_address = VIRTUAL_BALANCE_CONTRACT.load(deps.storage)?.to_string();

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

    // Simulation increases gas, ideally this can be resolved but we are still getting codespace wasm errors so this is added as a temporary fix for better error messages
    let simulate_swap_msg = euclid::msgs::vlp::base::QueryMsg::SimulateSwap(VlpSimulateSwapMsg {
        asset: msg.asset_in.token.clone(),
        asset_amount: msg.amount_in,
        swaps: next_swaps.to_vec(),
    });

    let simulate_swap_res: euclid::msgs::vlp::base::GetSwapQueryResponse = deps
        .querier
        .query_wasm_smart(first_swap.vlp_address.clone(), &simulate_swap_msg)?;

    ensure!(
        simulate_swap_res.amount_out.ge(&msg.min_amount_out),
        ContractError::SlippageExceeded {
            amount: simulate_swap_res.amount_out,
            min_amount_out: msg.min_amount_out,
        }
    );

    // Mint voucher token in escrow balance if it is not a voucher token
    if !msg.asset_in.token_type.is_voucher() {
        let token_escrow_key = (msg.asset_in.token.to_string(), sender.chain_uid.clone());
        let token_escrow_balance = ESCROW_BALANCES
            .may_load(deps.storage, token_escrow_key.clone())?
            .unwrap_or(Uint128::zero());

        ESCROW_BALANCES.save(
            deps.storage,
            token_escrow_key,
            &token_escrow_balance.checked_add(msg.amount_in)?,
        )?;

        // Mint virtual balance for the first swap vlp so it can start processing tx
        let mint_virtual_balance_msg =
            euclid::msgs::virtual_balance::msg::ExecuteMsg::Mint(ExecuteMint {
                amount: msg.amount_in,
                balance_key: BalanceKey {
                    cross_chain_user: sender.clone(),
                    token_id: msg.asset_in.token.to_string(),
                },
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
            user_voucher_balance_res.amount.ge(&msg.amount_in),
            ContractError::InsufficientAmount {
                min_amount: msg.amount_in,
                amount: user_voucher_balance_res.amount,
            }
        );
    }

    let approve_voucher_msg =
        euclid::msgs::virtual_balance::msg::ExecuteMsg::Approve(ExecuteApprove {
            amount: msg.amount_in,
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

    if msg.asset_in.token_type.is_voucher()
        && !msg.partner_fee_amount.is_zero()
        && msg.partner_fee_recipient != sender
    {
        let transfer_voucher_msg =
            euclid::msgs::virtual_balance::msg::ExecuteMsg::Transfer(ExecuteTransfer {
                amount: msg.partner_fee_amount,
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
    //     let liquidity_response: GetLiquidityResponse = deps.querier.query(
    //         &cosmwasm_std::QueryRequest::Wasm(cosmwasm_std::WasmQuery::Smart {
    //             contract_addr: first_swap.vlp_address.clone(),
    //             msg: to_json_binary(&euclid::msgs::stable_vlp::QueryMsg::Liquidity {})?,
    //         }),
    //     )?;
    //    let swap_msg =  if liquidity_response.token_1_reserve == liquidity_response.token_2_reserve {
    //         return Err(ContractError::Generic {
    //             err: "Liquidity is not enough".to_string(),
    //         });
    //     } else {

    //     }

    let swap_msg = msgs::vlp::base::ExecuteMsg::Swap(VlpSwapMsg {
        sender: sender.clone(),
        asset_in: msg.asset_in.token.clone(),
        amount_in: msg.amount_in,
        min_token_out: msg.min_amount_out,
        next_swaps: next_swaps.to_vec(),
        tx_id: msg.tx_id.clone(),
        test_fail: first_swap.test_fail,
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
    use cosmwasm_std::Uint128;
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
        state::{ESCROW_BALANCES, PENDING_SWAPS},
        tests::tests::tests::{call_reusable, make_swap_deps_with_mock_querier},
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
                },
            },
            amount_in: Uint128::new(100),
            asset_out: token_b.clone(),
            min_amount_out: Uint128::new(80),
            swaps: vec![NextSwapPair {
                token_in: token_a,
                token_out: token_b,
                test_fail: None,
            }],
            recipients: vec![],
            partner_fee_amount: Uint128::zero(),
            partner_fee_recipient: sender,
            tx_id: tx_id.to_string(),
        })
    }

    #[test]
    fn test_ibc_swap_saves_pending_and_emits_submsg() {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let mut deps = make_swap_deps_with_mock_querier(90);
        let token_a = Token::create("aaa".to_string()).unwrap();

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
        let escrow = ESCROW_BALANCES
            .load(deps.as_ref().storage, (token_a.to_string(), chain_uid))
            .unwrap();
        assert_eq!(
            escrow,
            Uint128::new(100),
            "escrow balance should equal amount_in"
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
}
