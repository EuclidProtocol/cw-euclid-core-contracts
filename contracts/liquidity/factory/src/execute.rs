use cosmwasm_std::{
    ensure, from_json, to_json_binary, Binary, CosmosMsg, Decimal, DepsMut, Env, IbcTimeout,
    MessageInfo, Response, SubMsg, Uint128, WasmMsg,
};
use cw20::{Cw20ReceiveMsg, Logo};
use euclid::{
    chain::{CrossChainUser, CrossChainUserWithLimit},
    deposit::DepositTokenRequest,
    error::ContractError,
    events::{deposit_token_event, swap_event, tx_event, TxType},
    fee::{PartnerFee, BPS_100_PERCENT, MAX_PARTNER_FEE_BPS},
    liquidity::{AddLiquidityRequest, RemoveLiquidityRequest},
    msgs::{
        escrow::{AllowedTokenResponse, QueryMsg as EscrowQueryMsg},
        factory::cw20::FactoryCw20HookMsg,
    },
    pool::{EscrowCreateRequest, PoolCreateRequest},
    swap::{NextSwapPair, SwapRequest},
    timeout::get_timeout,
    token::{Pair, PairWithDenom, PairWithDenomAndAmount, Token, TokenType, TokenWithDenom},
    utils::{fund_manager::FundManager, tx::generate_tx},
};
use euclid_ibc::msg::{
    ChainIbcExecuteMsg, ChainIbcRemoveLiquidityExecuteMsg, ChainIbcTransferExecuteMsg,
    ChainIbcWithdrawExecuteMsg, HubIbcExecuteMsg,
};

use crate::{
    ibc::receive,
    state::{
        State, HUB_CHANNEL, PAIR_TO_VLP, PENDING_ADD_LIQUIDITY, PENDING_ESCROW_REQUESTS,
        PENDING_POOL_REQUESTS, PENDING_REMOVE_LIQUIDITY, PENDING_SWAPS, PENDING_TOKEN_DEPOSIT,
        STATE, TOKEN_TO_ESCROW, VLP_TO_CW20,
    },
};

pub fn execute_update_hub_channel(
    deps: DepsMut,
    info: MessageInfo,
    new_channel: String,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.admin, ContractError::Unauthorized {});
    let old_channel = HUB_CHANNEL.may_load(deps.storage)?;
    HUB_CHANNEL.save(deps.storage, &new_channel)?;
    let mut response = Response::new().add_attribute("method", "execute_update_hub_channel");
    if !new_channel.is_empty() {
        response = response.add_attribute("new_channel", new_channel);
    }
    Ok(response.add_attribute(
        "old_channel",
        old_channel.unwrap_or("no_old_channel".to_string()),
    ))
}

// Function to send IBC request to Router in VSL to create a new pool
pub fn execute_request_pool_creation(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    pair_with_denom: PairWithDenom,
    lp_token_name: String,
    lp_token_symbol: String,
    lp_token_decimal: u8,
    lp_token_marketing: Option<cw20_base::msg::InstantiateMarketingInfo>,
    timeout: Option<u64>,
) -> Result<Response, ContractError> {
    let pair = pair_with_denom.get_pair()?;
    let state = STATE.load(deps.storage)?;
    let sender = CrossChainUser {
        address: info.sender.to_string(),
        chain_uid: state.chain_uid.clone(),
    };
    let tx_id = generate_tx(deps.branch(), &env, &sender)?;

    ensure!(
        !PENDING_POOL_REQUESTS.has(deps.storage, (info.sender.clone(), tx_id.clone())),
        ContractError::TxAlreadyExist {}
    );
    ensure!(
        !PAIR_TO_VLP.has(deps.storage, pair.get_tupple()),
        ContractError::PoolAlreadyExists {}
    );
    let mut one_token_already_exists = false;
    let tokens = pair_with_denom.get_vec_token_info();
    for token in tokens {
        // Vouchers are not escrowed
        if token.token_type.is_voucher() {
            // If its a voucher token, then we can assume that one token already exists
            one_token_already_exists = true;
            continue;
        }

        // Ensure valid denom if token already exists
        let escrow_address = TOKEN_TO_ESCROW.may_load(deps.storage, token.clone().token)?;
        if let Some(escrow_address) = escrow_address {
            let token_allowed_query_msg = EscrowQueryMsg::TokenAllowed {
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
        }
    }

    ensure!(
        one_token_already_exists,
        ContractError::new(
            "Cannot create pool two new tokens. Atleast one token must already be registered."
        )
    );

    let channel = HUB_CHANNEL.load(deps.storage)?;
    let timeout = get_timeout(timeout)?;

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
        sender: info.sender.to_string(),
        pair_info: pair_with_denom.clone(),
        lp_token_instantiate_msg,
    };

    PENDING_POOL_REQUESTS.save(deps.storage, (info.sender.clone(), tx_id.clone()), &req)?;

    let pool_create_msg = ChainIbcExecuteMsg::RequestPoolCreation {
        pair: pair_with_denom,
        sender,
        tx_id: tx_id.clone(),
    }
    .to_msg(
        deps,
        &env,
        state.router_contract,
        state.chain_uid,
        state.is_native,
        channel.clone(),
        timeout,
    )?;

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            info.sender.as_str(),
            euclid::events::TxType::PoolCreation,
        ))
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "request_pool_creation")
        .add_submessage(pool_create_msg))
}

