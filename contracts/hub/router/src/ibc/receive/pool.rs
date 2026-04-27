use cosmwasm_std::{ensure, to_json_binary, DepsMut, Env, Response, SubMsg, Uint256, WasmMsg};
use euclid::{
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    error::ContractError,
    events::{simple_event, tx_event, TxType},
    fee::Fee,
    msgs::{
        self,
        router::TokenDenom,
        virtual_balance::msg::{ExecuteApprove, ExecuteMint, ExecuteMsg as VirtualBalanceMsg},
        vlp::base::{PoolConfig, VlpAddLiquidityMsg, VlpRegisterPoolMsg, VlpRemoveLiquidityMsg},
    },
    normalize::normalize_token_to_voucher,
    token::{PairWithDenomAndAmount, TokenMetadata, TokenType},
    voucher::BalanceKey,
};
use euclid_ibc::router_ibc::RouterCrossChainRemoveLiquidityExecuteMsg;

use crate::{
    query::{query_token_metadata_by_denom, query_token_registered},
    reply::{
        ADD_LIQUIDITY_REPLY_ID, REMOVE_LIQUIDITY_REPLY_ID, VLP_INSTANTIATE_REPLY_ID,
        VLP_POOL_REGISTER_REPLY_ID,
    },
    state::{
        ADMIN, FEE_STATE, FUNDS_INFO, PENDING_REMOVE_LIQUIDITY, STATE, VIRTUAL_BALANCE_CONTRACT,
        VLPS,
    },
};

pub fn ibc_execute_request_pool_creation(
    deps: DepsMut,
    env: Env,
    sender: CrossChainUser,
    pair_with_denom: PairWithDenomAndAmount,
    pool_config: PoolConfig,
    tx_id: String,
    slippage_tolerance_bps: u64,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    let admins = ADMIN.load(deps.storage)?;
    let virtual_balance_address = VIRTUAL_BALANCE_CONTRACT.load(deps.storage)?;

    let pair = pair_with_denom.get_pair()?;
    pair.validate()?;

    let mut response = Response::new()
        .add_event(tx_event(
            &tx_id,
            &sender.to_sender_string(),
            TxType::PoolCreation,
        ))
        .add_attribute("tx_id", tx_id.clone())
        .add_attribute("method", "request_pool_creation");

    let mut one_token_already_exists = false;

    for token in pair_with_denom.get_vec_token_info() {
        let token_registered = query_token_registered(deps.as_ref(), &token.token)?;

        one_token_already_exists = one_token_already_exists || token_registered;

        // If its a voucher, then we need to check if this token atleast exist on one of the chains
        if token.token_type.is_voucher() {
            ensure!(
                token_registered,
                ContractError::new(
                    "Cannot create pool with voucher token that doesn't exist on any chain"
                )
            );
        } else {
            let token_registered_on_sender_chain = query_token_metadata_by_denom(
                deps.as_ref(),
                &virtual_balance_address,
                &token.token,
                &sender.chain_uid,
                &token.token_type,
            );
            match token_registered_on_sender_chain {
                Ok(token_metadata) => {
                    ensure!(
                        token_registered,
                        ContractError::new(
                            format!(
                                "Token: {}:: Cannot use already existing token without register on sender chain first",
                                token.token
                            )
                            .as_str()
                        )
                    );
                    let token_decimals = token.token_type.get_decimals()?;
                    let metadata_decimals = token_metadata.token_type.get_decimals()?;
                    ensure!(
                        metadata_decimals == token_decimals,
                        ContractError::DecimalsMismatch {
                            expected: metadata_decimals,
                            received: token_decimals,
                        }
                    );
                }
                Err(_) => {
                    // We don't have this token registered on sender chain, so we need to register it
                    let register_metadata_msg = VirtualBalanceMsg::RegisterTokenMetadata {
                        token_metadata: TokenMetadata::new(
                            token.token.clone(),
                            sender.chain_uid.clone(),
                            token.token_type.clone(),
                        ),
                    };
                    let register_metadata_wasm_msg = WasmMsg::Execute {
                        contract_addr: virtual_balance_address.to_string(),
                        msg: to_json_binary(&register_metadata_msg)?,
                        funds: vec![],
                    };
                    response = response.add_message(register_metadata_wasm_msg);

                    response = response.add_event(
                        simple_event()
                            .add_attribute("action", "register_denom")
                            .add_attribute("token", token.token.to_string())
                            .add_attribute("chain_uid", sender.chain_uid.to_string())
                            .add_attribute("token_type", token.token_type.get_key()),
                    );
                }
            };
        }
    }

    // Cannot create pool if both tokens are new
    ensure!(
        one_token_already_exists,
        ContractError::new("Cannot create pool with two new tokens")
    );

    FUNDS_INFO.save(
        deps.storage,
        &(pair_with_denom.clone(), slippage_tolerance_bps),
    )?;

    let register_msg = msgs::vlp::base::ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
        sender: sender.clone(),
        pair: pair.clone(),
        tx_id: tx_id.clone(),
    });

    let vlp = VLPS.may_load(deps.storage, pair.get_tupple())?;

    let virtual_balance_address = VIRTUAL_BALANCE_CONTRACT.load(deps.storage)?;

    // If VLP exists, register pool on it, otherwise create new VLP contract
    if let Some(vlp_addr) = vlp {
        let msg = WasmMsg::Execute {
            contract_addr: vlp_addr.to_string(),
            msg: to_json_binary(&register_msg)?,
            funds: vec![],
        };
        Ok(response.add_submessage(SubMsg::reply_always(msg, VLP_POOL_REGISTER_REPLY_ID)))
    } else {
        let default_fee_recipient = FEE_STATE.load(deps.storage)?.default_fee_recipient;
        let default_fee_recipient = CrossChainUser::new(
            ChainUid::vsl_chain_uid()?,
            default_fee_recipient.to_string(),
        );
        let msg = match pool_config {
            PoolConfig::Stable { amp_factor } => WasmMsg::Instantiate {
                admin: Some(admins.migration_admin.to_string()),
                code_id: state.stable_vlp_code_id,
                msg: to_json_binary(&msgs::vlp::stable::msg::InstantiateMsg {
                    router: env.contract.address,
                    virtual_balance_contract: virtual_balance_address.clone(),
                    pair: pair.clone(),
                    fee: Fee::new(10, 10, default_fee_recipient),
                    execute: Some(msgs::vlp::stable::msg::ExecuteMsg::RegisterPool(
                        VlpRegisterPoolMsg {
                            sender: sender.clone(),
                            pair: pair.clone(),
                            tx_id: tx_id.clone(),
                        },
                    )),
                    admin: admins,
                    amp_factor,
                })?,
                funds: vec![],
                label: "Stable VLP".to_string(),
            },
            PoolConfig::ConstantProduct {} => WasmMsg::Instantiate {
                admin: Some(admins.general_admin.to_string()),
                code_id: state.constant_product_vlp_code_id,
                msg: to_json_binary(&msgs::vlp::cp::msg::InstantiateMsg {
                    router: env.contract.address,
                    virtual_balance_contract: virtual_balance_address.clone(),
                    pair: pair.clone(),
                    fee: Fee::new(10, 10, default_fee_recipient),
                    execute: Some(msgs::vlp::cp::msg::ExecuteMsg::RegisterPool(
                        VlpRegisterPoolMsg {
                            sender: sender.clone(),
                            pair: pair.clone(),
                            tx_id: tx_id.clone(),
                        },
                    )),
                    admin: admins,
                })?,
                funds: vec![],
                label: "Constant Product VLP".to_string(),
            },
        };

        Ok(response.add_submessage(SubMsg::reply_always(msg, VLP_INSTANTIATE_REPLY_ID)))
    }
}

