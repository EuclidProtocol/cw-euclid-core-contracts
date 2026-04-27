use cosmwasm_std::{ensure, Decimal, DepsMut, Env, MessageInfo, Response, Uint256};
use euclid::{
    cross_chain_user::CrossChainUser,
    error::ContractError,
    events::{simple_event, swap_event, tx_event},
    fee::{PartnerFee, MAX_PARTNER_FEE_BPS},
    msgs::cross_chain_config::CrossChainConfig,
    recipient::Recipient,
    swap::{NextSwapPair, SwapRequest},
    token::{Token, TokenType, TokenWithDenom},
    utils::{fund_manager::FundManager, tx::generate_tx},
};
use euclid_ibc::router_ibc::{RouterCrossChainExecuteMsg, RouterCrossChainSwapExecuteMsg};

use crate::{
    query::get_chain_type,
    state::{PENDING_SWAPS, STATE, TOKEN_TO_ESCROW},
};

pub fn execute_swap_request(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    sender: CrossChainUser,
    asset_in: TokenWithDenom,
    amount_in: Uint256,
    asset_out: Token,
    min_amount_out: Uint256,
    swaps: Vec<NextSwapPair>,
    recipients: Vec<Recipient>,
    cross_chain_config: CrossChainConfig,
    partner_fee: Option<PartnerFee>,
) -> Result<Response, ContractError> {
    // Reject mixed-case or empty addresses before mutating state
    sender.validate()?;
    // Validate asset in
    asset_in.token_type.validate(&deps.as_ref())?;
    asset_in.token.validate()?;

    let state = STATE.load(deps.storage)?;
    let sender_addr = deps.api.addr_validate(&sender.address)?;

    let tx_id = generate_tx(deps, &env, &sender)?;

    let partner_fee_bps = partner_fee
        .clone()
        .map(|fee| fee.partner_fee_bps)
        .unwrap_or(0);

    ensure!(
        partner_fee_bps <= MAX_PARTNER_FEE_BPS,
        ContractError::InvalidPartnerFee {}
    );

    if !asset_in.token_type.is_voucher() {
        // Verify that this asset is allowed
        let escrow = TOKEN_TO_ESCROW.load(deps.storage, asset_in.token.clone())?;

        let token_allowed: euclid::msgs::escrow::AllowedTokenResponse =
            deps.querier.query_wasm_smart(
                escrow,
                &euclid::msgs::escrow::QueryMsg::TokenAllowed {
                    denom: asset_in.token_type.clone(),
                },
            )?;
        ensure!(
            token_allowed.allowed,
            ContractError::UnsupportedDenomination {}
        );
    }

    let mut fund_manager = FundManager::new(&info.funds);
    match &asset_in.token_type {
        TokenType::Native { denom, .. } => {
            // Verify thatthe amount of funds passed is greater than the asset amount
            fund_manager.use_fund(amount_in, denom)?;
        }
        TokenType::Smart {
            contract_address, ..
        } => {
            ensure!(
                info.sender.to_string() == *contract_address,
                ContractError::Unauthorized {}
            );
        }
        TokenType::Voucher { .. } => {}
    }
    ensure!(
        fund_manager.validate_funds_are_empty().is_ok(),
        ContractError::new("Extra funds sent with message")
    );

    let partner_fee_amount = amount_in.checked_mul_ceil(Decimal::bps(partner_fee_bps))?;

    let amount_in = amount_in.checked_sub(partner_fee_amount)?;
    // Verify that the asset amount is greater than 0
    ensure!(!amount_in.is_zero(), ContractError::ZeroAssetAmount {});

    // Verify that the min amount out is greater than 0
    ensure!(!min_amount_out.is_zero(), ContractError::ZeroAssetAmount {});

    ensure!(
        !PENDING_SWAPS.has(deps.storage, (sender_addr.clone(), tx_id.clone())),
        ContractError::TxAlreadyExist {}
    );

    let first_swap = swaps.first().ok_or(ContractError::Generic {
        err: "Empty Swap not allowed".to_string(),
    })?;

    ensure!(
        first_swap.token_in == asset_in.token,
        ContractError::new("Token in doesn't match swap route")
    );

    let last_swap = swaps.last().ok_or(ContractError::Generic {
        err: "Empty Swap not allowed".to_string(),
    })?;

    ensure!(
        last_swap.token_out == asset_out,
        ContractError::new("Token out doesn't match swap route")
    );

    let partner_fee_recipient = partner_fee
        .clone()
        .map(|partner_fee| deps.api.addr_validate(&partner_fee.recipient))
        .transpose()?
        .unwrap_or(sender_addr.clone());

    let swap_info = SwapRequest {
        sender: sender_addr.to_string(),
        asset_in: asset_in.clone(),
        amount_in,
        asset_out: asset_out.clone(),
        min_amount_out,
        swaps: swaps.clone(),
        tx_id: tx_id.clone(),
        recipients: recipients.clone(),
        partner_fee_amount,
        partner_fee_recipient: partner_fee_recipient.clone(),
    };
    PENDING_SWAPS.save(
        deps.storage,
        (sender_addr.clone(), tx_id.clone()),
        &swap_info,
    )?;

    let chain_type = get_chain_type(deps.as_ref(), &env)?;

    let asset_in_id = asset_in.token.to_string();
    let asset_out_id = asset_out.to_string();

    let swap_msg = RouterCrossChainExecuteMsg::Swap(RouterCrossChainSwapExecuteMsg {
        sender,
        asset_in,
        amount_in,
        asset_out,
        min_amount_out,
        swaps,
        tx_id: tx_id.clone(),
        recipients,
        partner_fee_recipient: CrossChainUser::new(
            state.chain_uid.clone(),
            partner_fee_recipient.to_string(),
        ),
        partner_fee_amount,
    })
    .to_msg(
        deps,
        &env,
        state.router_contract.clone(),
        sender_addr.clone(),
        state.chain_uid.clone(),
        chain_type,
        cross_chain_config.timeout,
        cross_chain_config.ack_response,
    )?;

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            sender_addr.as_str(),
            euclid::events::TxType::Swap,
        ))
        .add_event(swap_event(&tx_id, &swap_info))
        .add_event(simple_event().add_attribute(
            "meta",
            cross_chain_config.meta.unwrap_or("no_meta".to_string()),
        ))
        .add_attribute("action", "swap")
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "execute_request_swap")
        .add_attribute("asset_in", asset_in_id)
        .add_attribute("asset_out", asset_out_id)
        .add_attribute("amount_in", amount_in)
        .add_submessage(swap_msg))
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{
        testing::{message_info, mock_dependencies, mock_env},
        Uint128, Uint256,
    };
    use euclid::{
        error::ContractError,
        msgs::factory::{ExecuteMsg, ExecuteSwapRequest},
        swap::NextSwapPair,
        token::{Token, TokenType, TokenWithDenom},
    };

    use crate::{
        contract::execute,
        testing::helpers::{
            assert_attribute, assert_euclid_action, assert_tx_event_full,
            default_cross_chain_config, get_attribute, init, seed_escrow, set_escrow_token_allowed,
        },
    };

    fn make_voucher_swap_msg(amount_in: Uint256, min_amount_out: Uint256) -> ExecuteMsg {
        let token_in = Token::create("usdc".to_string()).unwrap();
        let token_out = Token::create("eth".to_string()).unwrap();

        ExecuteMsg::ExecuteSwapRequest(ExecuteSwapRequest {
            asset_in: TokenWithDenom {
                token: token_in.clone(),
                token_type: TokenType::Voucher {},
            },
            amount_in,
            asset_out: token_out.clone(),
            min_amount_out,
            swaps: vec![NextSwapPair {
                token_in: token_in.clone(),
                token_out: token_out.clone(),
                test_fail: None,
            }],
            recipients: vec![],
            partner_fee: None,
            cross_chain_config: default_cross_chain_config(),
        })
    }

    // -----------------------------------------------------------------------
    // Execute: ExecuteSwapRequest
    // -----------------------------------------------------------------------

    #[test]
    fn test_swap_request_zero_min_amount_out_fails() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let sender = deps.api.addr_make("sender");
        let info = message_info(&sender, &[]);
        let msg = make_voucher_swap_msg(Uint256::from(100u128), Uint256::zero());
        let res = execute(deps.as_mut(), mock_env(), info, msg);
        assert_eq!(res.unwrap_err(), ContractError::ZeroAssetAmount {});
    }

    #[test]
    fn test_swap_request_empty_swaps_fails() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let sender = deps.api.addr_make("sender");
        let info = message_info(&sender, &[]);
        let msg = ExecuteMsg::ExecuteSwapRequest(ExecuteSwapRequest {
            asset_in: TokenWithDenom {
                token: Token::create("usdc".to_string()).unwrap(),
                token_type: TokenType::Voucher {},
            },
            amount_in: Uint256::from(100u128),
            asset_out: Token::create("eth".to_string()).unwrap(),
            min_amount_out: Uint256::from(1u128),
            swaps: vec![],
            recipients: vec![],
            partner_fee: None,
            cross_chain_config: default_cross_chain_config(),
        });
        let res = execute(deps.as_mut(), mock_env(), info, msg);
        assert!(matches!(res.unwrap_err(), ContractError::Generic { .. }));
    }

    #[test]
    fn test_swap_request_mismatched_token_in_fails() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let sender = deps.api.addr_make("sender");
        let info = message_info(&sender, &[]);

        let asset_in_token = Token::create("usdc".to_string()).unwrap();
        let wrong_token_in = Token::create("wbtc".to_string()).unwrap();
        let token_out = Token::create("eth".to_string()).unwrap();

        let msg = ExecuteMsg::ExecuteSwapRequest(ExecuteSwapRequest {
            asset_in: TokenWithDenom {
                token: asset_in_token.clone(),
                token_type: TokenType::Voucher {},
            },
            amount_in: Uint256::from(100u128),
            asset_out: token_out.clone(),
            min_amount_out: Uint256::from(1u128),
            swaps: vec![NextSwapPair {
                token_in: wrong_token_in,
                token_out: token_out.clone(),
                test_fail: None,
            }],
            recipients: vec![],
            partner_fee: None,
            cross_chain_config: default_cross_chain_config(),
        });
        let res = execute(deps.as_mut(), mock_env(), info, msg);
        assert!(matches!(res.unwrap_err(), ContractError::Generic { .. }));
    }

    #[test]
    fn test_swap_request_mismatched_token_out_fails() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let sender = deps.api.addr_make("sender");
        let info = message_info(&sender, &[]);

        let asset_in_token = Token::create("usdc".to_string()).unwrap();
        let token_out = Token::create("eth".to_string()).unwrap();
        let wrong_token_out = Token::create("wbtc".to_string()).unwrap();

        let msg = ExecuteMsg::ExecuteSwapRequest(ExecuteSwapRequest {
            asset_in: TokenWithDenom {
                token: asset_in_token.clone(),
                token_type: TokenType::Voucher {},
            },
            amount_in: Uint256::from(100u128),
            asset_out: token_out.clone(),
            min_amount_out: Uint256::from(1u128),
            swaps: vec![NextSwapPair {
                token_in: asset_in_token.clone(),
                token_out: wrong_token_out,
                test_fail: None,
            }],
            recipients: vec![],
            partner_fee: None,
            cross_chain_config: default_cross_chain_config(),
        });
        let res = execute(deps.as_mut(), mock_env(), info, msg);
        assert!(matches!(res.unwrap_err(), ContractError::Generic { .. }));
    }

    #[test]
    fn test_swap_request_voucher_happy_path_writes_pending() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let token_in = Token::create("usdc".to_string()).unwrap();
        let token_out = Token::create("eth".to_string()).unwrap();
        let amount_in = Uint256::from(100u128);

        let sender = deps.api.addr_make("sender");
        let info = message_info(&sender, &[]);
        let msg = make_voucher_swap_msg(amount_in, Uint256::from(1u128));
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        assert!(res
            .attributes
            .iter()
            .any(|a| a.key == "method" && a.value == "execute_request_swap"));

        let tx_id = get_attribute(&res, "tx_id").to_owned();

        let pending = crate::state::PENDING_SWAPS
            .load(&deps.storage, (sender.clone(), tx_id.clone()))
            .unwrap();
        assert_eq!(pending.tx_id, tx_id);
        assert_eq!(pending.amount_in, amount_in);

        assert_attribute(&res, "action", "swap");
        assert_eq!(get_attribute(&res, "asset_in"), token_in.to_string());
        assert_eq!(get_attribute(&res, "asset_out"), token_out.to_string());
        assert_eq!(get_attribute(&res, "amount_in"), amount_in.to_string());
        assert_eq!(get_attribute(&res, "tx_id"), tx_id);
        assert_tx_event_full(&res, "swap", &tx_id, sender.as_str());
        assert_euclid_action(&res, "swap");
    }

    // -----------------------------------------------------------------------
    // Execute: ExecuteSwapRequest – partner fee validation
    // -----------------------------------------------------------------------

    #[test]
    fn test_swap_request_partner_fee_too_high_fails() {
        use euclid::fee::PartnerFee;

        let mut deps = mock_dependencies();
        init(&mut deps);

        let sender = deps.api.addr_make("sender");
        let info = message_info(&sender, &[]);

        let token_in = Token::create("usdc".to_string()).unwrap();
        let token_out = Token::create("eth".to_string()).unwrap();

        let msg = ExecuteMsg::ExecuteSwapRequest(ExecuteSwapRequest {
            asset_in: TokenWithDenom {
                token: token_in.clone(),
                token_type: TokenType::Voucher {},
            },
            amount_in: Uint256::from(100u128),
            asset_out: token_out.clone(),
            min_amount_out: Uint256::from(1u128),
            swaps: vec![NextSwapPair {
                token_in: token_in.clone(),
                token_out: token_out.clone(),
                test_fail: None,
            }],
            recipients: vec![],
            partner_fee: Some(PartnerFee {
                partner_fee_bps: 10_001,
                recipient: sender.to_string(),
            }),
            cross_chain_config: default_cross_chain_config(),
        });
        let res = execute(deps.as_mut(), mock_env(), info, msg);
        assert_eq!(res.unwrap_err(), ContractError::InvalidPartnerFee {});
    }

    // -----------------------------------------------------------------------
    // Execute: ExecuteSwapRequest – native token requires funds
    // -----------------------------------------------------------------------

    #[test]
    fn test_swap_request_native_token_requires_funds() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        seed_escrow(&mut deps, "usdc", "escrow_usdc");
        set_escrow_token_allowed(&mut deps, true);
        deps.querier
            .bank
            .update_balance("any", vec![cosmwasm_std::coin(1_000_000, "uusdc")]);

        let sender = deps.api.addr_make("sender");
        let info = message_info(&sender, &[]);

        let token_in = Token::create("usdc".to_string()).unwrap();
        let token_out = Token::create("eth".to_string()).unwrap();

        let msg = ExecuteMsg::ExecuteSwapRequest(ExecuteSwapRequest {
            asset_in: TokenWithDenom {
                token: token_in.clone(),
                token_type: TokenType::Native {
                    denom: "uusdc".to_string(),
                    decimals: None,
                },
            },
            amount_in: Uint256::from(100u128),
            asset_out: token_out.clone(),
            min_amount_out: Uint256::from(1u128),
            swaps: vec![NextSwapPair {
                token_in: token_in.clone(),
                token_out: token_out.clone(),
                test_fail: None,
            }],
            recipients: vec![],
            partner_fee: None,
            cross_chain_config: default_cross_chain_config(),
        });
        let res = execute(deps.as_mut(), mock_env(), info, msg);
        assert_eq!(res.unwrap_err(), ContractError::InsufficientFunds {});
    }

    // -----------------------------------------------------------------------
    // State invariant: PENDING_SWAPS accumulates across two different senders
    // -----------------------------------------------------------------------

    #[test]
    fn test_pending_swaps_accumulated_across_two_different_senders() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let alice = deps.api.addr_make("alice");
        let bob = deps.api.addr_make("bob");

        for sender_addr in [alice.clone(), bob.clone()] {
            let info = message_info(&sender_addr, &[]);
            let msg = make_voucher_swap_msg(Uint256::from(100u128), Uint256::from(1u128));
            let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
            let tx_id = res
                .attributes
                .iter()
                .find(|a| a.key == "tx_id")
                .unwrap()
                .value
                .clone();
            let pending = crate::state::PENDING_SWAPS
                .load(&deps.storage, (sender_addr, tx_id))
                .unwrap();
            assert_eq!(pending.amount_in, Uint256::from(100u128));
        }
    }
}
