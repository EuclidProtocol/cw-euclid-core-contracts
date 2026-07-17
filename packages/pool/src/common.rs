use euclid::{
    admin::{self, EuclidAdmin},
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    error::ContractError,
    events::{liquidity_event, simple_event, tx_event, TxType},
    fee::MAX_FEE_BPS,
    msgs::vlp::base::{PoolCreationResponse, State, VlpRemoveLiquidityResponse},
    token::{Pair, Token},
};

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    ensure, to_json_binary, Decimal256, DepsMut, Env, MessageInfo, Response, Uint256, Uint64,
};
use cw_storage_plus::{Item, Map};

use crate::stable_math::MIN_AMP;

pub const MINIMUM_LIQUIDITY: u128 = 1_000_000_000; // 10^9

#[cw_serde]
pub struct SwapResult {
    pub return_amount: Uint256,
    pub spread_amount: Uint256,
}

#[cw_serde]
pub struct PreSwapResponse {
    pub lp_fee: Uint256,
    pub euclid_fee: Uint256,
    pub swap_amount: Uint256,
    pub receive_amount: Uint256,
    pub asset_out: Token,
    pub spread_amount: Uint256,
}

// Function to assert slippage is tolerated during transaction
pub fn assert_slippage_tolerance(
    ratio: Decimal256,
    pool_ratio: Decimal256,
    slippage_tolerance_bps: u64,
) -> Result<bool, ContractError> {
    let slippage = ratio.abs_diff(pool_ratio).checked_div(pool_ratio)?;

    let slippage_tolerance = Decimal256::bps(slippage_tolerance_bps);
    ensure!(
        slippage.le(&slippage_tolerance),
        ContractError::LiquiditySlippageExceeded {
            expected: slippage,
            received: slippage_tolerance,
        }
    );
    Ok(true)
}

pub fn update_fee(
    deps: DepsMut,
    info: MessageInfo,
    state_storage: &Item<State>,
    admin_storage: &Item<EuclidAdmin>,
    lp_fee_bps: Option<u64>,
    euclid_fee_bps: Option<u64>,
    recipient: Option<CrossChainUser>,
) -> Result<Response, ContractError> {
    let mut state = state_storage.load(deps.storage)?;
    let admin = admin_storage.load(deps.storage)?;
    ensure!(
        info.sender == admin.fee_admin,
        ContractError::Unauthorized {}
    );

    state.fee.lp_fee_bps = lp_fee_bps.unwrap_or(state.fee.lp_fee_bps);
    ensure!(
        state.fee.lp_fee_bps <= MAX_FEE_BPS,
        ContractError::new("LP Fee cannot exceed maximum limit")
    );

    state.fee.euclid_fee_bps = euclid_fee_bps.unwrap_or(state.fee.euclid_fee_bps);
    ensure!(
        state.fee.euclid_fee_bps <= MAX_FEE_BPS,
        ContractError::new("Euclid Fee cannot exceed maximum limit")
    );

    state.fee.recipient = recipient.unwrap_or(state.fee.recipient);

    state_storage.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("action", "update_fee")
        .add_attribute("lp_fee_bps", state.fee.lp_fee_bps.to_string())
        .add_attribute("euclid_fee_bps", state.fee.euclid_fee_bps.to_string())
        .add_attribute("recipient", state.fee.recipient.to_sender_string())
        .add_event(simple_event()))
}

pub fn update_amp_factor(
    deps: DepsMut,
    info: MessageInfo,
    admin_storage: &Item<EuclidAdmin>,
    amp_factor_storage: &Item<Uint64>,
    amp_factor: Uint64,
) -> Result<Response, ContractError> {
    let admin = admin_storage.load(deps.storage)?;
    ensure!(
        info.sender == admin.general_admin,
        ContractError::Unauthorized {}
    );
    ensure!(
        amp_factor.u64() >= MIN_AMP,
        ContractError::new(&format!("Amp factor must be at least {MIN_AMP}"))
    );
    amp_factor_storage.save(deps.storage, &amp_factor)?;
    Ok(Response::new()
        .add_attribute("action", "update_amp_factor")
        .add_attribute("amp_factor", amp_factor.to_string())
        .add_event(simple_event()))
}

pub fn update_admin(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    admin_storage: &Item<EuclidAdmin>,
    admin: String,
    admin_type: admin::AdminType,
) -> Result<Response, ContractError> {
    let current_admin = admin_storage.load(deps.storage)?;

    let (updated_admins, response) =
        admin::update_admin(&current_admin, &deps, &env, &info.sender, admin, admin_type)?;
    admin_storage.save(deps.storage, &updated_admins)?;

    Ok(response)
}