pub fn ibc_execute_add_liquidity(
    deps: DepsMut,
    sender: CrossChainUser,
    pair: PairWithDenomAndAmount,
    slippage_tolerance_bps: u64,
    tx_id: String,
) -> Result<Response, ContractError> {
    let vlp_address = VLPS.load(deps.storage, pair.get_pair()?.get_tupple())?;

    let mut response = Response::new().add_event(
        tx_event(&tx_id, &sender.to_sender_string(), TxType::AddLiquidity)
            .add_attribute("tx_id", tx_id.clone()),
    );

    let virtual_balance_address = VIRTUAL_BALANCE_CONTRACT.load(deps.storage)?;

    // Collect normalized amounts for each token
    let mut normalized_amounts: Vec<Uint256> = Vec::new();
    for token in pair.get_vec_token_info() {
        let normalized_amount = if token.token_type.is_voucher() {
            // Voucher tokens are already normalized
            token.amount
        } else {
            let metadata = query_token_metadata_by_denom(
                deps.as_ref(),
                &virtual_balance_address,
                &token.token,
                &sender.chain_uid,
                &token.token_type,
            )?;
            normalize_token_to_voucher(token.amount, metadata.token_type.get_decimals()?)?
        };
        normalized_amounts.push(normalized_amount);

        // Mint if not voucher token (escrow managed by virtual_balance)
        if !token.token_type.is_voucher() {
            // Mint virtual balance for the token
            let mint_virtual_balance_msg =
                euclid::msgs::virtual_balance::msg::ExecuteMsg::Mint(ExecuteMint {
                    amount: token.amount.into(),
                    balance_key: BalanceKey {
                        cross_chain_user: sender.clone(),
                        token_id: token.token.to_string(),
                    },
                    token_type: token.token_type.clone(),
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

        // Transfer voucher token to the vlp contract
        let approve_voucher_msg =
            euclid::msgs::virtual_balance::msg::ExecuteMsg::Approve(ExecuteApprove {
                amount: normalized_amount,
                token_id: token.token.to_string(),
                spender: CrossChainUser::new(ChainUid::vsl_chain_uid()?, vlp_address.to_string()),
                owner: sender.clone(),
            });

        let approve_voucher_msg = WasmMsg::Execute {
            contract_addr: virtual_balance_address.to_string(),
            msg: to_json_binary(&approve_voucher_msg)?,
            funds: vec![],
        };

        // Should reject full execution if failed
        response = response.add_message(approve_voucher_msg);
    }

    let normalized_liquidity = pair
        .get_pair()?
        .get_pair_with_amount(normalized_amounts[0], normalized_amounts[1])?;

    let add_liquidity_msg = msgs::vlp::base::ExecuteMsg::AddLiquidity(VlpAddLiquidityMsg {
        liquidity: normalized_liquidity,
        sender,
        tx_id,
        slippage_tolerance_bps,
    });

    let msg = WasmMsg::Execute {
        contract_addr: vlp_address.to_string(),
        msg: to_json_binary(&add_liquidity_msg)?,
        funds: vec![],
    };

    Ok(response.add_submessage(SubMsg::reply_always(msg, ADD_LIQUIDITY_REPLY_ID)))
}

pub fn ibc_execute_remove_liquidity(
    deps: DepsMut,
    _env: Env,
    msg: RouterCrossChainRemoveLiquidityExecuteMsg,
) -> Result<Response, ContractError> {
    let vlp_address = VLPS.load(deps.storage, msg.pair.get_tupple())?;
    let response = Response::new()
        .add_event(tx_event(
            &msg.tx_id,
            &msg.sender.to_sender_string(),
            TxType::AddLiquidity,
        ))
        .add_attribute("tx_id", msg.tx_id.clone());

    let req_key = PENDING_REMOVE_LIQUIDITY.key(msg.tx_id.clone());
    ensure!(
        !req_key.has(deps.storage),
        ContractError::new("tx already present")
    );

    req_key.save(deps.storage, &msg)?;

    let remove_liquidity_msg =
        msgs::vlp::base::ExecuteMsg::RemoveLiquidity(VlpRemoveLiquidityMsg {
            sender: msg.sender,
            lp_allocation: msg.lp_allocation,
            tx_id: msg.tx_id,
        });

    let msg = WasmMsg::Execute {
        contract_addr: vlp_address.to_string(),
        msg: to_json_binary(&remove_liquidity_msg)?,
        funds: vec![],
    };
    Ok(response.add_submessage(SubMsg::reply_always(msg, REMOVE_LIQUIDITY_REPLY_ID)))
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{Addr, Uint128, Uint256};
    use euclid::{
        chain::ChainUid,
        cross_chain_user::CrossChainUser,
        error::ContractError,
        msgs::{router::TokenDenom, vlp::base::PoolConfig},
        token::{Pair, Token, TokenType},
    };
    use euclid_ibc::router_ibc::{
        RouterCrossChainExecuteMsg, RouterCrossChainRemoveLiquidityExecuteMsg,
    };

    use crate::{
        reply::{REMOVE_LIQUIDITY_REPLY_ID, VLP_INSTANTIATE_REPLY_ID, VLP_POOL_REGISTER_REPLY_ID},
        state::{PENDING_REMOVE_LIQUIDITY, VLPS},
        testing::{
            fixtures::initialized,
            helpers::{
                call_reusable, make_pool_pair, seed_virtual_balance, seed_vlp_aaa_bbb, MockDeps,
            },
        },
    };

    use rstest::*;

    #[rstest]
    fn test_ibc_add_liquidity_missing_vlp_fails(mut initialized: MockDeps) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        seed_virtual_balance(&mut initialized);
        // No VLPS entry

        let msg = RouterCrossChainExecuteMsg::AddLiquidity {
            sender: CrossChainUser::new(chain_uid.clone(), "user".to_string()),
            slippage_tolerance_bps: 100,
            pair: make_pool_pair(50, 50),
            tx_id: "tx1".to_string(),
        };
        assert!(call_reusable(&mut initialized, msg, chain_uid).is_err());
    }

    // -----------------------------------------------------------------------
    // RemoveLiquidity dispatch
    // -----------------------------------------------------------------------

    fn make_remove_liquidity_msg(chain_uid: &ChainUid, tx_id: &str) -> RouterCrossChainExecuteMsg {
        let pair = Pair::new(
            Token::create("aaa".to_string()).unwrap(),
            Token::create("bbb".to_string()).unwrap(),
        )
        .unwrap();
        RouterCrossChainExecuteMsg::RemoveLiquidity(RouterCrossChainRemoveLiquidityExecuteMsg {
            sender: CrossChainUser::new(chain_uid.clone(), "user".to_string()),
            lp_allocation: Uint256::from(100u128),
            pair,
            recipient: CrossChainUser::new(chain_uid.clone(), "recipient".to_string()),
            tx_id: tx_id.to_string(),
        })
    }

    #[rstest]
    fn test_ibc_remove_liquidity_saves_pending_and_emits_submsg(mut initialized: MockDeps) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        seed_vlp_aaa_bbb(&mut initialized);

        let res = call_reusable(
            &mut initialized,
            make_remove_liquidity_msg(&chain_uid, "tx_rem"),
            chain_uid,
        )
        .unwrap();

        assert_eq!(
            res.messages.last().unwrap().id,
            REMOVE_LIQUIDITY_REPLY_ID,
            "expected remove-liquidity submsg"
        );
        assert!(
            PENDING_REMOVE_LIQUIDITY.has(initialized.as_ref().storage, "tx_rem".to_string()),
            "expected PENDING_REMOVE_LIQUIDITY entry"
        );
    }

    #[rstest]
    fn test_ibc_remove_liquidity_duplicate_tx_fails(mut initialized: MockDeps) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        seed_vlp_aaa_bbb(&mut initialized);

        let pair = Pair::new(
            Token::create("aaa".to_string()).unwrap(),
            Token::create("bbb".to_string()).unwrap(),
        )
        .unwrap();
        let pending = RouterCrossChainRemoveLiquidityExecuteMsg {
            sender: CrossChainUser::new(chain_uid.clone(), "user".to_string()),
            lp_allocation: Uint256::from(100u128),
            pair,
            recipient: CrossChainUser::new(chain_uid.clone(), "recipient".to_string()),
            tx_id: "tx_dup".to_string(),
        };
        PENDING_REMOVE_LIQUIDITY
            .save(initialized.as_mut().storage, "tx_dup".to_string(), &pending)
            .unwrap();

        let result = call_reusable(
            &mut initialized,
            make_remove_liquidity_msg(&chain_uid, "tx_dup"),
            chain_uid,
        );
        assert_eq!(
            result.unwrap_err(),
            ContractError::new("tx already present")
        );
    }

    // -----------------------------------------------------------------------
    // RequestPoolCreation: new denom auto-registered in TOKEN_DENOMS
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_ibc_request_pool_creation_both_tokens_new_fails(mut initialized: MockDeps) {
        use cosmwasm_std::{
            from_json, to_json_binary, ContractResult, SystemError, SystemResult, WasmQuery,
        };
        use euclid::msgs::virtual_balance::msg::{
            GetTokenRegisteredResponse, QueryMsg as VirtualBalanceQueryMsg,
        };

        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        seed_virtual_balance(&mut initialized);
        // Neither token in TOKEN_DENOMS

        initialized.querier.update_wasm(|q| match q {
            WasmQuery::Smart { msg, .. } => {
                let parsed: VirtualBalanceQueryMsg = from_json(msg).unwrap();
                match parsed {
                    VirtualBalanceQueryMsg::GetTokenRegistered { .. } => {
                        let resp = GetTokenRegisteredResponse {
                            token_registered: false,
                        };
                        SystemResult::Ok(ContractResult::Ok(to_json_binary(&resp).unwrap()))
                    }
                    VirtualBalanceQueryMsg::GetTokenMetadataByDenom { .. } => {
                        SystemResult::Err(SystemError::InvalidRequest {
                            error: "metadata not found".to_string(),
                            request: Default::default(),
                        })
                    }
                    other => panic!("unexpected virtual_balance query: {other:?}"),
                }
            }
            _ => panic!("unexpected wasm query"),
        });

        let msg = RouterCrossChainExecuteMsg::RequestPoolCreation {
            sender: CrossChainUser::new(chain_uid.clone(), "user".to_string()),
            tx_id: "tx1".to_string(),
            pair: make_pool_pair(100, 100),
            pool_config: PoolConfig::ConstantProduct {},
            slippage_tolerance_bps: 100,
        };
        let result = call_reusable(&mut initialized, msg, chain_uid);
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Cannot create pool with two new tokens"),
            "expected two-new-tokens error"
        );
    }
}
