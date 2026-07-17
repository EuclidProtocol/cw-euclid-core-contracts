use cosmwasm_std::to_json_binary;
use cosmwasm_std::{from_json, Binary, CosmosMsg, DepsMut, Env, Response, WasmMsg};
use euclid::chain::{Chain, ChainType, ChainUid};
use euclid::cross_chain_user::CrossChainUser;
use euclid::error::ContractError;
use euclid::events::{tx_event, TxType};
use euclid::msgs::factory::{RegisterFactoryResponse, ReleaseEscrowResponse};
use euclid::msgs::virtual_balance::msg::{ExecuteMint, ExecuteMsg as VirtualBalanceExecuteMsg};
use euclid::token::Token;
use euclid::voucher::BalanceKey;
use euclid_ibc::wire::envelope::factory::FactoryReceiveMsg;
use euclid_ibc::wire::envelope::AcknowledgementMsg;

use euclid::token::TokenType;

use crate::state::{
    CHAIN_UID_TO_CHAIN, FEE_STATE, PENDING_RELEASE_VOUCHER, VIRTUAL_BALANCE_CONTRACT,
};

pub fn reusable_internal_ack_call(
    deps: DepsMut,
    env: Env,
    chain_uid: ChainUid,
    msg: FactoryReceiveMsg,
    ack: Binary,
    chain_type: euclid::chain::ChainType,
) -> Result<Response, ContractError> {
    let tx_id = msg.get_tx_id();

    let response = match msg {
        FactoryReceiveMsg::RegisterFactory(msg) => {
            let res = from_json(ack)?;
            ibc_ack_register_factory(deps, env, msg.chain_uid, chain_type, res, msg.tx_id)?
        }
        FactoryReceiveMsg::ReleaseEscrow(msg) => {
            let res = from_json(ack)?;
            let recipient = CrossChainUser::new(chain_uid, msg.recipient.to_string());
            // Reject mixed-case or empty addresses from IBC packet data
            msg.sender.validate()?;
            recipient.validate()?;
            ibc_ack_release_escrow(
                deps, env, msg.sender, msg.token, msg.denom, res, recipient, msg.tx_id,
            )?
        }
    };
    let response = response.add_attribute("tx_id", tx_id);
    Ok(response)
}

