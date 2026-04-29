use cosmwasm_std::{
    ensure, to_json_binary, CosmosMsg, DepsMut, Env, Response, SubMsg, Uint128, WasmMsg,
};
use euclid::{
    cross_chain_user::CrossChainUser,
    deposit::DepositTokenResponse,
    error::ContractError,
    events::{deregister_denom_event, register_denom_event, tx_event, TxType},
    msgs::{
        router::TokenDenom,
        virtual_balance::msg::{ExecuteMint, ExecuteMsg as VirtualBalanceMsg},
        vlp::base::{DeregisterDenomResponse, RegisterDenomResponse},
    },
    swap::TransferVoucherResponse,
    token::TokenWithDenom,
    voucher::BalanceKey,
};
use euclid_ibc::{
    ack::AcknowledgementMsg,
    router_ibc::{
        RouterCrossChainDepositTokenExecuteMsg, RouterCrossChainTransferVoucherExecuteMsg,
    },
};

use crate::{
    execute::token::execute_transfer_voucher,
    state::{ESCROW_BALANCES, TOKEN_DENOMS, VIRTUAL_BALANCE_CONTRACT},
};

pub fn ibc_execute_register_denom(
    deps: DepsMut,
    _env: Env,
    sender: CrossChainUser,
    token: TokenWithDenom,
    tx_id: String,
) -> Result<Response, ContractError> {
    token.token.validate()?;

    let mut token_denoms = TOKEN_DENOMS
        .load(deps.storage, token.token.clone())
        .unwrap_or_default();

    let token_exists = token_denoms
        .iter()
        .any(|denom| denom.chain_uid == sender.chain_uid && denom.token_type == token.token_type);

    ensure!(!token_exists, ContractError::TokenAlreadyExist {});

    token_denoms.push(TokenDenom {
        chain_uid: sender.chain_uid.clone(),
        token_type: token.token_type.clone(),
    });
    TOKEN_DENOMS.save(deps.storage, token.token.clone(), &token_denoms)?;

    let ack: AcknowledgementMsg<RegisterDenomResponse> =
        AcknowledgementMsg::Ok(RegisterDenomResponse {});

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            &sender.to_sender_string(),
            TxType::RegisterDenom,
        ))
        .add_event(register_denom_event(
            &token.token,
            &sender.chain_uid.to_string(),
            &token.token_type,
        ))
        .add_attribute("tx_id", tx_id)
        .add_attribute("action", "execute_register_denom")
        .set_data(to_json_binary(&ack)?))
}

pub fn ibc_execute_deregister_denom(
    deps: DepsMut,
    _env: Env,
    sender: CrossChainUser,
    token: TokenWithDenom,
    tx_id: String,
) -> Result<Response, ContractError> {
    let mut token_denoms = TOKEN_DENOMS
        .load(deps.storage, token.token.clone())
        .unwrap_or_default();

    let token_exists = token_denoms
        .iter()
        .any(|denom| denom.chain_uid == sender.chain_uid && denom.token_type == token.token_type);

    ensure!(token_exists, ContractError::AssetDoesNotExist {});

    // Remove the denom from list
    token_denoms.retain(|denom| {
        denom.chain_uid != sender.chain_uid || denom.token_type != token.token_type
    });

    TOKEN_DENOMS.save(deps.storage, token.token.clone(), &token_denoms)?;

    let ack: AcknowledgementMsg<DeregisterDenomResponse> =
        AcknowledgementMsg::Ok(DeregisterDenomResponse {});

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            &sender.to_sender_string(),
            TxType::DeregisterDenom,
        ))
        .add_attribute("tx_id", tx_id)
        .add_attribute("action", "execute_deregister_denom")
        .add_event(deregister_denom_event(
            &token.token,
            &sender.chain_uid.to_string(),
            &token.token_type,
        ))
        .set_data(to_json_binary(&ack)?))
}