pub fn execute_request_register_escrow(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    token: TokenWithDenom,
    timeout: Option<u64>,
) -> Result<Response, ContractError> {
    // Vouchers are not escrowed
    ensure!(
        !token.token_type.is_voucher(),
        ContractError::UnsupportedDenomination {}
    );

    let state = STATE.load(deps.storage)?;
    ensure!(state.admin == info.sender, ContractError::Unauthorized {});

    let sender = CrossChainUser {
        address: info.sender.to_string(),
        chain_uid: state.chain_uid.clone(),
    };
    let tx_id = generate_tx(deps.branch(), &env, &sender)?;

    ensure!(
        !PENDING_ESCROW_REQUESTS.has(deps.storage, (info.sender.clone(), tx_id.clone())),
        ContractError::TxAlreadyExist {}
    );

    let escrow_address = TOKEN_TO_ESCROW.has(deps.storage, token.clone().token);
    ensure!(!escrow_address, ContractError::TokenAlreadyExist {});

    let channel = HUB_CHANNEL.load(deps.storage)?;
    let timeout = get_timeout(timeout)?;

    let register_escrow_msg = ChainIbcExecuteMsg::RequestEscrowCreation {
        token: token.clone().token,
        sender,
        tx_id: tx_id.clone(),
    }
    .to_msg(
        deps,
        &env,
        state.router_contract,
        state.chain_uid,
        state.is_native,
        channel,
        timeout,
    )?;

    let req = EscrowCreateRequest {
        tx_id: tx_id.clone(),
        sender: info.sender.to_string(),
        token,
    };

    PENDING_ESCROW_REQUESTS.save(deps.storage, (info.sender.clone(), tx_id.clone()), &req)?;

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            info.sender.as_str(),
            euclid::events::TxType::PoolCreation,
        ))
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "request_escrow_creation")
        .add_submessage(register_escrow_msg))
}

// Add liquidity to the pool
// TODO look into alternatives of using .branch(), maybe unifying the functions would help
pub fn add_liquidity_request(
    deps: &mut DepsMut,
    info: MessageInfo,
    env: Env,
    pair_info: PairWithDenomAndAmount,
    slippage_tolerance_bps: u64,
    timeout: Option<u64>,
) -> Result<Response, ContractError> {
    pair_info.validate()?;
    let pair = pair_info.get_pair()?;

    // Check that slippage tolerance is between 1 and 100
    let state = STATE.load(deps.storage)?;
    let sender = CrossChainUser {
        address: info.sender.to_string(),
        chain_uid: state.chain_uid.clone(),
    };
    let tx_id = generate_tx(deps.branch(), &env, &sender)?;

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
        ContractError::PoolDoesNotExists {}
    );

    let channel = HUB_CHANNEL.load(deps.storage)?;
    let timeout = get_timeout(timeout)?;

    // Prepare msg vector
    let mut msgs: Vec<CosmosMsg> = Vec::new();

    let mut fund_manager = FundManager::new(&info.funds);
    // Do an early check for tokens escrow so that if it exists, it should allow the denom that we are sending
    let tokens = pair_info.get_vec_token_info();
    for token in tokens {
        // Ensure liquidity is not zero
        ensure!(!token.amount.is_zero(), ContractError::ZeroAssetAmount {});

        // Vouchers are not escrowed
        if !token.token_type.is_voucher() {
            let escrow_address = TOKEN_TO_ESCROW
                .load(deps.storage, token.token)
                .or(Err(ContractError::EscrowDoesNotExist {}))?;
            let token_allowed_query_msg = EscrowQueryMsg::TokenAllowed {
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
                TokenType::Native { denom } => {
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
                    )?;
                    msgs.push(msg);
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

    let add_liq_msg = ChainIbcExecuteMsg::AddLiquidity {
        sender,
        slippage_tolerance_bps,
        pair: pair_info,
        tx_id: tx_id.clone(),
    }
    .to_msg(
        deps,
        &env,
        state.router_contract,
        state.chain_uid,
        state.is_native,
        channel,
        timeout,
    )?;

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            info.sender.as_str(),
            euclid::events::TxType::AddLiquidity,
        ))
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "add_liquidity_request")
        .add_messages(msgs)
        .add_submessage(add_liq_msg))
}

