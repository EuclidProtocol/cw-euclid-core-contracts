use cosmwasm_std::{ensure, DepsMut, Env, MessageInfo, Response, SubMsg, Uint256};
use cw20::Logo;
use euclid::{
    cross_chain_user::CrossChainUser,
    error::ContractError,
    events::tx_event,
    fee::BPS_100_PERCENT,
    liquidity::{AddLiquidityRequest, RemoveLiquidityRequest},
    msgs::{
        cross_chain_config::CrossChainConfig, escrow::AllowedTokenResponse, vlp::base::PoolConfig,
    },
    token::{Pair, PairWithDenomAndAmount, TokenType},
    utils::{fund_manager::FundManager, tx::generate_tx},
};
use euclid_ibc::router_ibc::{
    RouterCrossChainExecuteMsg, RouterCrossChainRemoveLiquidityExecuteMsg,
};

use crate::{
    query::get_chain_type,
    state::{
        PoolCreateRequest, PAIR_TO_VLP, PENDING_ADD_LIQUIDITY, PENDING_POOL_REQUESTS,
        PENDING_REMOVE_LIQUIDITY, STATE, TOKEN_TO_ESCROW, VLP_TO_LP_TOKEN,
    },
};

// Function to send IBC request to Router in VSL to create a new pool
pub fn execute_request_pool_creation(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    pair_with_denom_and_amount: PairWithDenomAndAmount,
    pool_config: PoolConfig,
    lp_token_name: String,
    lp_token_symbol: String,
    lp_token_decimal: u8,
    lp_token_marketing: Option<cw20_base::msg::InstantiateMarketingInfo>,
    slippage_tolerance_bps: u64,
    cross_chain_config: CrossChainConfig,
) -> Result<Response, ContractError> {
    ensure!(
        slippage_tolerance_bps.le(&BPS_100_PERCENT),
        ContractError::InvalidSlippageTolerance {}
    );

    let pair = pair_with_denom_and_amount.get_pair()?;

    pair.validate()?;

    let state = STATE.load(deps.storage)?;
    let sender = CrossChainUser::new(state.chain_uid.clone(), info.sender.to_string());
    let tx_id = generate_tx(deps, &env, &sender)?;

    let mut res = Response::new();

    // Changes factory state without sending liquidity request to router. That will be handled in Pool creation request's reply in router
    // Add liquidity Section //
    // Prepare msg vector
    let mut msgs: Vec<SubMsg> = Vec::new();

    let mut fund_manager = FundManager::new(&info.funds);
    let mut one_token_already_exists = false;
    // Do an early check for tokens escrow so that if it exists, it should allow the denom that we are sending
    let tokens = pair_with_denom_and_amount.get_vec_token_info();

    for token in tokens {
        // Validate token id
        token.token.validate()?;

        // Vouchers are not escrowed
        if !token.token_type.is_voucher() {
            match token.token_type.clone() {
                TokenType::Native { denom, .. } => {
                    // Use funds, if its not present this will throw error.
                    // This will make sure enough funds are provided with the message
                    fund_manager.use_fund(token.amount, &denom)?;
                }
                TokenType::Smart { .. } => {
                    let msg = token.token_type.create_transfer_msg(
                        token.amount,
                        env.contract.address.clone().to_string(),
                        Some(sender.address.clone()),
                        None,
                    )?;
                    msgs.push(SubMsg::new(msg));
                }
                TokenType::Voucher { .. } => return Err(ContractError::UnreachableCode {}),
            }
            // Ensure valid denom if token already exists
            let escrow_address = TOKEN_TO_ESCROW.may_load(deps.storage, token.clone().token)?;
            if let Some(escrow_address) = escrow_address {
                let token_allowed_query_msg = euclid::msgs::escrow::QueryMsg::TokenAllowed {
                    denom: token.clone().token_type,
                };
                let token_allowed: AllowedTokenResponse = deps
                    .querier
                    .query_wasm_smart(escrow_address.clone(), &token_allowed_query_msg)?;

                ensure!(
                    token_allowed.allowed,
                    ContractError::UnsupportedDenomination {}
                );
                one_token_already_exists = true;
            } else {
                // This will validate token for its supply and address only
                token.token_type.validate(&deps.as_ref())?;
                let provided_decimals = token.token_type.get_decimals()?;
                // There is no escrow for this token so it must be new. Lets validate this token a bit more
                match token.token_type {
                    TokenType::Smart { .. } => {
                        let decimals = token.token_type.query_decimals(&deps.as_ref())?;
                        ensure!(
                            decimals == provided_decimals,
                            ContractError::DecimalsMismatch {
                                expected: decimals as u32,
                                received: provided_decimals as u32,
                            }
                        );
                    }
                    TokenType::Native { .. } => {
                        // We don't have a stable check yet for native tokens decimals as their metadata might not be stored on chain
                    }
                    TokenType::Voucher { .. } => {}
                }
            }
        } else {
            // If its a voucher token, then we can assume that one token already exists
            one_token_already_exists = true;
        }
    }

    ensure!(
        one_token_already_exists,
        ContractError::new(
            "Cannot create pool two new tokens. Atleast one token must already be registered."
        )
    );

    res = res.add_submessages(msgs);

    let pair = pair_with_denom_and_amount.get_pair()?;
    // Ensure tokens in pair are different
    ensure!(
        pair.token_1 != pair.token_2,
        ContractError::new("Cannot create pool with same token")
    );

    ensure!(
        !PENDING_POOL_REQUESTS.has(deps.storage, (info.sender.clone(), tx_id.clone())),
        ContractError::TxAlreadyExist {}
    );
    ensure!(
        !PAIR_TO_VLP.has(deps.storage, pair.get_tupple()),
        ContractError::PoolAlreadyExists {}
    );

    // We might get errors in ack if marketing is not valid
    if let Some(marketing) = &lp_token_marketing {
        if let Some(logo) = &marketing.logo {
            ensure!(
                matches!(logo, Logo::Url(_)),
                ContractError::new("Only URL logos are supported")
            );
        }

        if let Some(marketing_address) = &marketing.marketing {
            deps.api.addr_validate(marketing_address)?;
        }
    }

    let lp_token_instantiate_msg = cw20_base::msg::InstantiateMsg {
        name: lp_token_name,
        symbol: lp_token_symbol,
        decimals: lp_token_decimal,
        initial_balances: vec![],
        mint: Some(cw20::MinterResponse {
            minter: env.contract.address.clone().into_string(),
            cap: None,
        }),
        marketing: lp_token_marketing,
    };
    lp_token_instantiate_msg.validate()?;

    let req = PoolCreateRequest {
        tx_id: tx_id.clone(),
        sender: info.sender.clone(),
        pair_info: pair_with_denom_and_amount.clone(),
        lp_token_instantiate_msg,
    };

    PENDING_POOL_REQUESTS.save(deps.storage, (info.sender.clone(), tx_id.clone()), &req)?;

    let chain_type = get_chain_type(deps.as_ref(), &env)?;

    let pool_create_msg = RouterCrossChainExecuteMsg::RequestPoolCreation {
        sender,
        tx_id: tx_id.clone(),
        pair: pair_with_denom_and_amount,
        pool_config,
        slippage_tolerance_bps,
    }
    .to_msg(
        deps,
        &env,
        state.router_contract,
        info.sender.clone(),
        state.chain_uid,
        chain_type,
        cross_chain_config.timeout,
        cross_chain_config.ack_response,
    )?;

    Ok(res
        .add_event(tx_event(
            &tx_id,
            info.sender.as_str(),
            euclid::events::TxType::PoolCreation,
        ))
        .add_attribute("action", "pool_creation")
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "request_pool_creation")
        .add_attribute("token_1", pair.token_1.to_string())
        .add_attribute("token_2", pair.token_2.to_string())
        .add_submessage(pool_create_msg))
}

