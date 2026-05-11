use cosmwasm_std::{ensure, Decimal, DepsMut, Env, MessageInfo, Response, SubMsg, Uint256};
use cw20::Logo;
use euclid::{
    cross_chain_user::CrossChainUser,
    error::ContractError,
    events::{tx_event, TxType},
    fee::{PartnerFee, BPS_100_PERCENT, MAX_PARTNER_FEE_BPS},
    liquidity::{AddLiquidityRequest, RemoveLiquidityRequest, SingleSidedLiquidityRequest},
    msgs::{
        cross_chain_config::CrossChainConfig, escrow::AllowedTokenResponse, vlp::base::PoolConfig,
    },
    swap::NextSwapPair,
    token::{Pair, PairWithDenomAndAmount, Token, TokenType, TokenWithDenom},
    utils::{fund_manager::FundManager, tx::generate_tx},
};
use euclid_ibc::router_ibc::{
    RouterCrossChainExecuteMsg, RouterCrossChainRemoveLiquidityExecuteMsg,
    RouterCrossChainSingleSidedAddLiquidityMsg,
};

use crate::{
    query::get_chain_type,
    state::{
        PoolCreateRequest, PAIR_TO_VLP, PENDING_ADD_LIQUIDITY, PENDING_POOL_REQUESTS,
        PENDING_REMOVE_LIQUIDITY, PENDING_SINGLE_SIDED_LIQUIDITY, STATE, TOKEN_TO_ESCROW,
        VLP_TO_LP_TOKEN,
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

// Single-sided add liquidity: user deposits one token, the hub atomically swaps
// a backend-computed portion through the target VLP and adds liquidity on the
// same VLP, all in one IBC roundtrip.
#[allow(clippy::too_many_arguments)]
pub fn execute_single_sided_add_liquidity_request(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    asset_in: TokenWithDenom,
    amount_in: Uint256,
    asset_out: Token,
    swap_amount: Uint256,
    swap_route: Vec<NextSwapPair>,
    min_lp_out: Uint256,
    partner_fee: Option<PartnerFee>,
    cross_chain_config: CrossChainConfig,
) -> Result<Response, ContractError> {
    asset_in.token.validate()?;
    asset_in.token_type.validate(&deps.as_ref())?;
    asset_out.validate()?;

    ensure!(
        asset_in.token != asset_out,
        ContractError::new("asset_in must differ from asset_out")
    );

    // Partner fee: same model as execute_swap_request.
    // The full `amount_in` from the user is split into:
    //   - partner_fee_amount: retained at the factory until ack resolves
    //   - amount_in (rebound below): the portion that crosses IBC
    let partner_fee_bps = partner_fee
        .as_ref()
        .map(|fee| fee.partner_fee_bps)
        .unwrap_or(0);
    ensure!(
        partner_fee_bps <= MAX_PARTNER_FEE_BPS,
        ContractError::InvalidPartnerFee {}
    );
    let partner_fee_amount = amount_in.checked_mul_ceil(Decimal::bps(partner_fee_bps))?;
    let amount_in = amount_in.checked_sub(partner_fee_amount)?;

    ensure!(!amount_in.is_zero(), ContractError::ZeroAssetAmount {});
    ensure!(!swap_amount.is_zero(), ContractError::ZeroAssetAmount {});
    ensure!(
        swap_amount < amount_in,
        ContractError::new("swap_amount must be < amount_in")
    );
    ensure!(!min_lp_out.is_zero(), ContractError::ZeroAssetAmount {});

    // Single-hop in v1 — kept Vec for forward-compat.
    ensure!(
        swap_route.len() == 1,
        ContractError::new("swap_route must contain exactly one hop in v1")
    );
    let hop = &swap_route[0];
    ensure!(
        hop.token_in == asset_in.token,
        ContractError::new("swap_route first hop token_in must match asset_in")
    );
    ensure!(
        hop.token_out == asset_out,
        ContractError::new("swap_route last hop token_out must match asset_out")
    );

    let state = STATE.load(deps.storage)?;
    let sender = CrossChainUser::new(state.chain_uid.clone(), info.sender.to_string());
    let tx_id = generate_tx(deps, &env, &sender)?;

    ensure!(
        !PENDING_SINGLE_SIDED_LIQUIDITY.has(deps.storage, (info.sender.clone(), tx_id.clone())),
        ContractError::TxAlreadyExist {}
    );

    // Target VLP must already exist — fail fast before paying IBC roundtrip.
    let pair = Pair::new(asset_in.token.clone(), asset_out.clone())?;
    ensure!(
        PAIR_TO_VLP.has(deps.storage, pair.get_tupple()),
        ContractError::PoolDoesNotExist {}
    );

    // Escrow must exist and allow this denom.
    let escrow_address = TOKEN_TO_ESCROW
        .load(deps.storage, asset_in.token.clone())
        .or(Err(ContractError::EscrowDoesNotExist {}))?;
    let token_allowed: AllowedTokenResponse = deps.querier.query_wasm_smart(
        escrow_address,
        &euclid::msgs::escrow::QueryMsg::TokenAllowed {
            denom: asset_in.token_type.clone(),
        },
    )?;
    ensure!(
        token_allowed.allowed,
        ContractError::UnsupportedDenomination {}
    );

    // Handle funds. v1: Native only. Smart support layered on in a follow-up issue;
    // Voucher is structurally impossible from a remote-chain factory.
    // Note: native funds must cover the FULL deposit including partner_fee_amount.
    let full_amount = amount_in.checked_add(partner_fee_amount)?;
    let mut fund_manager = FundManager::new(&info.funds);
    match &asset_in.token_type {
        TokenType::Native { denom, .. } => {
            fund_manager.use_fund(full_amount, denom)?;
        }
        TokenType::Smart { .. } => {
            return Err(ContractError::new(
                "Smart (CW20) asset_in not supported in this version",
            ));
        }
        TokenType::Voucher { .. } => return Err(ContractError::UnreachableCode {}),
    }
    ensure!(
        fund_manager.validate_funds_are_empty().is_ok(),
        ContractError::new("Extra funds are not allowed")
    );

    // Resolve partner-fee recipient: default to sender if not specified.
    let partner_fee_recipient = partner_fee
        .as_ref()
        .map(|fee| deps.api.addr_validate(&fee.recipient))
        .transpose()?
        .unwrap_or(info.sender.clone());

    let pending = SingleSidedLiquidityRequest {
        sender: info.sender.to_string(),
        tx_id: tx_id.clone(),
        asset_in: asset_in.clone(),
        amount_in,
        partner_fee_amount,
        partner_fee_recipient: partner_fee_recipient.clone(),
    };
    PENDING_SINGLE_SIDED_LIQUIDITY.save(
        deps.storage,
        (info.sender.clone(), tx_id.clone()),
        &pending,
    )?;

    let chain_type = get_chain_type(deps.as_ref(), &env)?;

    let ibc_msg = RouterCrossChainExecuteMsg::SingleSidedAddLiquidity(
        RouterCrossChainSingleSidedAddLiquidityMsg {
            sender,
            asset_in: asset_in.clone(),
            amount_in,
            swap_amount,
            asset_out: asset_out.clone(),
            swaps: swap_route,
            min_lp_out,
            partner_fee_amount,
            partner_fee_recipient: CrossChainUser::new(
                state.chain_uid.clone(),
                partner_fee_recipient.to_string(),
            ),
            tx_id: tx_id.clone(),
        },
    )
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
            TxType::SingleSidedAddLiquidity,
        ))
        .add_attribute("action", "single_sided_add_liquidity")
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "execute_single_sided_add_liquidity_request")
        .add_attribute("asset_in", asset_in.token.to_string())
        .add_attribute("asset_out", asset_out.to_string())
        .add_attribute("amount_in", amount_in)
        .add_attribute("swap_amount", swap_amount)
        .add_submessage(ibc_msg))
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

    // -----------------------------------------------------------------------
    // Execute: AddSingleSidedLiquidity tests
    // -----------------------------------------------------------------------

    use cosmwasm_std::{to_json_binary, ContractResult, SystemResult, WasmQuery};
    use euclid::{
        msgs::escrow::AllowedTokenResponse,
        swap::NextSwapPair,
        token::{Pair, TokenWithDenom},
    };

    use crate::state::PENDING_SINGLE_SIDED_LIQUIDITY;

    fn native_token_with_denom(token_id: &str, denom: &str) -> TokenWithDenom {
        TokenWithDenom {
            token: Token::create(token_id.to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: denom.to_string(),
                decimals: None,
            },
        }
    }

    fn single_hop(token_in: &str, token_out: &str) -> Vec<NextSwapPair> {
        vec![NextSwapPair {
            token_in: Token::create(token_in.to_string()).unwrap(),
            token_out: Token::create(token_out.to_string()).unwrap(),
            test_fail: None,
        }]
    }

    fn ss_default_msg() -> ExecuteMsg {
        ExecuteMsg::AddSingleSidedLiquidity {
            asset_in: native_token_with_denom("eth", "ueth"),
            amount_in: Uint256::from(1000u128),
            asset_out: Token::create("usdc".to_string()).unwrap(),
            swap_amount: Uint256::from(500u128),
            swap_route: single_hop("eth", "usdc"),
            min_lp_out: Uint256::from(1u128),
            partner_fee: None,
            cross_chain_config: default_cross_chain_config(),
        }
    }

    /// Seed mock querier for `ueth` supply so `TokenType::Native::validate` succeeds.
    fn set_native_supply(deps: &mut crate::testing::helpers::MockDeps) {
        deps.querier
            .bank
            .update_balance("anywhere", vec![cosmwasm_std::coin(1_000_000, "ueth")]);
        deps.querier
            .bank
            .update_balance("anywhere2", vec![cosmwasm_std::coin(1_000_000, "uusdc")]);
    }

    /// Happy path: native USDC-style deposit creates pending entry and emits IBC submsg.
    #[test]
    fn test_single_sided_happy_path_native_saves_pending_and_emits_ibc() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);
        seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");
        seed_escrow(&mut deps, "eth", "escrow_eth");
        set_escrow_token_allowed(&mut deps, true);

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[cosmwasm_std::coin(1000, "ueth")]);

        let msg = ss_default_msg();
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        // One submsg: the IBC packet
        assert_eq!(res.messages.len(), 1);

        // PENDING_SINGLE_SIDED_LIQUIDITY should now contain exactly one entry for this sender.
        let entries: Vec<_> = PENDING_SINGLE_SIDED_LIQUIDITY
            .range(&deps.storage, None, None, cosmwasm_std::Order::Ascending)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(entries.len(), 1);
        let ((sender_addr, _tx_id), pending) = &entries[0];
        assert_eq!(sender_addr, &user);
        assert_eq!(pending.sender, user.to_string());
        assert_eq!(pending.amount_in, Uint256::from(1000u128));
        assert_eq!(pending.asset_in.token.to_string(), "eth");

        // Attributes
        let attrs = &res.attributes;
        assert!(attrs
            .iter()
            .any(|a| a.key == "method" && a.value == "execute_single_sided_add_liquidity_request"));
        assert!(attrs
            .iter()
            .any(|a| a.key == "asset_in" && a.value == "eth"));
        assert!(attrs
            .iter()
            .any(|a| a.key == "asset_out" && a.value == "usdc"));
    }

    /// PAIR_TO_VLP missing for (asset_in, asset_out) → PoolDoesNotExist.
    #[test]
    fn test_single_sided_pool_does_not_exist() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);
        // intentionally do NOT seed_vlp

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[cosmwasm_std::coin(1000, "ueth")]);

        let msg = ss_default_msg();
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::PoolDoesNotExist {});
    }

    /// asset_in.token == asset_out → "must differ" error.
    #[test]
    fn test_single_sided_same_asset_rejected() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[cosmwasm_std::coin(1000, "ueth")]);

        let mut msg = ss_default_msg();
        if let ExecuteMsg::AddSingleSidedLiquidity {
            ref mut asset_out, ..
        } = msg
        {
            *asset_out = Token::create("eth".to_string()).unwrap();
        }
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(
            err,
            ContractError::new("asset_in must differ from asset_out")
        );
    }

    /// amount_in == 0 → ZeroAssetAmount.
    #[test]
    fn test_single_sided_zero_amount_in() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[]);

        let mut msg = ss_default_msg();
        if let ExecuteMsg::AddSingleSidedLiquidity {
            ref mut amount_in, ..
        } = msg
        {
            *amount_in = Uint256::zero();
        }
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::ZeroAssetAmount {});
    }

    /// swap_amount == 0 → ZeroAssetAmount.
    #[test]
    fn test_single_sided_zero_swap_amount() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[cosmwasm_std::coin(1000, "ueth")]);

        let mut msg = ss_default_msg();
        if let ExecuteMsg::AddSingleSidedLiquidity {
            ref mut swap_amount,
            ..
        } = msg
        {
            *swap_amount = Uint256::zero();
        }
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::ZeroAssetAmount {});
    }

    /// swap_amount > amount_in → "swap_amount must be < amount_in".
    #[test]
    fn test_single_sided_swap_amount_greater_than_amount_in() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[cosmwasm_std::coin(2000, "ueth")]);

        let mut msg = ss_default_msg();
        if let ExecuteMsg::AddSingleSidedLiquidity {
            ref mut swap_amount,
            ref mut amount_in,
            ..
        } = msg
        {
            *amount_in = Uint256::from(1000u128);
            *swap_amount = Uint256::from(2000u128);
        }
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::new("swap_amount must be < amount_in"));
    }

    /// swap_amount == amount_in (boundary) → "swap_amount must be < amount_in".
    #[test]
    fn test_single_sided_swap_amount_equals_amount_in_rejected() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[cosmwasm_std::coin(1000, "ueth")]);

        let mut msg = ss_default_msg();
        if let ExecuteMsg::AddSingleSidedLiquidity {
            ref mut swap_amount,
            ref mut amount_in,
            ..
        } = msg
        {
            *amount_in = Uint256::from(1000u128);
            *swap_amount = Uint256::from(1000u128);
        }
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::new("swap_amount must be < amount_in"));
    }

    /// min_lp_out == 0 → ZeroAssetAmount.
    #[test]
    fn test_single_sided_zero_min_lp_out() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[cosmwasm_std::coin(1000, "ueth")]);

        let mut msg = ss_default_msg();
        if let ExecuteMsg::AddSingleSidedLiquidity {
            ref mut min_lp_out, ..
        } = msg
        {
            *min_lp_out = Uint256::zero();
        }
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::ZeroAssetAmount {});
    }

    /// swap_route.len() != 1 (e.g. empty) → "exactly one hop in v1".
    #[test]
    fn test_single_sided_empty_swap_route() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[cosmwasm_std::coin(1000, "ueth")]);

        let mut msg = ss_default_msg();
        if let ExecuteMsg::AddSingleSidedLiquidity {
            ref mut swap_route, ..
        } = msg
        {
            *swap_route = vec![];
        }
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(
            err,
            ContractError::new("swap_route must contain exactly one hop in v1")
        );
    }

    /// swap_route.len() > 1 → "exactly one hop in v1".
    #[test]
    fn test_single_sided_multi_hop_swap_route_rejected() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[cosmwasm_std::coin(1000, "ueth")]);

        let mut msg = ss_default_msg();
        if let ExecuteMsg::AddSingleSidedLiquidity {
            ref mut swap_route, ..
        } = msg
        {
            let mut routes = single_hop("eth", "usdc");
            routes.push(NextSwapPair {
                token_in: Token::create("usdc".to_string()).unwrap(),
                token_out: Token::create("dai".to_string()).unwrap(),
                test_fail: None,
            });
            *swap_route = routes;
        }
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(
            err,
            ContractError::new("swap_route must contain exactly one hop in v1")
        );
    }

    /// swap_route[0].token_in != asset_in.token → hop_in mismatch error.
    #[test]
    fn test_single_sided_route_token_in_mismatch() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[cosmwasm_std::coin(1000, "ueth")]);

        let mut msg = ss_default_msg();
        if let ExecuteMsg::AddSingleSidedLiquidity {
            ref mut swap_route, ..
        } = msg
        {
            *swap_route = single_hop("dai", "usdc"); // asset_in is eth, not dai
        }
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(
            err,
            ContractError::new("swap_route first hop token_in must match asset_in")
        );
    }

    /// swap_route[0].token_out != asset_out → hop_out mismatch error.
    #[test]
    fn test_single_sided_route_token_out_mismatch() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[cosmwasm_std::coin(1000, "ueth")]);

        let mut msg = ss_default_msg();
        if let ExecuteMsg::AddSingleSidedLiquidity {
            ref mut swap_route, ..
        } = msg
        {
            *swap_route = single_hop("eth", "dai"); // asset_out is usdc, not dai
        }
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(
            err,
            ContractError::new("swap_route last hop token_out must match asset_out")
        );
    }

    /// Escrow does not exist for asset_in.token → EscrowDoesNotExist.
    #[test]
    fn test_single_sided_escrow_does_not_exist() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);
        seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");
        // No seed_escrow for "eth"

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[cosmwasm_std::coin(1000, "ueth")]);

        let msg = ss_default_msg();
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::EscrowDoesNotExist {});
    }

    /// Escrow exists but TokenAllowed returns false → UnsupportedDenomination.
    #[test]
    fn test_single_sided_token_not_allowed_by_escrow() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);
        seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");
        seed_escrow(&mut deps, "eth", "escrow_eth");
        set_escrow_token_allowed(&mut deps, false);

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[cosmwasm_std::coin(1000, "ueth")]);

        let msg = ss_default_msg();
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::UnsupportedDenomination {});
    }

    /// Native funds insufficient (info.funds doesn't contain enough of the denom)
    /// → fund_manager InsufficientFunds.
    #[test]
    fn test_single_sided_native_funds_mismatch() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);
        seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");
        seed_escrow(&mut deps, "eth", "escrow_eth");
        set_escrow_token_allowed(&mut deps, true);

        let user = deps.api.addr_make("user");
        // Send only 100 of ueth even though amount_in = 1000.
        let info = message_info(&user, &[cosmwasm_std::coin(100, "ueth")]);

        let msg = ss_default_msg();
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::InsufficientFunds {});
    }

    /// Extra funds beyond what amount_in requires → "Extra funds are not allowed".
    #[test]
    fn test_single_sided_extra_funds_rejected() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);
        seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");
        seed_escrow(&mut deps, "eth", "escrow_eth");
        set_escrow_token_allowed(&mut deps, true);

        let user = deps.api.addr_make("user");
        // Send the right amount of ueth, plus an extra unrelated denom.
        let info = message_info(
            &user,
            &[
                cosmwasm_std::coin(1000, "ueth"),
                cosmwasm_std::coin(50, "uextra"),
            ],
        );

        let msg = ss_default_msg();
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::new("Extra funds are not allowed"));
    }

    /// Smart (CW20) asset_in → explicit "not supported" error.
    /// Mocks both ContractInfo (for token_type.validate) and Smart queries.
    #[test]
    fn test_single_sided_smart_asset_in_rejected() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");
        seed_escrow(&mut deps, "eth", "escrow_eth");

        // Mock all wasm queries: ContractInfo always succeeds; TokenAllowed always true.
        deps.querier.update_wasm(move |q| match q {
            WasmQuery::ContractInfo { .. } => SystemResult::Ok(ContractResult::Ok(
                to_json_binary(&cosmwasm_std::ContractInfoResponse::new(
                    1,
                    cosmwasm_std::Addr::unchecked("creator"),
                    Some(cosmwasm_std::Addr::unchecked("admin")),
                    false,
                    None,
                ))
                .unwrap(),
            )),
            WasmQuery::Smart { msg, .. } => {
                let query: serde_json::Value = serde_json::from_slice(msg.as_slice()).unwrap();
                if query.get("token_allowed").is_some() {
                    SystemResult::Ok(ContractResult::Ok(
                        to_json_binary(&AllowedTokenResponse { allowed: true }).unwrap(),
                    ))
                } else {
                    panic!("unexpected smart query")
                }
            }
            _ => panic!("unexpected query"),
        });

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[]);

        let smart_token = TokenWithDenom {
            token: Token::create("eth".to_string()).unwrap(),
            token_type: TokenType::Smart {
                contract_address: deps.api.addr_make("cw20").to_string(),
                decimals: Some(6),
            },
        };
        let msg = ExecuteMsg::AddSingleSidedLiquidity {
            asset_in: smart_token,
            amount_in: Uint256::from(1000u128),
            asset_out: Token::create("usdc".to_string()).unwrap(),
            swap_amount: Uint256::from(500u128),
            swap_route: single_hop("eth", "usdc"),
            min_lp_out: Uint256::from(1u128),
            partner_fee: None,
            cross_chain_config: default_cross_chain_config(),
        };
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(
            err,
            ContractError::new("Smart (CW20) asset_in not supported in this version")
        );
    }

    /// Voucher asset_in → UnreachableCode (Voucher is structurally impossible from a remote factory).
    #[test]
    fn test_single_sided_voucher_asset_in_rejected() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");
        seed_escrow(&mut deps, "eth", "escrow_eth");
        set_escrow_token_allowed(&mut deps, true);

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[]);

        let voucher = TokenWithDenom {
            token: Token::create("eth".to_string()).unwrap(),
            token_type: TokenType::Voucher {},
        };
        let msg = ExecuteMsg::AddSingleSidedLiquidity {
            asset_in: voucher,
            amount_in: Uint256::from(1000u128),
            asset_out: Token::create("usdc".to_string()).unwrap(),
            swap_amount: Uint256::from(500u128),
            swap_route: single_hop("eth", "usdc"),
            min_lp_out: Uint256::from(1u128),
            partner_fee: None,
            cross_chain_config: default_cross_chain_config(),
        };
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::UnreachableCode {});
    }

    /// Sanity: silence "unused" warnings for items only consumed by certain test cases.
    #[allow(dead_code)]
    fn _silence_unused() {
        let _ = (Pair::new, Uint128::zero);
    }

    // -----------------------------------------------------------------------
    // Execute: AddSingleSidedLiquidity – partner fee tests
    // -----------------------------------------------------------------------

    use euclid::fee::PartnerFee;

    /// partner_fee_bps > MAX_PARTNER_FEE_BPS (30) → InvalidPartnerFee.
    #[test]
    fn test_single_sided_partner_fee_exceeds_cap() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);
        seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");
        seed_escrow(&mut deps, "eth", "escrow_eth");
        set_escrow_token_allowed(&mut deps, true);

        let user = deps.api.addr_make("user");
        let recipient = deps.api.addr_make("partner");
        // amount_in (1000) + ceil(1000 * 31/10000) (4) = 1004 funds attached;
        // the partner-fee check happens before fund checks but we attach valid
        // funds to ensure the cap check is what actually trips.
        let info = message_info(&user, &[cosmwasm_std::coin(1004, "ueth")]);

        let mut msg = ss_default_msg();
        if let ExecuteMsg::AddSingleSidedLiquidity {
            ref mut partner_fee,
            ..
        } = msg
        {
            *partner_fee = Some(PartnerFee {
                partner_fee_bps: 31,
                recipient: recipient.to_string(),
            });
        }
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::InvalidPartnerFee {});
    }

    /// partner_fee_bps = 0 → behaves as no fee: partner_fee_amount=0, recipient=info.sender.
    #[test]
    fn test_single_sided_partner_fee_zero_bps_acts_as_no_fee() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);
        seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");
        seed_escrow(&mut deps, "eth", "escrow_eth");
        set_escrow_token_allowed(&mut deps, true);

        let user = deps.api.addr_make("user");
        let recipient = deps.api.addr_make("partner");
        // 0 bps means full amount_in (1000) crosses, no extra needed for the fee.
        let info = message_info(&user, &[cosmwasm_std::coin(1000, "ueth")]);

        let mut msg = ss_default_msg();
        if let ExecuteMsg::AddSingleSidedLiquidity {
            ref mut partner_fee,
            ..
        } = msg
        {
            *partner_fee = Some(PartnerFee {
                partner_fee_bps: 0,
                recipient: recipient.to_string(),
            });
        }
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(res.messages.len(), 1);

        let entries: Vec<_> = PENDING_SINGLE_SIDED_LIQUIDITY
            .range(&deps.storage, None, None, cosmwasm_std::Order::Ascending)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(entries.len(), 1);
        let ((sender_addr, _tx_id), pending) = &entries[0];
        assert_eq!(sender_addr, &user);
        assert_eq!(pending.amount_in, Uint256::from(1000u128));
        assert_eq!(pending.partner_fee_amount, Uint256::zero());
        // 0-bps with Some(partner_fee) still validates the recipient.
        // The recipient is honored (does NOT default to info.sender).
        assert_eq!(pending.partner_fee_recipient, recipient);
    }

    /// MAX bps fee deposit: native funds must cover post-fee amount_in + partner_fee_amount,
    /// which equals the originally supplied amount_in.
    /// User supplies amount_in=1000, bps=30 → partner_fee_amount = ceil(1000 * 30/10000) = 3,
    /// post-fee amount_in = 997, full_deposit = 997 + 3 = 1000.
    /// Funds=999 fails (InsufficientFunds); funds=1000 succeeds.
    #[test]
    fn test_single_sided_partner_fee_native_funds_must_cover_full_deposit() {
        let recipient_str = {
            let deps = mock_dependencies();
            deps.api.addr_make("partner").to_string()
        };

        // Case A: funds short by 1 → InsufficientFunds.
        {
            let mut deps = mock_dependencies();
            init(&mut deps);
            set_native_supply(&mut deps);
            seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");
            seed_escrow(&mut deps, "eth", "escrow_eth");
            set_escrow_token_allowed(&mut deps, true);

            let user = deps.api.addr_make("user");
            let info = message_info(&user, &[cosmwasm_std::coin(999, "ueth")]);

            let mut msg = ss_default_msg();
            if let ExecuteMsg::AddSingleSidedLiquidity {
                ref mut partner_fee,
                ..
            } = msg
            {
                *partner_fee = Some(PartnerFee {
                    partner_fee_bps: 30,
                    recipient: recipient_str.clone(),
                });
            }
            let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
            assert_eq!(err, ContractError::InsufficientFunds {});
        }

        // Case B: exact full deposit (1000) attached → success.
        {
            let mut deps = mock_dependencies();
            init(&mut deps);
            set_native_supply(&mut deps);
            seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");
            seed_escrow(&mut deps, "eth", "escrow_eth");
            set_escrow_token_allowed(&mut deps, true);

            let user = deps.api.addr_make("user");
            let info = message_info(&user, &[cosmwasm_std::coin(1000, "ueth")]);

            let mut msg = ss_default_msg();
            if let ExecuteMsg::AddSingleSidedLiquidity {
                ref mut partner_fee,
                ..
            } = msg
            {
                *partner_fee = Some(PartnerFee {
                    partner_fee_bps: 30,
                    recipient: recipient_str.clone(),
                });
            }
            let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
            assert_eq!(res.messages.len(), 1);
        }
    }

    /// Pending state reflects post-fee deduction:
    /// User supplies amount_in=1000, bps=30 → stored amount_in=997, partner_fee_amount=3,
    /// partner_fee_recipient = validated recipient. Native funds attached = 1000 (the
    /// original amount_in, which equals post-fee amount_in + partner_fee_amount).
    #[test]
    fn test_single_sided_partner_fee_pending_state_reflects_deduction() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);
        seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");
        seed_escrow(&mut deps, "eth", "escrow_eth");
        set_escrow_token_allowed(&mut deps, true);

        let user = deps.api.addr_make("user");
        let recipient = deps.api.addr_make("partner");
        let info = message_info(&user, &[cosmwasm_std::coin(1000, "ueth")]);

        let mut msg = ss_default_msg();
        // swap_amount default in ss_default_msg is 500; new amount_in becomes 997
        // so 500 < 997 still holds.
        if let ExecuteMsg::AddSingleSidedLiquidity {
            ref mut partner_fee,
            ..
        } = msg
        {
            *partner_fee = Some(PartnerFee {
                partner_fee_bps: 30,
                recipient: recipient.to_string(),
            });
        }
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(res.messages.len(), 1);

        let entries: Vec<_> = PENDING_SINGLE_SIDED_LIQUIDITY
            .range(&deps.storage, None, None, cosmwasm_std::Order::Ascending)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(entries.len(), 1);
        let ((sender_addr, _tx_id), pending) = &entries[0];
        assert_eq!(sender_addr, &user);
        assert_eq!(pending.amount_in, Uint256::from(997u128));
        assert_eq!(pending.partner_fee_amount, Uint256::from(3u128));
        assert_eq!(pending.partner_fee_recipient, recipient);

        // amount_in attribute reflects post-fee amount.
        let attrs = &res.attributes;
        assert!(attrs
            .iter()
            .any(|a| a.key == "amount_in" && a.value == "997"));
    }

    /// partner_fee = None → partner_fee_amount=0 and recipient defaults to info.sender.
    #[test]
    fn test_single_sided_partner_fee_default_recipient_is_sender() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        set_native_supply(&mut deps);
        seed_vlp(&mut deps, "eth", "usdc", "vlp_addr");
        seed_escrow(&mut deps, "eth", "escrow_eth");
        set_escrow_token_allowed(&mut deps, true);

        let user = deps.api.addr_make("user");
        let info = message_info(&user, &[cosmwasm_std::coin(1000, "ueth")]);

        // ss_default_msg() uses partner_fee: None
        let msg = ss_default_msg();
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(res.messages.len(), 1);

        let entries: Vec<_> = PENDING_SINGLE_SIDED_LIQUIDITY
            .range(&deps.storage, None, None, cosmwasm_std::Order::Ascending)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(entries.len(), 1);
        let ((sender_addr, _tx_id), pending) = &entries[0];
        assert_eq!(sender_addr, &user);
        assert_eq!(pending.partner_fee_amount, Uint256::zero());
        assert_eq!(pending.partner_fee_recipient, user);
    }
}