// Add liquidity to the pool
// TODO look into alternatives of using .branch(), maybe unifying the functions would help
pub fn remove_liquidity_request(
    deps: &mut DepsMut,
    info: MessageInfo,
    env: Env,
    sender: CrossChainUser,
    pair: Pair,
    lp_allocation: Uint128,
    timeout: Option<u64>,
    cross_chain_addresses: Vec<CrossChainUserWithLimit>,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    let sender_addr = deps.api.addr_validate(&sender.address)?;

    let tx_id = generate_tx(deps.branch(), &env, &sender)?;

    ensure!(
        !PENDING_REMOVE_LIQUIDITY.has(deps.storage, (sender_addr.clone(), tx_id.clone())),
        ContractError::TxAlreadyExist {}
    );

    let vlp = PAIR_TO_VLP.load(deps.storage, pair.get_tupple())?;
    let cw20 = VLP_TO_CW20.load(deps.storage, vlp)?;

    ensure!(cw20 == info.sender, ContractError::Unauthorized {});

    ensure!(
        PAIR_TO_VLP.has(deps.storage, pair.get_tupple()),
        ContractError::PoolDoesNotExists {}
    );
    // TODO: Do we want to add check for lp shares for early fail?

    let channel = HUB_CHANNEL.load(deps.storage)?;
    let timeout = get_timeout(timeout)?;

    // Check that the liquidity is greater than 0
    ensure!(!lp_allocation.is_zero(), ContractError::ZeroAssetAmount {});

    let liquidity_tx_info = RemoveLiquidityRequest {
        sender: sender_addr.to_string(),
        lp_allocation,
        pair: pair.clone(),
        tx_id: tx_id.clone(),
        cw20,
    };

    PENDING_REMOVE_LIQUIDITY.save(
        deps.storage,
        (sender_addr.clone(), tx_id.clone()),
        &liquidity_tx_info,
    )?;

    let remove_liq_msg = ChainIbcExecuteMsg::RemoveLiquidity(ChainIbcRemoveLiquidityExecuteMsg {
        sender,
        lp_allocation,
        pair,
        cross_chain_addresses,
        tx_id: tx_id.clone(),
    })
    .to_msg(
        deps,
        &env,
        state.router_contract,
        state.chain_uid,
        state.is_native,
        channel,
        timeout,
    )?;

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            sender_addr.as_str(),
            euclid::events::TxType::RemoveLiquidity,
        ))
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "remove_liquidity_request")
        .add_submessage(remove_liq_msg))
}

// TODO make execute_swap an internal function OR merge execute_swap_request and execute_swap into one function