// Add liquidity to the pool
pub fn add_liquidity_request(
    deps: &mut DepsMut,
    info: MessageInfo,
    env: Env,
    pair_info: PairWithDenomAndAmount,
    slippage_tolerance_bps: u64,
    cross_chain_config: CrossChainConfig,
) -> Result<Response, ContractError> {
    let pair = pair_info.get_pair()?;

    // Check that slippage tolerance is between 1 and 100
    let state = STATE.load(deps.storage)?;
    let sender = CrossChainUser::new(state.chain_uid.clone(), info.sender.to_string());
    let tx_id = generate_tx(deps, &env, &sender)?;

    ensure!(
        (1..=BPS_100_PERCENT).contains(&slippage_tolerance_bps),
        ContractError::InvalidSlippageTolerance {}
    );

    ensure!(
        !PENDING_ADD_LIQUIDITY.has(deps.storage, (info.sender.clone(), tx_id.clone())),
        ContractError::TxAlreadyExist {}
    );
    ensure!(
        PAIR_TO_VLP.has(deps.storage, pair.get_tupple()),
        ContractError::PoolDoesNotExist {}
    );

    // Prepare msg vector
    let mut msgs: Vec<SubMsg> = Vec::new();

    let mut fund_manager = FundManager::new(&info.funds);
    // Do an early check for tokens escrow so that if it exists, it should allow the denom that we are sending
    let tokens = pair_info.get_vec_token_info();
    for token in tokens {
        // validate token
        token.token_type.validate(&deps.as_ref())?;

        // Ensure liquidity is not zero
        ensure!(!token.amount.is_zero(), ContractError::ZeroAssetAmount {});

        // Vouchers are not escrowed
        if !token.token_type.is_voucher() {
            let escrow_address = TOKEN_TO_ESCROW
                .load(deps.storage, token.token)
                .or(Err(ContractError::EscrowDoesNotExist {}))?;
            let token_allowed_query_msg = euclid::msgs::escrow::QueryMsg::TokenAllowed {
                denom: token.token_type.clone(),
            };
            let token_allowed: AllowedTokenResponse = deps
                .querier
                .query_wasm_smart(escrow_address.clone(), &token_allowed_query_msg)?;

            ensure!(
                token_allowed.allowed,
                ContractError::UnsupportedDenomination {}
            );

            match token.token_type {
                TokenType::Native { denom, .. } => {
                    ensure!(
                        !info.funds.is_empty(),
                        ContractError::InsufficientDeposit {}
                    );
                    // Use funds, if its not present this will throw error.
                    // This will make sure enough funds are provided with the message
                    fund_manager.use_fund(token.amount, &denom)?;
                }
                TokenType::Smart { .. } => {
                    let msg = token.token_type.create_transfer_msg(
                        token.amount,
                        env.contract.address.clone().to_string(),
                        Some(sender.address.clone()),
                        None,
                    )?;
                    msgs.push(SubMsg::new(msg));
                }
                TokenType::Voucher { .. } => return Err(ContractError::UnreachableCode {}),
            }
        }
    }

    ensure!(
        fund_manager.validate_funds_are_empty().is_ok(),
        ContractError::new("Extra funds are not allowed")
    );

    let liquidity_tx_info = AddLiquidityRequest {
        sender: info.sender.to_string(),
        pair_info: pair_info.clone(),
        tx_id: tx_id.clone(),
    };

    PENDING_ADD_LIQUIDITY.save(
        deps.storage,
        (info.sender.clone(), tx_id.clone()),
        &liquidity_tx_info,
    )?;

    let chain_type = get_chain_type(deps.as_ref(), &env)?;

    let add_liq_msg = RouterCrossChainExecuteMsg::AddLiquidity {
        sender,
        slippage_tolerance_bps,
        pair: pair_info,
        tx_id: tx_id.clone(),
    }
    .to_msg(
        deps,
        &env,
        state.router_contract,
        info.sender.clone(),
        state.chain_uid,
        chain_type,
        cross_chain_config.timeout,
        cross_chain_config.ack_response,
    )?;

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            info.sender.as_str(),
            euclid::events::TxType::AddLiquidity,
        ))
        .add_attribute("action", "add_liquidity")
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "add_liquidity_request")
        .add_attribute("token_1", pair.token_1.to_string())
        .add_attribute("token_2", pair.token_2.to_string())
        .add_submessages(msgs)
        .add_submessage(add_liq_msg))
}