pub fn ibc_ack_register_factory(
    deps: DepsMut,
    env: Env,
    chain_uid: ChainUid,
    chain_type: ChainType,
    res: AcknowledgementMsg<RegisterFactoryResponse>,
    tx_id: String,
) -> Result<Response, ContractError> {
    let response = Response::new().add_event(tx_event(
        &tx_id,
        env.contract.address.as_str(),
        TxType::RegisterFactory,
    ));
    match res {
        AcknowledgementMsg::Ok(data) => {
            // The factory is the authority on duplicate registration: it tracks
            // whether it has already been registered and returns an error ack if so
            // (handled in the Error arm below). A success ack therefore means this
            // is the first registration for the chain, so save it.
            let chain_data = Chain {
                chain_uid: chain_uid.clone(),
                factory_address: data.factory_address.clone(),
                chain_type: chain_type.clone(),
            };
            CHAIN_UID_TO_CHAIN.save(deps.storage, chain_uid.clone(), &chain_data)?;
            Ok(response
                .add_attribute("method", "register_factory_ack_success")
                .add_attribute("chain_uid", chain_uid.to_string())
                .add_attribute("factory_chain", data.chain_id)
                .add_attribute("factory_address", data.factory_address))
        }

        AcknowledgementMsg::Error(err) => {
            // If its a native then reject via error
            if matches!(chain_type, ChainType::Native {}) {
                return Err(ContractError::new(&err));
            }
            Ok(response
                .add_attribute("method", "register_factory_ack_error")
                .add_attribute("chain_uid", chain_uid.to_string())
                .add_attribute("error", err.clone()))
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn ibc_ack_release_escrow(
    deps: DepsMut,
    _env: Env,
    sender: CrossChainUser,
    token: Token,
    token_type: TokenType,
    res: AcknowledgementMsg<ReleaseEscrowResponse>,
    recipient: CrossChainUser,
    tx_id: String,
) -> Result<Response, ContractError> {
    let response = Response::new().add_event(tx_event(
        &tx_id,
        sender.address.as_str(),
        TxType::EscrowRelease,
    ));
    let pending_release_voucher = PENDING_RELEASE_VOUCHER.load(deps.storage, tx_id.clone())?;
    PENDING_RELEASE_VOUCHER.remove(deps.storage, tx_id);
    let virtual_balance_address = VIRTUAL_BALANCE_CONTRACT.load(deps.storage)?;
    match res {
        AcknowledgementMsg::Ok(data) => {
            let mut response = response
                .add_attribute("method", "release_escrow_success")
                .add_attribute("amount", data.amount.to_string())
                .add_attribute("recipient", data.to_address)
                .add_attribute("updated_escrow_balance", data.escrow_balance.to_string())
                .add_attribute("chain_uid", sender.chain_uid.to_string());

            if !pending_release_voucher.release_fee_amount.is_zero() {
                let fee_recipient = FEE_STATE.load(deps.storage)?.release_fee_recipient;
                let balance_key = BalanceKey {
                    cross_chain_user: CrossChainUser::new(
                        ChainUid::vsl_chain_uid()?,
                        fee_recipient.to_string(),
                    ),
                    token_id: token.to_string(),
                };

                // Mint release fee to fee recipient
                let mint_msg = VirtualBalanceExecuteMsg::Mint(ExecuteMint {
                    amount: pending_release_voucher.release_fee_amount,
                    balance_key: balance_key.clone(),
                    token_type: token_type.clone(),
                    token_source_chain_uid: recipient.chain_uid.clone(),
                });
                let msg: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: virtual_balance_address.to_string(),
                    msg: to_json_binary(&mint_msg)?,
                    funds: vec![],
                });
                response = response.add_message(msg)
            };

            Ok(response)
        }
        // Re-mint tokens
        AcknowledgementMsg::Error(err) => {
            // Escrow release failed: re-mint vouchers (virtual_balance will re-increment escrow)
            let refund_recipient = if pending_release_voucher.unsafe_refund_voucher {
                recipient.clone()
            } else {
                sender.clone()
            };

            let balance_key = BalanceKey {
                cross_chain_user: refund_recipient.clone(),
                token_id: token.to_string(),
            };
            let mint_amount = pending_release_voucher.total_amount;
            // Escrow release failed, mint tokens again for the original cross chain sender
            let mint_msg = VirtualBalanceExecuteMsg::Mint(ExecuteMint {
                amount: mint_amount,
                balance_key: balance_key.clone(),
                token_type,
                token_source_chain_uid: recipient.chain_uid.clone(),
            });
            let msg: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: virtual_balance_address.to_string(),
                msg: to_json_binary(&mint_msg)?,
                funds: vec![],
            });

            // Even if its a native chain, we can't reject via Err because other escrow release will also be rejected
            Ok(response
                .add_message(msg)
                .add_attribute("method", "escrow_release_ack")
                .add_attribute("error", err)
                .add_attribute("mint_amount", mint_amount.to_string())
                .add_attribute("balance_key", format!("{:?}", balance_key)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env};
    use cosmwasm_std::{Addr, CosmosMsg, Uint256, WasmMsg};
    use euclid::chain::ChainUid;
    use euclid::cross_chain_user::CrossChainUser;
    use euclid::msgs::factory::ReleaseEscrowResponse;
    use euclid::token::{Token, TokenType};
    use euclid_ibc::wire::envelope::AcknowledgementMsg;

    use crate::state::{
        FeeState, PendingReleaseVoucher, FEE_STATE, PENDING_RELEASE_VOUCHER,
        VIRTUAL_BALANCE_CONTRACT,
    };

    fn setup_release_ack_deps(
        total_amount: Uint256,
        release_fee_amount: Uint256,
        unsafe_refund_voucher: bool,
    ) -> cosmwasm_std::OwnedDeps<
        cosmwasm_std::MemoryStorage,
        cosmwasm_std::testing::MockApi,
        cosmwasm_std::testing::MockQuerier,
    > {
        let mut deps = mock_dependencies();
        let vb_addr = Addr::unchecked("virtual_balance_contract");

        VIRTUAL_BALANCE_CONTRACT
            .save(deps.as_mut().storage, &vb_addr)
            .unwrap();
        FEE_STATE
            .save(
                deps.as_mut().storage,
                &FeeState {
                    release_fee_recipient: Addr::unchecked("fee_recipient"),
                    default_fee_recipient: Addr::unchecked("default_fee"),
                },
            )
            .unwrap();
        PENDING_RELEASE_VOUCHER
            .save(
                deps.as_mut().storage,
                "tx_001".to_string(),
                &PendingReleaseVoucher {
                    total_amount,
                    release_fee_amount,
                    unsafe_refund_voucher,
                },
            )
            .unwrap();

        deps
    }

    /// Disproves review Bug #1: "unit-space mismatch in release-failure refund".
    ///
    /// The review claimed mixed units in refund calculation. In reality,
    /// `mint_amount = pending_release_voucher.total_amount` which is stored in
    /// raw token units (set during `_release_voucher`). This is passed to
    /// virtual_balance's execute_mint which normalizes raw → voucher internally.
    /// No unit mismatch exists.
    #[test]
    fn test_release_error_ack_refund_uses_raw_token_units() {
        let total_amount = Uint256::from(200u128); // raw token units
        let release_fee = Uint256::from(10u128); // raw token units

        let mut deps = setup_release_ack_deps(total_amount, release_fee, false);

        let sender = CrossChainUser::new(
            ChainUid::create("chain1".to_string()).unwrap(),
            "sender_addr".to_string(),
        );
        let recipient = CrossChainUser::new(
            ChainUid::create("chain1".to_string()).unwrap(),
            "recipient_addr".to_string(),
        );

        let res = ibc_ack_release_escrow(
            deps.as_mut(),
            mock_env(),
            sender.clone(),
            Token::create("usdc".to_string()).unwrap(),
            TokenType::Native {
                denom: "uusdc".to_string(),
                decimals: Some(6),
            },
            AcknowledgementMsg::<ReleaseEscrowResponse>::Error("factory error".to_string()),
            recipient.clone(),
            "tx_001".to_string(),
        )
        .unwrap();

        // Extract the mint message sent to virtual_balance
        assert_eq!(res.messages.len(), 1);
        let mint_msg = match &res.messages[0].msg {
            CosmosMsg::Wasm(WasmMsg::Execute { msg, .. }) => {
                let parsed: euclid::msgs::virtual_balance::msg::ExecuteMsg =
                    cosmwasm_std::from_json(msg).unwrap();
                match parsed {
                    euclid::msgs::virtual_balance::msg::ExecuteMsg::Mint(m) => m,
                    _ => panic!("expected Mint message"),
                }
            }
            _ => panic!("expected WasmMsg::Execute"),
        };

        // mint_amount = total_amount from PendingReleaseVoucher = 200 (raw token units)
        assert_eq!(mint_msg.amount, total_amount);
        assert_eq!(mint_msg.amount, Uint256::from(200u128));

        // token_type carries decimals info for virtual_balance to normalize
        assert_eq!(
            mint_msg.token_type,
            TokenType::Native {
                denom: "uusdc".to_string(),
                decimals: Some(6),
            }
        );
    }

    /// Disproves review Bug #2: "double-normalization on ack-error re-mint path".
    ///
    /// The review claimed execute_mint normalizes an already-normalized amount.
    /// This test proves PendingReleaseVoucher.total_amount is stored in raw token
    /// units (not voucher units), so execute_mint's single normalization is correct.
    #[test]
    fn test_pending_release_stores_raw_amounts_not_voucher_units() {
        let total_amount = Uint256::from(500u128); // raw 6-decimal token units
        let release_fee = Uint256::from(0u128);

        let mut deps = setup_release_ack_deps(total_amount, release_fee, false);

        let sender = CrossChainUser::new(
            ChainUid::create("chain1".to_string()).unwrap(),
            "sender_addr".to_string(),
        );
        let recipient = CrossChainUser::new(
            ChainUid::create("chain1".to_string()).unwrap(),
            "recipient_addr".to_string(),
        );

        let res = ibc_ack_release_escrow(
            deps.as_mut(),
            mock_env(),
            sender,
            Token::create("usdc".to_string()).unwrap(),
            TokenType::Native {
                denom: "uusdc".to_string(),
                decimals: Some(6),
            },
            AcknowledgementMsg::<ReleaseEscrowResponse>::Error("timeout".to_string()),
            recipient,
            "tx_001".to_string(),
        )
        .unwrap();

        let mint_msg = match &res.messages[0].msg {
            CosmosMsg::Wasm(WasmMsg::Execute { msg, .. }) => {
                let parsed: euclid::msgs::virtual_balance::msg::ExecuteMsg =
                    cosmwasm_std::from_json(msg).unwrap();
                match parsed {
                    euclid::msgs::virtual_balance::msg::ExecuteMsg::Mint(m) => m,
                    _ => panic!("expected Mint message"),
                }
            }
            _ => panic!("expected WasmMsg::Execute"),
        };

        // Amount passed to mint is 500 raw token units (NOT 500 * 10^18 voucher units).
        // If this were already in voucher units, the value would be 500_000_000_000_000_000_000.
        assert_eq!(mint_msg.amount, Uint256::from(500u128));

        // execute_mint will normalize: 500 raw * 10^18 = 500_000_000_000_000_000_000 voucher units.
        // Only ONE normalization happens (inside execute_mint), not two.
    }

    /// On success ack, release fee is minted to fee recipient in raw token units.
    #[test]
    fn test_release_success_ack_mints_fee_in_raw_units() {
        let total_amount = Uint256::from(200u128);
        let release_fee = Uint256::from(15u128);

        let mut deps = setup_release_ack_deps(total_amount, release_fee, false);

        let sender = CrossChainUser::new(
            ChainUid::create("chain1".to_string()).unwrap(),
            "sender_addr".to_string(),
        );
        let recipient = CrossChainUser::new(
            ChainUid::create("chain1".to_string()).unwrap(),
            "recipient_addr".to_string(),
        );

        let res = ibc_ack_release_escrow(
            deps.as_mut(),
            mock_env(),
            sender,
            Token::create("usdc".to_string()).unwrap(),
            TokenType::Native {
                denom: "uusdc".to_string(),
                decimals: Some(6),
            },
            AcknowledgementMsg::Ok(ReleaseEscrowResponse {
                amount: Uint256::from(185u128),
                to_address: "recipient_addr".to_string(),
                escrow_balance: Uint256::from(300u128),
            }),
            recipient,
            "tx_001".to_string(),
        )
        .unwrap();

        // On success, fee is minted to fee_recipient
        assert_eq!(res.messages.len(), 1);
        let mint_msg = match &res.messages[0].msg {
            CosmosMsg::Wasm(WasmMsg::Execute { msg, .. }) => {
                let parsed: euclid::msgs::virtual_balance::msg::ExecuteMsg =
                    cosmwasm_std::from_json(msg).unwrap();
                match parsed {
                    euclid::msgs::virtual_balance::msg::ExecuteMsg::Mint(m) => m,
                    _ => panic!("expected Mint message"),
                }
            }
            _ => panic!("expected WasmMsg::Execute"),
        };

        // Fee minted in raw token units (15), not voucher units
        assert_eq!(mint_msg.amount, Uint256::from(15u128));
        assert_eq!(mint_msg.balance_key.token_id, "usdc");
    }

    #[test]
    fn test_unsafe_refund_voucher_mints_to_recipient() {
        let total_amount = Uint256::from(100u128);
        let release_fee = Uint256::from(5u128);

        let mut deps = setup_release_ack_deps(total_amount, release_fee, true);

        let sender = CrossChainUser::new(
            ChainUid::create("chain1".to_string()).unwrap(),
            "sender_addr".to_string(),
        );
        let recipient = CrossChainUser::new(
            ChainUid::create("chain2".to_string()).unwrap(),
            "recipient_addr".to_string(),
        );

        let res = ibc_ack_release_escrow(
            deps.as_mut(),
            mock_env(),
            sender,
            Token::create("usdc".to_string()).unwrap(),
            TokenType::Native {
                denom: "uusdc".to_string(),
                decimals: Some(6),
            },
            AcknowledgementMsg::<ReleaseEscrowResponse>::Error("escrow failed".to_string()),
            recipient.clone(),
            "tx_001".to_string(),
        )
        .unwrap();

        assert_eq!(res.messages.len(), 1);
        let mint_msg = match &res.messages[0].msg {
            CosmosMsg::Wasm(WasmMsg::Execute { msg, .. }) => {
                let parsed: euclid::msgs::virtual_balance::msg::ExecuteMsg =
                    cosmwasm_std::from_json(msg).unwrap();
                match parsed {
                    euclid::msgs::virtual_balance::msg::ExecuteMsg::Mint(m) => m,
                    _ => panic!("expected Mint message"),
                }
            }
            _ => panic!("expected WasmMsg::Execute"),
        };

        assert_eq!(mint_msg.balance_key.cross_chain_user, recipient);
        assert_eq!(mint_msg.amount, total_amount);
    }

    // Synthesizes the Q1(b) ack-direction failure mode: the inbound ack
    // references a `tx_id` that does not match any `PENDING_RELEASE_VOUCHER`
    // entry (the state a reorg-replayed source would produce if its replayed
    // `tx_id` diverged from the one the destination already ack'd). The
    // handler must fail predictably rather than silently mutating state.
    #[test]
    fn test_release_ack_with_unknown_tx_id_fails_cleanly() {
        let mut deps = setup_release_ack_deps(Uint256::from(100u128), Uint256::from(5u128), false);

        let sender = CrossChainUser::new(
            ChainUid::create("chain1".to_string()).unwrap(),
            "sender_addr".to_string(),
        );
        let recipient = CrossChainUser::new(
            ChainUid::create("chain1".to_string()).unwrap(),
            "recipient_addr".to_string(),
        );

        // setup_release_ack_deps seeds PENDING_RELEASE_VOUCHER under "tx_001";
        // we deliver an ack for a different tx_id, simulating a key mismatch.
        let err = ibc_ack_release_escrow(
            deps.as_mut(),
            mock_env(),
            sender,
            Token::create("usdc".to_string()).unwrap(),
            TokenType::Native {
                denom: "uusdc".to_string(),
                decimals: Some(6),
            },
            AcknowledgementMsg::Ok(ReleaseEscrowResponse {
                amount: Uint256::from(95u128),
                to_address: "recipient_addr".to_string(),
                escrow_balance: Uint256::from(300u128),
            }),
            recipient,
            "tx_unknown".to_string(),
        )
        .unwrap_err();

        // Defined failure: .load() on a missing key surfaces as a contract
        // error. No panic, no silent partial state change.
        assert!(
            format!("{err}").to_lowercase().contains("not found")
                || matches!(err, ContractError::Std(_)),
            "unexpected error variant: {err:?}"
        );

        // Seeded entry under the original tx_id is untouched.
        assert!(PENDING_RELEASE_VOUCHER
            .may_load(deps.as_ref().storage, "tx_001".to_string())
            .unwrap()
            .is_some());
    }

    /// The success ack settles the registration by saving the chain the factory
    /// reported into `CHAIN_UID_TO_CHAIN`. Duplicate protection lives in the
    /// factory (it returns an error ack for an already-registered chain), so the
    /// success arm only ever runs for a first registration and simply saves.
    #[test]
    fn register_factory_ack_success_saves_chain() {
        use euclid::chain::{ChainType, CosmosChain};
        use euclid::msgs::factory::RegisterFactoryResponse;

        let mut deps = mock_dependencies();
        let chain_uid = ChainUid::create("osmosis".to_string()).unwrap();

        ibc_ack_register_factory(
            deps.as_mut(),
            mock_env(),
            chain_uid.clone(),
            ChainType::Cosmos(CosmosChain {
                chain_id: "osmosis-1".to_string(),
            }),
            AcknowledgementMsg::Ok(RegisterFactoryResponse {
                factory_address: "factory_one".to_string(),
                chain_id: "osmosis-1".to_string(),
            }),
            "tx_first".to_string(),
        )
        .unwrap();

        let stored = CHAIN_UID_TO_CHAIN
            .load(deps.as_ref().storage, chain_uid)
            .unwrap();
        assert_eq!(stored.factory_address, "factory_one");
    }

    /// A non-native error ack (e.g. the factory rejecting an already-registered
    /// duplicate) is logged and does not write the settled registry.
    #[test]
    fn register_factory_ack_error_does_not_save_chain() {
        use euclid::chain::{ChainType, CosmosChain};
        use euclid::msgs::factory::RegisterFactoryResponse;

        let mut deps = mock_dependencies();
        let chain_uid = ChainUid::create("osmosis".to_string()).unwrap();

        // Non-native error acks return Ok (only native rejects via Err).
        ibc_ack_register_factory(
            deps.as_mut(),
            mock_env(),
            chain_uid.clone(),
            ChainType::Cosmos(CosmosChain {
                chain_id: "osmosis-1".to_string(),
            }),
            AcknowledgementMsg::<RegisterFactoryResponse>::Error(
                "Factory already registered".to_string(),
            ),
            "tx_err".to_string(),
        )
        .unwrap();

        assert!(
            !CHAIN_UID_TO_CHAIN.has(deps.as_ref().storage, chain_uid),
            "error ack must not write the settled registry"
        );
    }
}