pub fn execute_swap_request(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    sender: CrossChainUser,
    asset_in: TokenWithDenom,
    amount_in: Uint128,
    asset_out: Token,
    min_amount_out: Uint128,
    swaps: Vec<NextSwapPair>,
    timeout: Option<u64>,
    cross_chain_addresses: Vec<CrossChainUserWithLimit>,
    partner_fee: Option<PartnerFee>,
) -> Result<Response, ContractError> {
    // Validate asset in
    asset_in.validate(deps.as_ref())?;

    let state = STATE.load(deps.storage)?;
    let sender_addr = deps.api.addr_validate(&sender.address)?;

    let tx_id = generate_tx(deps.branch(), &env, &sender)?;

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
        TokenType::Native { denom } => {
            // Verify thatthe amount of funds passed is greater than the asset amount
            fund_manager.use_fund(amount_in, denom)?;
        }
        TokenType::Smart { contract_address } => {
            ensure!(
                info.sender == *contract_address,
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
        ContractError::new("Amount in doesn't match swap route")
    );

    let last_swap = swaps.last().ok_or(ContractError::Generic {
        err: "Empty Swap not allowed".to_string(),
    })?;

    ensure!(
        last_swap.token_out == asset_out,
        ContractError::new("Amount out doesn't match swap route")
    );

    let channel = HUB_CHANNEL.load(deps.storage)?;
    let timeout = get_timeout(timeout)?;

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
        timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(timeout)),
        tx_id: tx_id.clone(),
        cross_chain_addresses: cross_chain_addresses.clone(),
        partner_fee_amount,
        partner_fee_recipient: partner_fee_recipient.clone(),
    };
    PENDING_SWAPS.save(
        deps.storage,
        (sender_addr.clone(), tx_id.clone()),
        &swap_info,
    )?;

    let swap_msg = ChainIbcExecuteMsg::Swap(euclid_ibc::msg::ChainIbcSwapExecuteMsg {
        sender,
        asset_in,
        amount_in,
        asset_out,
        min_amount_out,
        swaps,
        tx_id: tx_id.clone(),
        cross_chain_addresses,
        partner_fee_recipient: CrossChainUser {
            address: partner_fee_recipient.to_string(),
            chain_uid: state.chain_uid.clone(),
        },
        partner_fee_amount,
    })
    .to_msg(
        deps,
        &env,
        state.router_contract.clone(),
        state.chain_uid.clone(),
        state.is_native,
        channel,
        timeout,
    )?;

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            sender_addr.as_str(),
            euclid::events::TxType::Swap,
        ))
        .add_event(swap_event(&tx_id, &swap_info))
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "execute_request_swap")
        .add_submessage(swap_msg))
}

pub fn execute_deposit_token(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    sender: CrossChainUser,
    asset_in: TokenWithDenom,
    amount_in: Uint128,
    timeout: Option<u64>,
    recipient: Option<CrossChainUser>,
) -> Result<Response, ContractError> {
    ensure!(
        !asset_in.token_type.is_voucher(),
        ContractError::UnsupportedDenomination {}
    );

    let state = STATE.load(deps.storage)?;

    let sender_addr = deps.api.addr_validate(&sender.address)?;
    let recipient = recipient.unwrap_or(sender.clone());

    // Validate asset in
    asset_in.validate(deps.as_ref())?;

    let tx_id = generate_tx(deps.branch(), &env, &sender)?;
    let channel = HUB_CHANNEL.load(deps.storage)?;

    let timeout = get_timeout(timeout)?;

    ensure!(
        !PENDING_TOKEN_DEPOSIT.has(deps.storage, (sender_addr.clone(), tx_id.clone())),
        ContractError::TxAlreadyExist {}
    );
    // Verify that this asset is allowed
    let escrow = TOKEN_TO_ESCROW.load(deps.storage, asset_in.token.clone())?;

    let token_allowed: euclid::msgs::escrow::AllowedTokenResponse = deps.querier.query_wasm_smart(
        escrow,
        &euclid::msgs::escrow::QueryMsg::TokenAllowed {
            denom: asset_in.token_type.clone(),
        },
    )?;
    ensure!(
        token_allowed.allowed,
        ContractError::UnsupportedDenomination {}
    );
    let mut fund_manager = FundManager::new(&info.funds);

    match &asset_in.token_type {
        TokenType::Native { denom } => {
            fund_manager.use_fund(amount_in, denom)?;
        }
        TokenType::Smart { contract_address } => {
            ensure!(
                info.sender == *contract_address,
                ContractError::Unauthorized {}
            );
        }
        TokenType::Voucher { .. } => {}
    }

    ensure!(
        fund_manager.validate_funds_are_empty().is_ok(),
        ContractError::new("Extra funds sent with message")
    );

    let deposit_token_info = DepositTokenRequest {
        sender: sender_addr.to_string(),
        asset_in: asset_in.clone(),
        amount_in,
        timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(timeout)),
        tx_id: tx_id.clone(),
        recipient: recipient.clone(),
    };

    PENDING_TOKEN_DEPOSIT.save(
        deps.storage,
        (sender_addr.clone(), tx_id.clone()),
        &deposit_token_info,
    )?;

    let deposit_token_msg =
        ChainIbcExecuteMsg::DepositToken(euclid_ibc::msg::ChainIbcDepositTokenExecuteMsg {
            sender,
            asset_in: asset_in.token,
            amount_in,
            tx_id: tx_id.clone(),
            recipient,
        })
        .to_msg(
            deps,
            &env,
            state.clone().router_contract,
            state.clone().chain_uid,
            state.is_native,
            channel,
            timeout,
        )?;

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            sender_addr.as_str(),
            euclid::events::TxType::DepositToken,
        ))
        .add_event(deposit_token_event(&tx_id, &deposit_token_info))
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "execute_deposit_token")
        .add_submessage(deposit_token_msg))
}