// Remove liquidity from the pool
pub fn remove_liquidity_request(
    deps: &mut DepsMut,
    info: MessageInfo,
    env: Env,
    sender: CrossChainUser,
    pair: Pair,
    lp_allocation: Uint256,
    recipient: CrossChainUser,
    cross_chain_config: CrossChainConfig,
) -> Result<Response, ContractError> {
    // Reject mixed-case or empty addresses before mutating state
    sender.validate()?;
    recipient.validate()?;

    let state = STATE.load(deps.storage)?;
    let sender_addr = deps.api.addr_validate(&sender.address)?;

    let tx_id = generate_tx(deps, &env, &sender)?;

    ensure!(
        !PENDING_REMOVE_LIQUIDITY.has(deps.storage, (sender_addr.clone(), tx_id.clone())),
        ContractError::TxAlreadyExist {}
    );

    let vlp = PAIR_TO_VLP.load(deps.storage, pair.get_tupple())?;
    let lp_token = VLP_TO_LP_TOKEN.load(deps.storage, vlp)?;

    ensure!(lp_token == info.sender, ContractError::Unauthorized {});

    ensure!(
        PAIR_TO_VLP.has(deps.storage, pair.get_tupple()),
        ContractError::PoolDoesNotExist {}
    );

    // Check that the liquidity is greater than 0
    ensure!(!lp_allocation.is_zero(), ContractError::ZeroAssetAmount {});

    let liquidity_tx_info = RemoveLiquidityRequest {
        sender: sender_addr.to_string(),
        lp_allocation,
        pair: pair.clone(),
        tx_id: tx_id.clone(),
        lp_token,
    };

    PENDING_REMOVE_LIQUIDITY.save(
        deps.storage,
        (sender_addr.clone(), tx_id.clone()),
        &liquidity_tx_info,
    )?;

    let chain_type = get_chain_type(deps.as_ref(), &env)?;
    let token_1 = pair.token_1.to_string();
    let token_2 = pair.token_2.to_string();
    let remove_liq_msg =
        RouterCrossChainExecuteMsg::RemoveLiquidity(RouterCrossChainRemoveLiquidityExecuteMsg {
            sender,
            lp_allocation,
            pair,
            recipient,
            tx_id: tx_id.clone(),
        })
        .to_msg(
            deps,
            &env,
            state.router_contract,
            sender_addr.clone(),
            state.chain_uid,
            chain_type,
            cross_chain_config.timeout,
            cross_chain_config.ack_response,
        )?;

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            sender_addr.as_str(),
            euclid::events::TxType::RemoveLiquidity,
        ))
        .add_attribute("action", "remove_liquidity")
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "remove_liquidity_request")
        .add_attribute("token_1", token_1)
        .add_attribute("token_2", token_2)
        .add_submessage(remove_liq_msg))
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{
        testing::{message_info, mock_dependencies, mock_env},
        Uint128, Uint256,
    };
    use euclid::{
        error::ContractError,
        msgs::factory::ExecuteMsg,
        token::{Token, TokenType},
    };

    use crate::{
        contract::execute,
        testing::helpers::{
            default_cross_chain_config, init, seed_escrow, seed_vlp, set_escrow_token_allowed,
        },
    };

    // -----------------------------------------------------------------------
    // Execute: AddLiquidity – PoolDoesNotExist
    // -----------------------------------------------------------------------

    #[test]
    fn test_add_liquidity_pool_does_not_exist() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        deps.querier
            .bank
            .update_balance("any", vec![cosmwasm_std::coin(1_000_000, "uusdc")]);
        deps.querier
            .bank
            .update_balance("any2", vec![cosmwasm_std::coin(1_000_000, "ueth")]);

        let user = deps.api.addr_make("user");
        let info = message_info(
            &user,
            &[
                cosmwasm_std::coin(100, "uusdc"),
                cosmwasm_std::coin(100, "ueth"),
            ],
        );

        let pair_info = euclid::token::PairWithDenomAndAmount {
            token_1: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("eth".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "ueth".to_string(),
                    decimals: None,
                },
                amount: Uint256::from(100u128),
            },
            token_2: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("usdc".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uusdc".to_string(),
                    decimals: None,
                },
                amount: Uint256::from(100u128),
            },
        };
        let msg = ExecuteMsg::AddLiquidity {
            pair_with_denom_and_amount: pair_info,
            slippage_tolerance_bps: 50,
            cross_chain_config: default_cross_chain_config(),
        };
        let res = execute(deps.as_mut(), mock_env(), info, msg);
        assert_eq!(res.unwrap_err(), ContractError::PoolDoesNotExist {});
    }

    // -----------------------------------------------------------------------
    // Execute: AddLiquidity – invalid slippage (zero)
    // -----------------------------------------------------------------------

    #[test]
    fn test_add_liquidity_zero_slippage_fails() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");

        deps.querier
            .bank
            .update_balance("any", vec![cosmwasm_std::coin(1_000_000, "uusdc")]);
        deps.querier
            .bank
            .update_balance("any2", vec![cosmwasm_std::coin(1_000_000, "ueth")]);

        set_escrow_token_allowed(&mut deps, true);
        seed_escrow(&mut deps, "eth", "escrow_eth");
        seed_escrow(&mut deps, "usdc", "escrow_usdc");

        let user = deps.api.addr_make("user");
        let info = message_info(
            &user,
            &[
                cosmwasm_std::coin(100, "uusdc"),
                cosmwasm_std::coin(100, "ueth"),
            ],
        );
        let pair_info = euclid::token::PairWithDenomAndAmount {
            token_1: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("eth".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "ueth".to_string(),
                    decimals: None,
                },
                amount: Uint256::from(100u128),
            },
            token_2: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("usdc".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uusdc".to_string(),
                    decimals: None,
                },
                amount: Uint256::from(100u128),
            },
        };
        let msg = ExecuteMsg::AddLiquidity {
            pair_with_denom_and_amount: pair_info,
            slippage_tolerance_bps: 0,
            cross_chain_config: default_cross_chain_config(),
        };
        let res = execute(deps.as_mut(), mock_env(), info, msg);
        assert_eq!(res.unwrap_err(), ContractError::InvalidSlippageTolerance {});
    }

    // -----------------------------------------------------------------------
    // Execute: AddLiquidity – zero token amount
    // -----------------------------------------------------------------------

    #[test]
    fn test_add_liquidity_zero_amount_fails() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");
        seed_escrow(&mut deps, "eth", "escrow_eth");
        seed_escrow(&mut deps, "usdc", "escrow_usdc");

        deps.querier
            .bank
            .update_balance("any", vec![cosmwasm_std::coin(1_000_000, "uusdc")]);
        deps.querier
            .bank
            .update_balance("any2", vec![cosmwasm_std::coin(1_000_000, "ueth")]);

        set_escrow_token_allowed(&mut deps, true);

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[]);

        let pair_info = euclid::token::PairWithDenomAndAmount {
            token_1: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("eth".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "ueth".to_string(),
                    decimals: None,
                },
                amount: Uint256::zero(),
            },
            token_2: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("usdc".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uusdc".to_string(),
                    decimals: None,
                },
                amount: Uint256::from(100u128),
            },
        };
        let msg = ExecuteMsg::AddLiquidity {
            pair_with_denom_and_amount: pair_info,
            slippage_tolerance_bps: 50,
            cross_chain_config: default_cross_chain_config(),
        };
        let res = execute(deps.as_mut(), mock_env(), info, msg);
        assert_eq!(res.unwrap_err(), ContractError::ZeroAssetAmount {});
    }

    // -----------------------------------------------------------------------
    // Execute: RequestPoolCreation – slippage tolerance exceeds 100%
    // -----------------------------------------------------------------------

    #[test]
    fn test_request_pool_creation_slippage_too_high_fails() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        deps.querier
            .bank
            .update_balance("any", vec![cosmwasm_std::coin(1_000_000, "uusdc")]);
        deps.querier
            .bank
            .update_balance("any2", vec![cosmwasm_std::coin(1_000_000, "ueth")]);

        let user = deps.api.addr_make("user");
        let info = message_info(
            &user,
            &[
                cosmwasm_std::coin(100, "uusdc"),
                cosmwasm_std::coin(100, "ueth"),
            ],
        );

        let pair_info = euclid::token::PairWithDenomAndAmount {
            token_1: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("eth".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "ueth".to_string(),
                    decimals: None,
                },
                amount: Uint256::from(100u128),
            },
            token_2: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("usdc".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uusdc".to_string(),
                    decimals: None,
                },
                amount: Uint256::from(100u128),
            },
        };

        let msg = ExecuteMsg::RequestPoolCreation {
            pair_with_denom_and_amount: pair_info,
            pool_config: euclid::msgs::vlp::base::PoolConfig::ConstantProduct {},
            lp_token_name: "LP Token".to_string(),
            lp_token_symbol: "LPT".to_string(),
            lp_token_decimal: 6,
            slippage_tolerance_bps: 10_001,
            lp_token_marketing: None,
            cross_chain_config: default_cross_chain_config(),
        };
        let res = execute(deps.as_mut(), mock_env(), info, msg);
        assert_eq!(res.unwrap_err(), ContractError::InvalidSlippageTolerance {});
    }

    // -----------------------------------------------------------------------
    // Execute: RequestPoolCreation – pool already exists
    // -----------------------------------------------------------------------

    #[test]
    fn test_request_pool_creation_pool_already_exists_fails() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_vlp(&mut deps, "eth", "usdc", "existing_vlp");
        seed_escrow(&mut deps, "eth", "escrow_eth");
        seed_escrow(&mut deps, "usdc", "escrow_usdc");

        deps.querier
            .bank
            .update_balance("any", vec![cosmwasm_std::coin(1_000_000, "uusdc")]);
        deps.querier
            .bank
            .update_balance("any2", vec![cosmwasm_std::coin(1_000_000, "ueth")]);

        set_escrow_token_allowed(&mut deps, true);

        let user = deps.api.addr_make("user");
        let info = message_info(
            &user,
            &[
                cosmwasm_std::coin(100, "uusdc"),
                cosmwasm_std::coin(100, "ueth"),
            ],
        );

        let pair_info = euclid::token::PairWithDenomAndAmount {
            token_1: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("eth".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "ueth".to_string(),
                    decimals: None,
                },
                amount: Uint256::from(100u128),
            },
            token_2: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("usdc".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uusdc".to_string(),
                    decimals: None,
                },
                amount: Uint256::from(100u128),
            },
        };

        let msg = ExecuteMsg::RequestPoolCreation {
            pair_with_denom_and_amount: pair_info,
            pool_config: euclid::msgs::vlp::base::PoolConfig::ConstantProduct {},
            lp_token_name: "LP Token".to_string(),
            lp_token_symbol: "LPT".to_string(),
            lp_token_decimal: 6,
            slippage_tolerance_bps: 50,
            lp_token_marketing: None,
            cross_chain_config: default_cross_chain_config(),
        };
        let res = execute(deps.as_mut(), mock_env(), info, msg);
        assert_eq!(res.unwrap_err(), ContractError::PoolAlreadyExists {});
    }

    // -----------------------------------------------------------------------
    // Execute: RequestPoolCreation – same token on both sides
    // -----------------------------------------------------------------------

    #[test]
    fn test_request_pool_creation_same_token_fails() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        deps.querier
            .bank
            .update_balance("any", vec![cosmwasm_std::coin(1_000_000, "uusdc")]);

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[cosmwasm_std::coin(100, "uusdc")]);

        let pair_info = euclid::token::PairWithDenomAndAmount {
            token_1: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("usdc".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uusdc".to_string(),
                    decimals: None,
                },
                amount: Uint256::from(100u128),
            },
            token_2: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("usdc".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uusdc".to_string(),
                    decimals: None,
                },
                amount: Uint256::from(100u128),
            },
        };

        let msg = ExecuteMsg::RequestPoolCreation {
            pair_with_denom_and_amount: pair_info,
            pool_config: euclid::msgs::vlp::base::PoolConfig::ConstantProduct {},
            lp_token_name: "LP Token".to_string(),
            lp_token_symbol: "LPT".to_string(),
            lp_token_decimal: 6,
            slippage_tolerance_bps: 50,
            lp_token_marketing: None,
            cross_chain_config: default_cross_chain_config(),
        };
        let res = execute(deps.as_mut(), mock_env(), info, msg);
        assert!(res.is_err());
    }

    // -----------------------------------------------------------------------
    // Execute: RequestPoolCreation – both tokens new (no pre-existing escrow)
    // -----------------------------------------------------------------------

    // Reorg-replay safety: regenerating the same tx_id on RequestPoolCreation
    // must hit the PENDING_POOL_REQUESTS TxAlreadyExist guard before the
    // PoolAlreadyExists check.
    #[test]
    fn test_request_pool_creation_duplicate_tx_id_rejected() {
        use euclid::utils::tx::TX_NONCES;

        let mut deps = mock_dependencies();
        init(&mut deps);
        // One token must already be registered (escrow exists) so the second
        // token is treated as the new one; the pair itself is NOT seeded into
        // PAIR_TO_VLP, so the first call succeeds.
        seed_escrow(&mut deps, "usdc", "escrow_usdc");
        set_escrow_token_allowed(&mut deps, true);

        deps.querier
            .bank
            .update_balance("any", vec![cosmwasm_std::coin(1_000_000, "uusdc")]);
        deps.querier
            .bank
            .update_balance("any2", vec![cosmwasm_std::coin(1_000_000, "ueth")]);

        let user = deps.api.addr_make("user");
        let make_msg = || ExecuteMsg::RequestPoolCreation {
            pair_with_denom_and_amount: euclid::token::PairWithDenomAndAmount {
                token_1: euclid::token::TokenWithDenomAndAmount {
                    token: Token::create("eth".to_string()).unwrap(),
                    token_type: TokenType::Native {
                        denom: "ueth".to_string(),
                        decimals: Some(6),
                    },
                    amount: Uint256::from(100u128),
                },
                token_2: euclid::token::TokenWithDenomAndAmount {
                    token: Token::create("usdc".to_string()).unwrap(),
                    token_type: TokenType::Native {
                        denom: "uusdc".to_string(),
                        decimals: Some(6),
                    },
                    amount: Uint256::from(100u128),
                },
            },
            pool_config: euclid::msgs::vlp::base::PoolConfig::ConstantProduct {},
            lp_token_name: "LP Token".to_string(),
            lp_token_symbol: "LPT".to_string(),
            lp_token_decimal: 6,
            slippage_tolerance_bps: 50,
            lp_token_marketing: None,
            cross_chain_config: default_cross_chain_config(),
        };
        let funds = [
            cosmwasm_std::coin(100, "uusdc"),
            cosmwasm_std::coin(100, "ueth"),
        ];

        execute(
            deps.as_mut(),
            mock_env(),
            message_info(&user, &funds),
            make_msg(),
        )
        .unwrap();

        TX_NONCES
            .save(deps.as_mut().storage, format!("testchain:{user}"), &0u128)
            .unwrap();

        let err = execute(
            deps.as_mut(),
            mock_env(),
            message_info(&user, &funds),
            make_msg(),
        )
        .unwrap_err();
        assert_eq!(err, ContractError::TxAlreadyExist {});
    }

    // Reorg-replay safety for AddLiquidity: the PENDING_ADD_LIQUIDITY guard
    // must reject a regenerated tx_id even though the pool exists.
    #[test]
    fn test_add_liquidity_duplicate_tx_id_rejected() {
        use euclid::utils::tx::TX_NONCES;

        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");
        seed_escrow(&mut deps, "eth", "escrow_eth");
        seed_escrow(&mut deps, "usdc", "escrow_usdc");
        set_escrow_token_allowed(&mut deps, true);

        deps.querier
            .bank
            .update_balance("any", vec![cosmwasm_std::coin(1_000_000, "uusdc")]);
        deps.querier
            .bank
            .update_balance("any2", vec![cosmwasm_std::coin(1_000_000, "ueth")]);

        let user = deps.api.addr_make("user");
        let make_msg = || ExecuteMsg::AddLiquidity {
            pair_with_denom_and_amount: euclid::token::PairWithDenomAndAmount {
                token_1: euclid::token::TokenWithDenomAndAmount {
                    token: Token::create("eth".to_string()).unwrap(),
                    token_type: TokenType::Native {
                        denom: "ueth".to_string(),
                        decimals: None,
                    },
                    amount: Uint256::from(100u128),
                },
                token_2: euclid::token::TokenWithDenomAndAmount {
                    token: Token::create("usdc".to_string()).unwrap(),
                    token_type: TokenType::Native {
                        denom: "uusdc".to_string(),
                        decimals: None,
                    },
                    amount: Uint256::from(100u128),
                },
            },
            slippage_tolerance_bps: 50,
            cross_chain_config: default_cross_chain_config(),
        };
        let funds = [
            cosmwasm_std::coin(100, "uusdc"),
            cosmwasm_std::coin(100, "ueth"),
        ];

        execute(
            deps.as_mut(),
            mock_env(),
            message_info(&user, &funds),
            make_msg(),
        )
        .unwrap();

        TX_NONCES
            .save(deps.as_mut().storage, format!("testchain:{user}"), &0u128)
            .unwrap();

        let err = execute(
            deps.as_mut(),
            mock_env(),
            message_info(&user, &funds),
            make_msg(),
        )
        .unwrap_err();
        assert_eq!(err, ContractError::TxAlreadyExist {});
    }

    // Reorg-replay safety for RemoveLiquidity: the PENDING_REMOVE_LIQUIDITY
    // guard must reject a regenerated tx_id. RemoveLiquidity is reached via
    // a CW20 hook, so this test calls `remove_liquidity_request` directly
    // (the same path the CW20 receive handler invokes).
    #[test]
    fn test_remove_liquidity_duplicate_tx_id_rejected() {
        use crate::execute::pool::remove_liquidity_request;
        use crate::state::VLP_TO_LP_TOKEN;
        use euclid::utils::tx::TX_NONCES;

        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");
        seed_escrow(&mut deps, "eth", "escrow_eth");
        seed_escrow(&mut deps, "usdc", "escrow_usdc");
        let lp_token = cosmwasm_std::Addr::unchecked("lp_addr");
        VLP_TO_LP_TOKEN
            .save(deps.as_mut().storage, "vlp_addr".to_string(), &lp_token)
            .unwrap();

        let user_addr = deps.api.addr_make("user");
        let sender = euclid::cross_chain_user::CrossChainUser::new(
            euclid::chain::ChainUid::create(crate::testing::helpers::TEST_CHAIN_UID.to_string())
                .unwrap(),
            user_addr.to_string(),
        );
        let pair = euclid::token::Pair::new(
            Token::create("eth".to_string()).unwrap(),
            Token::create("usdc".to_string()).unwrap(),
        )
        .unwrap();
        let lp_info = message_info(&lp_token, &[]);

        remove_liquidity_request(
            &mut deps.as_mut(),
            lp_info.clone(),
            mock_env(),
            sender.clone(),
            pair.clone(),
            Uint256::from(10u128),
            sender.clone(),
            default_cross_chain_config(),
        )
        .unwrap();

        TX_NONCES
            .save(
                deps.as_mut().storage,
                format!("testchain:{user_addr}"),
                &0u128,
            )
            .unwrap();

        let err = remove_liquidity_request(
            &mut deps.as_mut(),
            lp_info,
            mock_env(),
            sender.clone(),
            pair,
            Uint256::from(10u128),
            sender,
            default_cross_chain_config(),
        )
        .unwrap_err();
        assert_eq!(err, ContractError::TxAlreadyExist {});
    }

    #[test]
    fn test_request_pool_creation_both_tokens_new_fails() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        deps.querier
            .bank
            .update_balance("any", vec![cosmwasm_std::coin(1_000_000, "uaaa")]);
        deps.querier
            .bank
            .update_balance("any2", vec![cosmwasm_std::coin(1_000_000, "ubbb")]);

        let user = deps.api.addr_make("user");
        let info = message_info(
            &user,
            &[
                cosmwasm_std::coin(100, "uaaa"),
                cosmwasm_std::coin(100, "ubbb"),
            ],
        );

        let pair_info = euclid::token::PairWithDenomAndAmount {
            token_1: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("aaa".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uaaa".to_string(),
                    decimals: None,
                },
                amount: Uint256::from(100u128),
            },
            token_2: euclid::token::TokenWithDenomAndAmount {
                token: Token::create("bbb".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "ubbb".to_string(),
                    decimals: None,
                },
                amount: Uint256::from(100u128),
            },
        };

        let msg = ExecuteMsg::RequestPoolCreation {
            pair_with_denom_and_amount: pair_info,
            pool_config: euclid::msgs::vlp::base::PoolConfig::ConstantProduct {},
            lp_token_name: "LP Token".to_string(),
            lp_token_symbol: "LPT".to_string(),
            lp_token_decimal: 6,
            slippage_tolerance_bps: 50,
            lp_token_marketing: None,
            cross_chain_config: default_cross_chain_config(),
        };
        let res = execute(deps.as_mut(), mock_env(), info, msg);
        assert!(matches!(res.unwrap_err(), ContractError::Generic { .. }));
    }
}
