use cosmwasm_std::{
    ensure, to_json_binary, to_json_string, Addr, Binary, DepsMut, Env, Response, SubMsg, Uint256,
    WasmMsg,
};
use euclid::{
    cross_chain_user::CrossChainUser,
    error::ContractError,
    limit::Limit,
    msgs::cross_chain_config::CrossChainConfig,
    normalize::{normalize_token_to_voucher, normalize_voucher_to_token},
    recipient::Recipient,
    token::Token,
    utils::tx::generate_tx,
};
use euclid_ibc::wire::envelope::factory::FactoryReceiveMsg;
use euclid_ibc::wire::msgs::ReleaseEscrowSendMsg;

use euclid::msgs::virtual_balance::msg::{
    ExecuteBurn, GetEscrowBalanceResponse, QueryMsg as VirtualBalanceQueryMsg,
};

use crate::{
    helpers::release::get_release_fee_storage,
    query::query_token_metadata_by_denom,
    state::{
        PendingReleaseVoucher, CHAIN_TIMEOUT_SECONDS, CHAIN_UID_TO_CHAIN, LOCKED_CHAINS,
        PENDING_RELEASE_VOUCHER, VIRTUAL_BALANCE_CONTRACT,
    },
};

pub fn execute_withdraw_voucher(
    deps: &mut DepsMut,
    env: Env,
    sender: CrossChainUser,
    token: Token,
    amount: Uint256,
    recipient: Recipient,
    cross_chain_config: CrossChainConfig,
) -> Result<Response, ContractError> {
    let virtual_balance_address = VIRTUAL_BALANCE_CONTRACT.load(deps.storage)?;
    let ack_response = cross_chain_config.ack_response;
    let timeout = cross_chain_config.timeout;
    let tx_id = generate_tx(deps, &env, &sender.clone())?;
    let escrow_balance = deps
        .querier
        .query_wasm_smart::<GetEscrowBalanceResponse>(
            virtual_balance_address.clone(),
            &VirtualBalanceQueryMsg::GetEscrowBalance {
                token_id: token.to_string(),
                chain_uid: recipient.recipient.chain_uid.clone(),
                token_type: recipient.denom.clone(),
            },
        )
        .map(|r| r.balance)
        .unwrap_or_default();
    let (msgs, released_amount) = _release_voucher(
        deps,
        &env,
        &virtual_balance_address,
        escrow_balance,
        sender,
        token,
        amount,
        recipient,
        ack_response,
        timeout,
        tx_id,
    )?;
    Ok(Response::new()
        .add_submessages(msgs)
        .add_attribute("released_amount", released_amount.to_string())
        .add_attribute("method", "withdraw_voucher"))
}