/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
///
/// * **cw20_msg** is the CW20 message that has to be processed.
pub fn receive_cw20(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;

    let sender = CrossChainUser {
        address: cw20_msg.sender,
        chain_uid: state.chain_uid,
    };

    match from_json(&cw20_msg.msg)? {
        // Allow to swap using a CW20 hook message
        FactoryCw20HookMsg::Swap {
            asset_in,
            asset_out,
            min_amount_out,
            timeout,
            swaps,
            cross_chain_addresses,
            partner_fee,
        } => {
            let contract_adr = info.sender.clone();

            // ensure that contract address is same as asset being swapped
            ensure!(
                contract_adr == asset_in.token_type.get_smart_contract_address()?,
                ContractError::AssetDoesNotExist {}
            );

            let amount_in = cw20_msg.amount;

            // ensure that the contract address is the same as the asset contract address
            execute_swap_request(
                &mut deps,
                env,
                info,
                sender,
                asset_in,
                amount_in,
                asset_out,
                min_amount_out,
                swaps,
                timeout,
                cross_chain_addresses,
                partner_fee,
            )
        }
        FactoryCw20HookMsg::RemoveLiquidity {
            pair,
            lp_allocation,
            timeout,
            cross_chain_addresses,
        } => remove_liquidity_request(
            &mut deps,
            info,
            env,
            sender,
            pair,
            lp_allocation,
            timeout,
            cross_chain_addresses,
        ),
        FactoryCw20HookMsg::Deposit {
            token,
            recipient,
            timeout,
        } => {
            let contract_adr = info.sender.clone();

            let asset_in = token.with_type(TokenType::Smart {
                contract_address: contract_adr.to_string(),
            });
            let amount_in = cw20_msg.amount;

            // ensure that the contract address is the same as the asset contract address
            execute_deposit_token(
                &mut deps, env, info, sender, asset_in, amount_in, timeout, recipient,
            )
        }
    }
}

// New factory functions //
pub fn execute_request_register_denom(
    deps: DepsMut,
    info: MessageInfo,
    token: TokenWithDenom,
) -> Result<Response, ContractError> {
    let admin = STATE.load(deps.storage)?.admin;
    ensure!(
        admin == info.sender.into_string(),
        ContractError::Unauthorized {}
    );

    let escrow_address = TOKEN_TO_ESCROW
        .load(deps.storage, token.token.clone())
        .map_err(|_err| ContractError::EscrowDoesNotExist {})?;

    let msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: escrow_address.into_string(),
        msg: to_json_binary(&euclid::msgs::escrow::ExecuteMsg::AddAllowedDenom {
            denom: token.token_type.clone(),
        })?,
        funds: vec![],
    });
    Ok(Response::new()
        .add_submessage(SubMsg::new(msg))
        .add_attribute("method", "request_add_allowed_denom")
        .add_attribute("token", token.token.to_string())
        .add_attribute("denom", token.token_type.get_key()))
}

pub fn execute_request_deregister_denom(
    deps: DepsMut,
    info: MessageInfo,
    token: TokenWithDenom,
) -> Result<Response, ContractError> {
    let admin = STATE.load(deps.storage)?.admin;
    ensure!(
        admin == info.sender.into_string(),
        ContractError::Unauthorized {}
    );

    let escrow_address = TOKEN_TO_ESCROW
        .load(deps.storage, token.token.clone())
        .map_err(|_err| ContractError::EscrowDoesNotExist {})?;

    let msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: escrow_address.into_string(),
        msg: to_json_binary(&euclid::msgs::escrow::ExecuteMsg::DisallowDenom {
            denom: token.token_type.clone(),
        })?,
        funds: vec![],
    });
    Ok(Response::new()
        .add_submessage(SubMsg::new(msg))
        .add_attribute("method", "request_disallow_denom")
        .add_attribute("token", token.token.to_string())
        .add_attribute("denom", token.token_type.get_key()))
}