pub fn ibc_execute_deposit_token(
    deps: &mut DepsMut,
    env: Env,
    msg: RouterCrossChainDepositTokenExecuteMsg,
) -> Result<Response, ContractError> {
    let sender = msg.sender.clone();

    // Add token 1 in escrow balance
    let token_escrow_key = (msg.asset_in.token.to_string(), sender.chain_uid.clone());
    let token_escrow_balance = ESCROW_BALANCES
        .may_load(deps.storage, token_escrow_key.clone())?
        .unwrap_or(Uint128::zero());

    let new_escrow_balance = token_escrow_balance.checked_add(msg.amount_in)?;

    ESCROW_BALANCES.save(deps.storage, token_escrow_key, &new_escrow_balance)?;

    let deposit_token_response = DepositTokenResponse {
        amount: msg.amount_in,
        token: msg.asset_in.token.clone(),
        sender: msg.sender.clone(),
    };
    let ack = AcknowledgementMsg::Ok(deposit_token_response.clone());

    // Load state to get virtual balance address
    let virtual_balance_address = VIRTUAL_BALANCE_CONTRACT.load(deps.storage)?;

    // Send mint msg to virtual balance
    let mint_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: virtual_balance_address.to_string(),
        msg: to_json_binary(&VirtualBalanceMsg::Mint(ExecuteMint {
            amount: msg.amount_in,
            balance_key: BalanceKey {
                cross_chain_user: msg.sender.clone(),
                token_id: msg.asset_in.token.to_string(),
            },
        }))?,
        funds: vec![],
    });

    let response = Response::new()
        .add_submessage(SubMsg::new(mint_msg))
        .add_attribute("action", "reply_deposit_token")
        .add_attribute(
            "deposit_token_response",
            format!("{deposit_token_response:?}"),
        )
        .add_event(
            tx_event(
                &msg.tx_id,
                &msg.sender.to_sender_string(),
                TxType::DepositToken,
            )
            .add_attribute("tx_id", msg.tx_id.clone()),
        )
        .add_attribute("chain_uid", sender.chain_uid.to_string())
        .add_attribute(
            format!(
                "escrow_added_token_{token}_denom_{denom}",
                token = msg.asset_in.token,
                denom = msg.asset_in.token_type.get_key()
            ),
            msg.amount_in,
        )
        .add_attribute(
            format!(
                "escrow_balance_token_{token}_denom_{denom}",
                token = msg.asset_in.token,
                denom = msg.asset_in.token_type.get_key()
            ),
            new_escrow_balance,
        );

    let transfer_response = execute_transfer_voucher(
        deps,
        env,
        sender,
        msg.asset_in.token,
        msg.amount_in,
        msg.recipients,
    )?;

    let response = response
        .add_submessages(transfer_response.messages)
        .add_attributes(transfer_response.attributes)
        .add_events(transfer_response.events);

    Ok(response.set_data(to_json_binary(&ack)?))
}