/// Registers a pool for a chain on the VLP.
///
/// Pool-type-specific event attributes (`pool_type`, `amp_factor`, `fee_tier_bps`,
/// `tick_spacing`) are added by the calling VLP after this returns — keeping
/// this function free of pool-type branching.
#[allow(clippy::too_many_arguments)]
pub fn register_pool(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    state_storage: &Item<State>,
    chain_lp_tokens: &Map<ChainUid, Uint256>,
    sender: CrossChainUser,
    pair: Pair,
    tx_id: String,
) -> Result<Response, ContractError> {
    let state = state_storage.load(deps.storage)?;

    ensure!(info.sender == state.router, ContractError::Unauthorized {});

    // Verify that chain pool does not already exist
    ensure!(
        !chain_lp_tokens.has(deps.storage, sender.chain_uid.clone()),
        ContractError::PoolAlreadyExists {}
    );

    // Check for token id
    ensure!(
        state.pair.get_tupple() == pair.get_tupple(),
        ContractError::AssetDoesNotExist {}
    );

    // Store the pool in the map
    chain_lp_tokens.save(deps.storage, sender.chain_uid.clone(), &Uint256::zero())?;

    let ack = PoolCreationResponse {
        vlp_contract: env.contract.address.to_string(),
        tx_id: tx_id.clone(),
        mint_lp_tokens: Uint256::zero(),
        sender: sender.clone(),
    };

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            &sender.to_sender_string(),
            TxType::PoolCreation,
        ))
        .add_attribute("action", "register_pool")
        .add_attribute("pool_chain", sender.chain_uid.to_string())
        .set_data(to_json_binary(&ack)?))
}

#[allow(clippy::too_many_arguments)]
pub fn remove_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    state_storage: &Item<State>,
    balances_storage: &Map<Token, Uint256>,
    chain_lp_tokens_storage: &Map<ChainUid, Uint256>,
    sender: CrossChainUser,
    lp_allocation: Uint256,
    tx_id: String,
) -> Result<Response, ContractError> {
    // Get the pool for the chain_id provided
    let mut state = state_storage.load(deps.storage)?;
    ensure!(info.sender == state.router, ContractError::Unauthorized {});
    let pair = state.pair.clone();

    let mut total_reserve_1 = balances_storage.load(deps.storage, pair.token_1.clone())?;
    let mut total_reserve_2 = balances_storage.load(deps.storage, pair.token_2.clone())?;

    // Remove chain lp tokens from the sender, remove liquidity only works for a single chain remove liquidity
    let mut chain_lp_tokens =
        chain_lp_tokens_storage.load(deps.storage, sender.chain_uid.clone())?;
    chain_lp_tokens = chain_lp_tokens.checked_sub(lp_allocation)?;
    chain_lp_tokens_storage.save(deps.storage, sender.chain_uid.clone(), &chain_lp_tokens)?;

    // Fetch allocated liquidity to LP tokens
    let lp_tokens = state.total_lp_tokens;
    let lp_share = Decimal256::checked_from_ratio(lp_allocation, lp_tokens)
        .map_err(|err| ContractError::new(&err.to_string()))?;

    // Calculate tokens_1 to send
    let token_1_liquidity = total_reserve_1.checked_mul_floor(lp_share)?;
    // Calculate tokens_2 to send
    let token_2_liquidity = total_reserve_2.checked_mul_floor(lp_share)?;

    let liquidity_released = pair.get_pair_with_amount(token_1_liquidity, token_2_liquidity)?;

    total_reserve_1 = total_reserve_1.checked_sub(token_1_liquidity)?;
    total_reserve_2 = total_reserve_2.checked_sub(token_2_liquidity)?;

    balances_storage.save(deps.storage, pair.token_1.clone(), &total_reserve_1)?;
    balances_storage.save(deps.storage, pair.token_2.clone(), &total_reserve_2)?;

    state.total_lp_tokens = state.total_lp_tokens.checked_sub(lp_allocation)?;
    state_storage.save(deps.storage, &state)?;

    // Prepare Liquidity Response
    let liquidity_response = VlpRemoveLiquidityResponse {
        burn_lp_tokens: lp_allocation,
        tx_id: tx_id.clone(),
        sender: sender.clone(),
        vlp_address: env.contract.address.to_string(),
        liquidity_released: liquidity_released.clone(),
    };

    // Prepare acknowledgement
    let acknowledgement = to_json_binary(&liquidity_response)?;

    let token_1_transfer_msg = pair.token_1.create_voucher_transfer_msg(
        state.virtual_balance_contract.to_string(),
        token_1_liquidity,
        None,
        sender.clone(),
        None,
        None,
    )?;

    let token_2_transfer_msg = pair.token_2.create_voucher_transfer_msg(
        state.virtual_balance_contract.to_string(),
        token_2_liquidity,
        None,
        sender.clone(),
        None,
        None,
    )?;

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            &sender.to_sender_string(),
            TxType::RemoveLiquidity,
        ))
        .add_message(token_1_transfer_msg)
        .add_message(token_2_transfer_msg)
        .add_event(liquidity_event(
            &pair
                .get_pair_with_amount(total_reserve_1, total_reserve_2)?
                .get_vec_token(),
            &liquidity_released.get_vec_token(),
            &tx_id,
        ))
        .add_attribute("action", "remove_liquidity")
        .add_attribute("sender", sender.to_sender_string())
        .add_attribute("token_1_removed_liquidity", token_1_liquidity)
        .add_attribute("token_2_removed_liquidity", token_2_liquidity)
        .add_attribute("burn_lp", lp_allocation)
        .set_data(acknowledgement))
}

pub fn calculate_amount_from_shares(
    reserve: Uint256,
    shares: Uint256,
    total_shares: Uint256,
) -> Result<Uint256, ContractError> {
    let amount = reserve.checked_multiply_ratio(shares, total_shares)?;
    Ok(amount)
}