pub fn execute_withdraw_virtual_balance(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    token: Token,
    amount: Uint128,
    cross_chain_addresses: Vec<CrossChainUserWithLimit>,
    timeout: Option<u64>,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;

    let channel = HUB_CHANNEL.load(deps.storage)?;
    let sender = CrossChainUser {
        address: info.sender.to_string(),
        chain_uid: state.chain_uid.clone(),
    };
    let tx_id = generate_tx(deps.branch(), &env, &sender)?;
    let timeout = get_timeout(timeout)?;

    let withdraw_msg = ChainIbcExecuteMsg::Withdraw(ChainIbcWithdrawExecuteMsg {
        sender,
        token,
        amount,
        cross_chain_addresses,
        tx_id: tx_id.clone(),
        timeout: Some(timeout),
    })
    .to_msg(
        deps,
        &env,
        state.router_contract,
        state.chain_uid,
        state.is_native,
        channel,
        timeout,
    )?;

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            info.sender.as_str(),
            TxType::WithdrawVirtualBalance,
        ))
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "withdraw_virtual_balance")
        .add_submessage(withdraw_msg))
}

pub fn execute_transfer_virtual_balance(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    token: Token,
    amount: Uint128,
    recipient_address: CrossChainUser,
    timeout: Option<u64>,
) -> Result<Response, ContractError> {
    // The transfer amount should be greater than zero
    ensure!(!amount.is_zero(), ContractError::ZeroAssetAmount {});

    // Validate recipient address
    recipient_address.validate()?;

    let state = STATE.load(deps.storage)?;

    let channel = HUB_CHANNEL.load(deps.storage)?;
    let sender = CrossChainUser {
        address: info.sender.to_string(),
        chain_uid: state.chain_uid.clone(),
    };
    let tx_id = generate_tx(deps.branch(), &env, &sender)?;
    let timeout = get_timeout(timeout)?;

    let withdraw_msg = ChainIbcExecuteMsg::Transfer(ChainIbcTransferExecuteMsg {
        sender,
        token,
        amount,
        recipient_address,
        tx_id: tx_id.clone(),
        timeout: Some(timeout),
    })
    .to_msg(
        deps,
        &env,
        state.router_contract,
        state.chain_uid,
        state.is_native,
        channel,
        timeout,
    )?;

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            info.sender.as_str(),
            TxType::TransferVirtualBalance,
        ))
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "withdraw_virtual_balance")
        .add_submessage(withdraw_msg))
}
pub fn execute_update_state(
    deps: DepsMut,
    info: MessageInfo,
    router_contract: Option<String>,
    admin: Option<String>,
    escrow_code_id: Option<u64>,
    cw20_code_id: Option<u64>,
    is_native: Option<bool>,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;

    ensure!(
        state.admin == info.sender.into_string(),
        ContractError::Unauthorized {}
    );

    let new_state = State {
        router_contract: router_contract.clone().unwrap_or(state.router_contract),
        admin: admin.clone().unwrap_or(state.admin),
        escrow_code_id: escrow_code_id.unwrap_or(state.escrow_code_id),
        cw20_code_id: cw20_code_id.unwrap_or(state.cw20_code_id),
        chain_uid: state.chain_uid,
        is_native: is_native.unwrap_or(state.is_native),
        partner_fees_collected: state.partner_fees_collected,
    };

    STATE.save(deps.storage, &new_state)?;

    Ok(Response::new()
        .add_attribute("method", "update_state")
        .add_attribute("admin", admin.unwrap_or("unchanged".to_string()))
        .add_attribute(
            "router_contract",
            router_contract.unwrap_or("unchanged".to_string()),
        )
        .add_attribute(
            "escrow_code_id",
            escrow_code_id.unwrap_or(state.escrow_code_id).to_string(),
        )
        .add_attribute(
            "cw20_code_id",
            cw20_code_id.map_or_else(|| "unchanged".to_string(), |x| x.to_string()),
        )
        .add_attribute(
            "is_native",
            is_native.map_or_else(|| "unchanged".to_string(), |x| x.to_string()),
        ))
}

pub fn execute_native_receive_callback(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: Binary,
) -> Result<Response, ContractError> {
    let msg: HubIbcExecuteMsg = from_json(msg)?;
    let state = STATE.load(deps.storage)?;

    // Only native chains can directly use this messages
    ensure!(state.is_native, ContractError::Unauthorized {});

    // Only router contract can execute this message
    ensure!(
        state.router_contract == info.sender,
        ContractError::Unauthorized {}
    );
    receive::reusable_internal_call(deps, env, msg)
}