pub fn execute_transfer_voucher(
    deps: &mut DepsMut,
    env: Env,
    sender: CrossChainUser,
    token: Token,
    amount: Uint256,
    recipients: Vec<Recipient>,
) -> Result<Response, ContractError> {
    let virtual_balance_address = VIRTUAL_BALANCE_CONTRACT.load(deps.storage)?;

    let mut response = Response::new().add_attribute("recipients", to_json_string(&recipients)?);

    let mut remaining_withdraw_amount = amount;

    let mut recipients_iterator = recipients.into_iter().peekable();
    let mut release_initiated_amount = Uint256::zero();
    let mut transferred_amount = Uint256::zero();

    let mut index = 0;

    // Ensure that the amount desired doesn't exceed the current balance
    while !remaining_withdraw_amount.is_zero() && recipients_iterator.peek().is_some() {
        let recipient = recipients_iterator
            .next()
            .ok_or(ContractError::new("Recipient Iter Failed"))?;

        let release_amount_event_key = format!("release_id_{}_amount", index);
        let transfer_amount_event_key = format!("transfer_id_{}_amount", index);

        // We will transfer vouchers to the recipient
        if recipient.denom.is_voucher() {
            let (transfer_voucher_msgs, transfer_amount) = _transfer_voucher_as_voucher(
                &virtual_balance_address,
                sender.clone(),
                token.clone(),
                remaining_withdraw_amount,
                recipient.clone(),
            )?;
            if transfer_amount.is_zero() {
                continue;
            }
            if !transfer_voucher_msgs.is_empty() {
                response = response.add_submessages(transfer_voucher_msgs);
            }
            response =
                response.add_attribute(transfer_amount_event_key, transfer_amount.to_string());
            remaining_withdraw_amount = remaining_withdraw_amount.checked_sub(transfer_amount)?;
            transferred_amount = transferred_amount.checked_add(transfer_amount)?;
        } else {
            let tx_id = generate_tx(deps, &env, &sender.clone())?;
            let timeout = CHAIN_TIMEOUT_SECONDS
                .may_load(deps.storage, recipient.recipient.chain_uid.clone())?;
            let escrow_balance = deps
                .querier
                .query_wasm_smart::<GetEscrowBalanceResponse>(
                    virtual_balance_address.clone(),
                    &VirtualBalanceQueryMsg::GetEscrowBalance {
                        token_id: token.to_string(),
                        chain_uid: recipient.recipient.chain_uid.clone(),
                        token_type: recipient.denom.clone(),
                    },
                )
                .map(|r| r.balance)
                .unwrap_or_default();
            let (release_msgs, release_amount) = _release_voucher(
                deps,
                &env,
                &virtual_balance_address,
                escrow_balance,
                sender.clone(),
                token.clone(),
                remaining_withdraw_amount,
                recipient.clone(),
                None,
                timeout,
                tx_id,
            )?;
            if release_amount.is_zero() {
                continue;
            }
            response = response
                .add_submessages(release_msgs)
                .add_attribute(release_amount_event_key, release_amount.to_string());
            remaining_withdraw_amount = remaining_withdraw_amount.checked_sub(release_amount)?;
            release_initiated_amount = release_initiated_amount.checked_add(release_amount)?;
        }
        index += 1;
    }
    ensure!(
        transferred_amount
            .checked_add(release_initiated_amount)?
            .checked_add(remaining_withdraw_amount)?
            == amount,
        ContractError::new("Amount mismatch after transfer calculations")
    );
    Ok(response
        .add_attribute("amount", amount.to_string())
        .add_attribute("method", "release_escrow_initiate")
        .add_attribute("token", token.to_string())
        .add_attribute("release_expected", amount.to_string())
        .add_attribute("release_initiated", release_initiated_amount.to_string())
        .add_attribute("transferred_amount", transferred_amount.to_string())
        .add_attribute("unreleased_amount", remaining_withdraw_amount.to_string()))
}

pub fn _transfer_voucher_as_voucher(
    virtual_balance_address: &Addr,
    sender: CrossChainUser,
    token: Token,
    amount: Uint256,
    recipient: Recipient,
) -> Result<(Vec<SubMsg>, Uint256), ContractError> {
    recipient.validate()?;
    ensure!(
        recipient.denom.is_voucher(),
        ContractError::new("Recipient denom is not a voucher")
    );

    let amount = match recipient.amount {
        Limit::LessThanOrEqual(limit) => amount.min(limit),
        Limit::Equal(limit) => amount.min(limit),
        Limit::GreaterThanOrEqual(limit) => {
            ensure!(
                amount.ge(&limit),
                ContractError::InsufficientAmount {
                    min_amount: limit,
                    amount
                }
            );
            amount
        }
        Limit::Dynamic(_) => amount,
    };
    if amount.is_zero() {
        return Ok((vec![], Uint256::zero()));
    }
    let forwarding_msg = match recipient.forwarding_message {
        Some(msg) => Some(Binary::from_base64(msg.as_str())?),
        None => None,
    };
    // If sender is the same as recipient, we don't need to transfer the voucher but return the amount that would have been transferred if it was different so next recipient will be calculated accordingly.
    if sender == recipient.recipient {
        return Ok((vec![], amount));
    }
    let transfer_voucher_msg = euclid::msgs::virtual_balance::msg::ExecuteMsg::Transfer(
        euclid::msgs::virtual_balance::msg::ExecuteTransfer {
            amount,
            token_id: token.to_string(),
            sender: Some(sender.clone()),
            to: recipient.recipient.clone(),
            from: None,
            msg: forwarding_msg,
        },
    );

    let transfer_voucher_msg = WasmMsg::Execute {
        contract_addr: virtual_balance_address.to_string(),
        msg: to_json_binary(&transfer_voucher_msg)?,
        funds: vec![],
    };
    Ok((vec![SubMsg::new(transfer_voucher_msg)], amount))
}

