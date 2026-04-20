use cosmwasm_std::{ensure, Decimal, DepsMut, Env, MessageInfo, Response, Uint128};
use euclid::{
    cross_chain_user::CrossChainUser,
    error::ContractError,
    events::{simple_event, swap_event, tx_event},
    fee::{PartnerFee, MAX_PARTNER_FEE_BPS},
    msgs::cross_chain_config::CrossChainConfig,
    msgs::vlp::base::PoolType,
    recipient::Recipient,
    swap::{NextSwapPair, SwapRequest},
    token::{Pair, Token, TokenType, TokenWithDenom},
    utils::{fund_manager::FundManager, tx::generate_tx},
};
use euclid_ibc::router_ibc::{RouterCrossChainExecuteMsg, RouterCrossChainSwapExecuteMsg};

use crate::{
    query::get_chain_type,
    state::{PENDING_SWAPS, POOL_KEY_TO_VLP, STATE, TOKEN_TO_ESCROW},
};

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
        TokenType::Native { denom } => {
            // Verify thatthe amount of funds passed is greater than the asset amount
            fund_manager.use_fund(amount_in, denom)?;
        }
        TokenType::Smart { contract_address } => {
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

    for swap in &swaps {
        if let Some(pool_key) = &swap.pool_key {
            ensure!(
                matches!(pool_key.pool_type, PoolType::Concentrated { .. }),
                ContractError::new("swap hop pool_key must be concentrated")
            );
            let hop_pair = Pair::new(swap.token_in.clone(), swap.token_out.clone())?;
            ensure!(
                hop_pair.get_tupple() == pool_key.pair.get_tupple(),
                ContractError::new("swap hop tokens do not match pool_key pair")
            );
        }
    }

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
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "execute_request_swap")
        .add_submessage(swap_msg))
}