pub fn ibc_execute_transfer_virtual_balance(
    deps: &mut DepsMut,
    env: Env,
    msg: RouterCrossChainTransferVoucherExecuteMsg,
) -> Result<Response, ContractError> {
    let sender = msg.sender.clone();

    let response = execute_transfer_voucher(
        deps,
        env,
        sender,
        msg.token.clone(),
        msg.amount,
        msg.recipients,
    )?;
    Ok(response
        .add_attribute("action", "transfer_virtual_balance")
        .add_event(
            tx_event(
                &msg.tx_id,
                &msg.sender.to_sender_string(),
                TxType::TransferVoucher,
            )
            .add_attribute("tx_id", msg.tx_id.clone()),
        )
        .set_data(to_json_binary(&AcknowledgementMsg::Ok(
            TransferVoucherResponse {
                token: msg.token,
                tx_id: msg.tx_id,
            },
        ))?))
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::Uint128;
    use euclid::{
        chain::ChainUid,
        cross_chain_user::CrossChainUser,
        error::ContractError,
        limit::Limit,
        msgs::router::TokenDenom,
        recipient::Recipient,
        token::{Token, TokenType, TokenWithDenom},
    };
    use euclid_ibc::router_ibc::{
        RouterCrossChainDepositTokenExecuteMsg, RouterCrossChainExecuteMsg,
        RouterCrossChainTransferVoucherExecuteMsg,
    };

    use crate::{
        state::{ESCROW_BALANCES, TOKEN_DENOMS},
        testing::{
            fixtures::initialized,
            helpers::{call_reusable, register_denom_msg, seed_virtual_balance, MockDeps},
        },
    };

    use rstest::*;

    // -----------------------------------------------------------------------
    // RegisterDenom / DeregisterDenom dispatch
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_ibc_register_denom_saves_token_denoms(mut initialized: MockDeps) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let token = Token::create("usdc".to_string()).unwrap();

        call_reusable(
            &mut initialized,
            register_denom_msg(chain_uid.clone()),
            chain_uid.clone(),
        )
        .unwrap();

        let denoms = TOKEN_DENOMS
            .load(initialized.as_ref().storage, token)
            .unwrap();
        assert_eq!(denoms.len(), 1);
        assert_eq!(denoms[0].chain_uid, chain_uid);
        assert_eq!(
            denoms[0].token_type,
            TokenType::Native {
                denom: "uusdc".to_string()
            }
        );
    }

    #[rstest]
    fn test_ibc_register_denom_duplicate_fails(mut initialized: MockDeps) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let token = Token::create("usdc".to_string()).unwrap();

        TOKEN_DENOMS
            .save(
                initialized.as_mut().storage,
                token,
                &vec![TokenDenom {
                    chain_uid: chain_uid.clone(),
                    token_type: TokenType::Native {
                        denom: "uusdc".to_string(),
                    },
                }],
            )
            .unwrap();

        let result = call_reusable(
            &mut initialized,
            register_denom_msg(chain_uid.clone()),
            chain_uid,
        );
        assert_eq!(result.unwrap_err(), ContractError::TokenAlreadyExist {});
    }

    #[rstest]
    fn test_ibc_deregister_denom_removes_entry(mut initialized: MockDeps) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let token = Token::create("usdc".to_string()).unwrap();

        TOKEN_DENOMS
            .save(
                initialized.as_mut().storage,
                token.clone(),
                &vec![TokenDenom {
                    chain_uid: chain_uid.clone(),
                    token_type: TokenType::Native {
                        denom: "uusdc".to_string(),
                    },
                }],
            )
            .unwrap();

        let msg = RouterCrossChainExecuteMsg::DeregisterDenom {
            sender: CrossChainUser::new(chain_uid.clone(), "user".to_string()),
            token: TokenWithDenom {
                token: token.clone(),
                token_type: TokenType::Native {
                    denom: "uusdc".to_string(),
                },
            },
            tx_id: "tx1".to_string(),
        };
        call_reusable(&mut initialized, msg, chain_uid).unwrap();

        let denoms = TOKEN_DENOMS
            .load(initialized.as_ref().storage, token)
            .unwrap();
        assert!(
            denoms.is_empty(),
            "entry should be removed after deregister"
        );
    }

    #[rstest]
    fn test_ibc_deregister_denom_not_found_fails(mut initialized: MockDeps) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let token = Token::create("usdc".to_string()).unwrap();

        // No entry for this chain
        TOKEN_DENOMS
            .save(initialized.as_mut().storage, token.clone(), &vec![])
            .unwrap();

        let msg = RouterCrossChainExecuteMsg::DeregisterDenom {
            sender: CrossChainUser::new(chain_uid.clone(), "user".to_string()),
            token: TokenWithDenom {
                token,
                token_type: TokenType::Native {
                    denom: "uusdc".to_string(),
                },
            },
            tx_id: "tx1".to_string(),
        };
        let result = call_reusable(&mut initialized, msg, chain_uid);
        assert_eq!(result.unwrap_err(), ContractError::AssetDoesNotExist {});
    }

    // -----------------------------------------------------------------------
    // Multi-chain token registration
    // -----------------------------------------------------------------------

    /// The same token may be registered on two different chains simultaneously.
    #[rstest]
    fn test_register_denom_same_token_two_chains(mut initialized: MockDeps) {
        let chain1 = ChainUid::create("chain1".to_string()).unwrap();
        let chain2 = ChainUid::create("chain2".to_string()).unwrap();
        let token = Token::create("usdc".to_string()).unwrap();

        for (chain_uid, denom) in [(&chain1, "uusdc"), (&chain2, "usdc.ibc")] {
            call_reusable(
                &mut initialized,
                RouterCrossChainExecuteMsg::RegisterDenom {
                    sender: CrossChainUser::new(chain_uid.clone(), "user".to_string()),
                    token: TokenWithDenom {
                        token: token.clone(),
                        token_type: TokenType::Native {
                            denom: denom.to_string(),
                        },
                    },
                    tx_id: format!("tx_{denom}"),
                },
                chain_uid.clone(),
            )
            .unwrap();
        }

        let entries = TOKEN_DENOMS
            .load(initialized.as_ref().storage, token)
            .unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|e| e.chain_uid == chain1));
        assert!(entries.iter().any(|e| e.chain_uid == chain2));
    }

    /// Deregistering a token on chain1 must not affect chain2's registration.
    #[rstest]
    fn test_deregister_denom_preserves_other_chain_entries(mut initialized: MockDeps) {
        let chain1 = ChainUid::create("chain1".to_string()).unwrap();
        let chain2 = ChainUid::create("chain2".to_string()).unwrap();
        let token = Token::create("usdc".to_string()).unwrap();

        // Pre-seed both chains.
        TOKEN_DENOMS
            .save(
                initialized.as_mut().storage,
                token.clone(),
                &vec![
                    TokenDenom {
                        chain_uid: chain1.clone(),
                        token_type: TokenType::Native {
                            denom: "uusdc".to_string(),
                        },
                    },
                    TokenDenom {
                        chain_uid: chain2.clone(),
                        token_type: TokenType::Native {
                            denom: "usdc.ibc".to_string(),
                        },
                    },
                ],
            )
            .unwrap();

        // Deregister only chain1.
        call_reusable(
            &mut initialized,
            RouterCrossChainExecuteMsg::DeregisterDenom {
                sender: CrossChainUser::new(chain1.clone(), "user".to_string()),
                token: TokenWithDenom {
                    token: token.clone(),
                    token_type: TokenType::Native {
                        denom: "uusdc".to_string(),
                    },
                },
                tx_id: "tx1".to_string(),
            },
            chain1.clone(),
        )
        .unwrap();

        let entries = TOKEN_DENOMS
            .load(initialized.as_ref().storage, token)
            .unwrap();
        assert_eq!(entries.len(), 1, "chain1 entry should be removed");
        assert_eq!(entries[0].chain_uid, chain2, "chain2 entry must survive");
    }

    // -----------------------------------------------------------------------
    // DepositToken: escrow accumulation
    // -----------------------------------------------------------------------

    /// Successive deposits for the same token+chain accumulate in ESCROW_BALANCES.
    #[rstest]
    fn test_ibc_deposit_token_accumulates_balance(mut initialized: MockDeps) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let token = Token::create("usdc".to_string()).unwrap();

        seed_virtual_balance(&mut initialized);
        TOKEN_DENOMS
            .save(initialized.as_mut().storage, token.clone(), &vec![])
            .unwrap();

        let deposit = |deps: &mut MockDeps, amount: u128| {
            call_reusable(
                deps,
                RouterCrossChainExecuteMsg::DepositToken(RouterCrossChainDepositTokenExecuteMsg {
                    sender: CrossChainUser::new(chain_uid.clone(), "user".to_string()),
                    asset_in: TokenWithDenom {
                        token: token.clone(),
                        token_type: TokenType::Native {
                            denom: "uusdc".to_string(),
                        },
                    },
                    amount_in: Uint128::new(amount),
                    recipients: vec![],
                    tx_id: format!("tx_{amount}"),
                }),
                chain_uid.clone(),
            )
            .unwrap()
        };

        deposit(&mut initialized, 100);
        deposit(&mut initialized, 250);

        let balance = ESCROW_BALANCES
            .load(initialized.as_ref().storage, (token.to_string(), chain_uid))
            .unwrap();
        assert_eq!(balance, Uint128::new(350), "deposits should accumulate");
    }

    // -----------------------------------------------------------------------
    // DepositToken dispatch
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_ibc_deposit_token_updates_escrow_and_emits_mint(mut initialized: MockDeps) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let token = Token::create("usdc".to_string()).unwrap();

        seed_virtual_balance(&mut initialized);
        // execute_transfer_voucher unconditionally loads TOKEN_DENOMS for the token
        TOKEN_DENOMS
            .save(initialized.as_mut().storage, token.clone(), &vec![])
            .unwrap();

        let msg =
            RouterCrossChainExecuteMsg::DepositToken(RouterCrossChainDepositTokenExecuteMsg {
                sender: CrossChainUser::new(chain_uid.clone(), "user".to_string()),
                asset_in: TokenWithDenom {
                    token: token.clone(),
                    token_type: TokenType::Native {
                        denom: "uusdc".to_string(),
                    },
                },
                amount_in: Uint128::new(100),
                recipients: vec![],
                tx_id: "tx1".to_string(),
            });
        let res = call_reusable(&mut initialized, msg, chain_uid.clone()).unwrap();

        let balance = ESCROW_BALANCES
            .load(initialized.as_ref().storage, (token.to_string(), chain_uid))
            .unwrap();
        assert_eq!(balance, Uint128::new(100));
        assert!(
            !res.messages.is_empty(),
            "expected virtual balance mint submessage"
        );
    }

    // -----------------------------------------------------------------------
    // TransferVoucher dispatch
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_ibc_transfer_voucher_returns_action_attribute(mut initialized: MockDeps) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let token = Token::create("usdc".to_string()).unwrap();

        seed_virtual_balance(&mut initialized);
        TOKEN_DENOMS
            .save(initialized.as_mut().storage, token.clone(), &vec![])
            .unwrap();

        let vsl_chain = ChainUid::vsl_chain_uid().unwrap();
        let msg = RouterCrossChainExecuteMsg::TransferVoucher(
            RouterCrossChainTransferVoucherExecuteMsg {
                sender: CrossChainUser::new(chain_uid.clone(), "user".to_string()),
                token,
                amount: Uint128::new(100),
                from: None,
                recipients: vec![Recipient {
                    recipient: CrossChainUser::new(vsl_chain, "recipient".to_string()),
                    amount: Limit::LessThanOrEqual(Uint128::new(100)),
                    denom: TokenType::Voucher {},
                    forwarding_message: None,
                    unsafe_refund_as_voucher: None,
                }],
                tx_id: "tx1".to_string(),
            },
        );
        let res = call_reusable(&mut initialized, msg, chain_uid).unwrap();

        assert!(
            res.attributes
                .iter()
                .any(|a| a.key == "action" && a.value == "transfer_virtual_balance"),
            "expected action=transfer_virtual_balance attribute"
        );
    }
}