#[allow(clippy::too_many_arguments)]
pub fn _release_voucher(
    deps: &mut DepsMut,
    env: &Env,
    virtual_balance_address: &Addr,
    escrow_balance: Uint256,
    sender: CrossChainUser,
    token: Token,
    voucher_amount: Uint256,
    recipient: Recipient,
    ack_response: Option<Binary>,
    timeout: Option<u64>,
    tx_id: String,
) -> Result<(Vec<SubMsg>, Uint256), ContractError> {
    recipient.validate()?;
    ensure!(
        !recipient.denom.is_voucher(),
        ContractError::new("Recipient denom should not be a voucher")
    );
    let chain = CHAIN_UID_TO_CHAIN.load(deps.storage, recipient.recipient.chain_uid.clone())?;
    let locked_chains = LOCKED_CHAINS.load(deps.storage)?;
    ensure!(
        !locked_chains.contains(&recipient.recipient.chain_uid),
        ContractError::new("Chain is locked")
    );

    let token_metadata = query_token_metadata_by_denom(
        deps.as_ref(),
        virtual_balance_address,
        &token,
        &recipient.recipient.chain_uid,
        &recipient.denom,
    )?;
    // Ensure that denom is valid
    ensure!(
        token_metadata.allowed,
        ContractError::new("Denom not allowed")
    );

    let normalized_token_amount =
        normalize_voucher_to_token(voucher_amount, token_metadata.token_type.get_decimals()?)?;

    if normalized_token_amount.is_zero() && !voucher_amount.is_zero() {
        return Ok((vec![], Uint256::zero()));
    }

    // We cannot release more than escrow balance
    let max_release_amount = normalized_token_amount.min(escrow_balance);

    let release_amount = match recipient.amount {
        Limit::LessThanOrEqual(limit) => max_release_amount.min(limit),
        Limit::Equal(limit) => max_release_amount.min(limit),
        Limit::GreaterThanOrEqual(limit) => {
            ensure!(
                max_release_amount.ge(&limit),
                ContractError::InsufficientAmount {
                    min_amount: limit,
                    amount: max_release_amount,
                }
            );
            max_release_amount
        }
        Limit::Dynamic(_) => {
            ensure!(
                max_release_amount.ge(&normalized_token_amount),
                ContractError::InsufficientAmount {
                    min_amount: normalized_token_amount,
                    amount: max_release_amount
                }
            );
            max_release_amount
        }
    };
    if release_amount.is_zero() {
        return Ok((vec![], Uint256::zero()));
    }

    let release_fee_amount = get_release_fee_storage(deps, &token, &recipient.recipient.chain_uid);
    let release_amount_after_fee = release_amount.checked_sub(release_fee_amount)?;

    PENDING_RELEASE_VOUCHER.save(
        deps.storage,
        tx_id.clone(),
        &PendingReleaseVoucher {
            total_amount: release_amount,
            release_fee_amount,
            unsafe_refund_voucher: recipient.unsafe_refund_as_voucher.unwrap_or(false),
        },
    )?;
    // Prepare IBC Release Message
    let release_ibc_msg = FactoryReceiveMsg::ReleaseEscrow(ReleaseEscrowSendMsg {
        sender: sender.clone(),
        amount: release_amount_after_fee,
        recipient: recipient.recipient.address.clone(),
        denom: recipient.denom.clone(),
        forwarding_message: recipient.forwarding_message.clone(),
        token: token.clone(),
        tx_id,
    })
    .to_msg(
        deps,
        env,
        sender.address.clone(),
        chain,
        timeout,
        ack_response,
    )?;

    // Convert raw release_amount back to voucher units for burn and return
    let voucher_release_amount =
        normalize_token_to_voucher(release_amount, token_metadata.token_type.get_decimals()?)?;

    let burn_voucher_msg = euclid::msgs::virtual_balance::msg::ExecuteMsg::Burn(ExecuteBurn {
        voucher_amount: voucher_release_amount,
        from_user: sender.clone(),
        token_id: token.to_string(),
        release_denom: recipient.denom.clone(),
        release_chain_uid: recipient.recipient.chain_uid.clone(),
    });
    let burn_voucher_msg = WasmMsg::Execute {
        contract_addr: virtual_balance_address.to_string(),
        msg: to_json_binary(&burn_voucher_msg)?,
        funds: vec![],
    };

    // Escrow balance is decremented by virtual_balance during burn

    // Order matters here because we want to burn the vouchers before releasing to prevent any reentrancy attacks.
    Ok((
        vec![SubMsg::new(burn_voucher_msg), release_ibc_msg],
        voucher_release_amount,
    ))
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{
        testing::{message_info, mock_env},
        Addr, Order, Uint256,
    };
    use euclid::{
        chain::ChainUid,
        cross_chain_user::CrossChainUser,
        error::ContractError,
        limit::Limit,
        msgs::cross_chain_config::CrossChainConfig,
        recipient::Recipient,
        token::{Token, TokenType},
    };

    use crate::{
        contract::execute,
        state::{PENDING_RELEASE_VOUCHER, RELEASE_FEES},
        testing::{
            fixtures::{initialized, transfer_deps, voucher_deps},
            helpers::{make_native_recipient, seed_virtual_balance, MockDeps},
        },
    };
    use euclid::msgs::router::ExecuteMsg;
    use rstest::*;

    /// Converts a raw 6-decimal token amount to 24-decimal voucher units.
    fn to_voucher_units(raw: u128) -> Uint256 {
        Uint256::from(raw) * Uint256::from(1_000_000_000_000_000_000u128)
    }

    #[rstest]
    fn test_withdraw_voucher_unregistered_token_fails(mut initialized: MockDeps) {
        let creator = initialized.api.addr_make("creator");

        seed_virtual_balance(&mut initialized);

        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::WithdrawVoucher {
                token: Token::create("usdc".to_string()).unwrap(),
                amount: Uint256::from(100u128),
                recipient: make_native_recipient(
                    ChainUid::create("chain1".to_string()).unwrap(),
                    "recipient",
                    "uusdc",
                    Uint256::from(100u128),
                ),
                cross_chain_config: CrossChainConfig::default(),
            },
        );
        assert!(res.is_err());
    }

    #[test]
    fn test_withdraw_voucher_with_release_fee() {
        let mut deps = voucher_deps();
        let creator = deps.api.addr_make("creator");
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let token = Token::create("usdc".to_string()).unwrap();

        RELEASE_FEES
            .save(
                deps.as_mut().storage,
                (token.clone(), chain_uid.clone()),
                &Uint256::from(10u128),
            )
            .unwrap();

        let res = execute(
            deps.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::WithdrawVoucher {
                token: token.clone(),
                amount: to_voucher_units(200),
                recipient: make_native_recipient(
                    chain_uid.clone(),
                    "recipientaddr",
                    "uusdc",
                    Uint256::from(200u128),
                ),
                cross_chain_config: CrossChainConfig::default(),
            },
        )
        .unwrap();

        let pending: Vec<_> = PENDING_RELEASE_VOUCHER
            .range(deps.as_ref().storage, None, None, Order::Ascending)
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(pending[0].1.total_amount, Uint256::from(200u128));
        assert_eq!(pending[0].1.release_fee_amount, Uint256::from(10u128));

        assert_eq!(
            res.attributes
                .iter()
                .find(|a| a.key == "released_amount")
                .unwrap()
                .value,
            to_voucher_units(200).to_string()
        );
    }
    // -----------------------------------------------------------------------
    // TransferVoucher
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_transfer_voucher_unregistered_token_fails(mut initialized: MockDeps) {
        let creator = initialized.api.addr_make("creator");

        seed_virtual_balance(&mut initialized);

        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::TransferVoucher {
                token: Token::create("usdc".to_string()).unwrap(),
                amount: Uint256::from(100u128),
                recipient: vec![euclid::recipient::Recipient {
                    recipient: euclid::cross_chain_user::CrossChainUser::new(
                        ChainUid::create("unregistered".to_string()).unwrap(),
                        "recipient_addr".to_string(),
                    ),
                    amount: Limit::LessThanOrEqual(Uint256::from(100u128)),
                    denom: TokenType::Native {
                        denom: "uusdc".to_string(),
                        decimals: None,
                    },
                    forwarding_message: None,
                    unsafe_refund_as_voucher: None,
                }],
            },
        );
        assert!(res.is_err());
    }

    #[test]
    fn test_transfer_voucher_to_voucher_recipient_happy_path() {
        let mut deps = transfer_deps();
        let sender = Addr::unchecked("sender_address");
        let token = Token::create("usdc".to_string()).unwrap();

        let res = execute(
            deps.as_mut(),
            mock_env(),
            message_info(&sender, &[]),
            ExecuteMsg::TransferVoucher {
                token,
                amount: Uint256::from(100u128),
                recipient: vec![euclid::recipient::Recipient {
                    recipient: euclid::cross_chain_user::CrossChainUser::new(
                        ChainUid::vsl_chain_uid().unwrap(),
                        "recipient_addr".to_string(),
                    ),
                    amount: Limit::LessThanOrEqual(Uint256::from(100u128)),
                    denom: TokenType::Voucher {},
                    forwarding_message: None,
                    unsafe_refund_as_voucher: None,
                }],
            },
        )
        .unwrap();

        assert_eq!(
            res.messages.len(),
            1,
            "expected 1 virtual balance transfer submsg"
        );
        assert_eq!(
            res.attributes
                .iter()
                .find(|a| a.key == "transferred_amount")
                .unwrap()
                .value,
            "100"
        );
        assert_eq!(
            res.attributes
                .iter()
                .find(|a| a.key == "release_initiated")
                .unwrap()
                .value,
            "0"
        );
    }

    #[test]
    fn test_transfer_voucher_self_transfer_no_submsg() {
        let mut deps = transfer_deps();
        let sender = Addr::unchecked("senderaddr");
        let token = Token::create("usdc".to_string()).unwrap();

        let res = execute(
            deps.as_mut(),
            mock_env(),
            message_info(&sender, &[]),
            ExecuteMsg::TransferVoucher {
                token,
                amount: Uint256::from(100u128),
                recipient: vec![euclid::recipient::Recipient {
                    recipient: euclid::cross_chain_user::CrossChainUser::new(
                        ChainUid::vsl_chain_uid().unwrap(),
                        sender.to_string(),
                    ),
                    amount: Limit::LessThanOrEqual(Uint256::from(100u128)),
                    denom: TokenType::Voucher {},
                    forwarding_message: None,
                    unsafe_refund_as_voucher: None,
                }],
            },
        )
        .unwrap();

        assert!(
            res.messages.is_empty(),
            "expected no submessages for self-transfer"
        );
        assert_eq!(
            res.attributes
                .iter()
                .find(|a| a.key == "transferred_amount")
                .unwrap()
                .value,
            "100"
        );
    }

    // -----------------------------------------------------------------------
    // WithdrawVoucher: escrow-cap and Limit variants
    // -----------------------------------------------------------------------

    /// When the requested amount exceeds the escrow balance the release is
    /// capped at the available escrow.
    #[test]
    fn test_withdraw_voucher_capped_at_escrow_balance() {
        let mut deps = voucher_deps(); // escrow = 500 raw tokens (6 decimals)
        let creator = deps.api.addr_make("creator");
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let token = Token::create("usdc".to_string()).unwrap();

        let res = execute(
            deps.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::WithdrawVoucher {
                token: token.clone(),
                amount: to_voucher_units(1_000),
                recipient: make_native_recipient(
                    chain_uid.clone(),
                    "recipientaddr",
                    "uusdc",
                    Uint256::from(1_000u128),
                ),
                cross_chain_config: CrossChainConfig::default(),
            },
        )
        .unwrap();

        // PENDING_RELEASE records the actual released amount in raw token units (500, not 1000).
        let pending: Vec<_> = PENDING_RELEASE_VOUCHER
            .range(deps.as_ref().storage, None, None, Order::Ascending)
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(pending[0].1.total_amount, Uint256::from(500u128));

        // released_amount attribute is in voucher units (24-decimal).
        assert_eq!(
            res.attributes
                .iter()
                .find(|a| a.key == "released_amount")
                .unwrap()
                .value,
            to_voucher_units(500).to_string()
        );
    }

    /// GreaterThanOrEqual limit fails when the escrow can't satisfy the minimum.
    #[test]
    fn test_withdraw_voucher_gte_limit_fails_when_escrow_too_low() {
        let mut deps = voucher_deps(); // escrow = 500 raw tokens
        let creator = deps.api.addr_make("creator");
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let token = Token::create("usdc".to_string()).unwrap();

        // Request 200 raw tokens worth of vouchers but require at least 600 raw (escrow=500 < 600).
        let res = execute(
            deps.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::WithdrawVoucher {
                token,
                amount: to_voucher_units(200),
                recipient: Recipient {
                    recipient: CrossChainUser::new(chain_uid, "recipientaddr".to_string()),
                    amount: Limit::GreaterThanOrEqual(Uint256::from(600u128)),
                    denom: TokenType::Native {
                        denom: "uusdc".to_string(),
                        decimals: None,
                    },
                    forwarding_message: None,
                    unsafe_refund_as_voucher: None,
                },
                cross_chain_config: CrossChainConfig::default(),
            },
        );
        assert!(matches!(
            res.unwrap_err(),
            ContractError::InsufficientAmount { .. }
        ));
    }

    // -----------------------------------------------------------------------
    // TransferVoucher: multi-recipient sequential allocation
    // -----------------------------------------------------------------------

    /// When multiple recipients are listed, amount flows through them in order;
    /// each recipient absorbs up to its limit and the remainder goes to the next.
    #[test]
    fn test_transfer_voucher_splits_amount_across_recipients() {
        let mut deps = transfer_deps();
        let sender = Addr::unchecked("sender_address");
        let token = Token::create("usdc".to_string()).unwrap();
        let vsl_chain = ChainUid::vsl_chain_uid().unwrap();

        // recipient1 takes up to 60, recipient2 takes the remainder (40).
        let res = execute(
            deps.as_mut(),
            mock_env(),
            message_info(&sender, &[]),
            ExecuteMsg::TransferVoucher {
                token,
                amount: Uint256::from(100u128),
                recipient: vec![
                    Recipient {
                        recipient: CrossChainUser::new(vsl_chain.clone(), "addr_one".to_string()),
                        amount: Limit::LessThanOrEqual(Uint256::from(60u128)),
                        denom: TokenType::Voucher {},
                        forwarding_message: None,
                        unsafe_refund_as_voucher: None,
                    },
                    Recipient {
                        recipient: CrossChainUser::new(vsl_chain, "addr_two".to_string()),
                        amount: Limit::LessThanOrEqual(Uint256::from(100u128)),
                        denom: TokenType::Voucher {},
                        forwarding_message: None,
                        unsafe_refund_as_voucher: None,
                    },
                ],
            },
        )
        .unwrap();

        // Two separate virtual-balance transfer submessages.
        assert_eq!(
            res.messages.len(),
            2,
            "expected one submsg per non-self recipient"
        );
        // Amounts reflected in attributes.
        let attrs: std::collections::HashMap<_, _> = res
            .attributes
            .iter()
            .map(|a| (a.key.as_str(), a.value.as_str()))
            .collect();
        assert_eq!(attrs["transfer_id_0_amount"], "60");
        assert_eq!(attrs["transfer_id_1_amount"], "40");
        assert_eq!(attrs["transferred_amount"], "100");
    }

    // -----------------------------------------------------------------------
    // Truncation guard (Bug #4 from review)
    // -----------------------------------------------------------------------

    /// Voucher amount too small to produce any raw tokens after normalization
    /// Dust amounts that normalize to zero are skipped gracefully, not errored.
    #[test]
    fn test_withdraw_voucher_dust_amount_skips() {
        let mut deps = voucher_deps(); // 6-decimal token, escrow = 500
        let creator = deps.api.addr_make("creator");
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let token = Token::create("usdc".to_string()).unwrap();

        // 999 voucher units (24-dec). normalize_voucher_to_token(999, 6) = 999 / 10^18 = 0.
        let dust_amount = Uint256::from(999u128);

        let res = execute(
            deps.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::WithdrawVoucher {
                token,
                amount: dust_amount,
                recipient: make_native_recipient(
                    chain_uid,
                    "recipientaddr",
                    "uusdc",
                    Uint256::from(999u128),
                ),
                cross_chain_config: CrossChainConfig::default(),
            },
        )
        .unwrap();

        assert_eq!(
            res.attributes
                .iter()
                .find(|a| a.key == "released_amount")
                .map(|a| a.value.as_str()),
            Some("0")
        );
    }
}
