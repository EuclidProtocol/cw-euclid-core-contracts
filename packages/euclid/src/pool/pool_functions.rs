use crate::{
    chain::{ChainUid, CrossChainUser},
    error::ContractError,
    events::{liquidity_event, simple_event, tx_event, TxType},
    fee::{Fee, TotalFees, BPS_50_PERCENT},
    liquidity::AddLiquidityResponse,
    msgs::virtual_balance::{ExecuteApprove, ExecuteTransfer},
    pool::stable_math::compute_stable_swap,
    swap::NextSwapVlp,
    token::{Pair, PairWithAmount, PairWithDenomAndAmount, Token, TokenWithDenom},
    utils::math::Decimal256Ext,
};
pub const NEXT_SWAP_REPLY_ID: u64 = 2;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    ensure, to_json_binary, Decimal, Decimal256, Deps, DepsMut, Env, Isqrt, MessageInfo, Response,
    SubMsg, Uint128, Uint512, Uint64, WasmMsg,
};
use cw_storage_plus::{Item, Map};

pub const MINIMUM_LIQUIDITY: u128 = 1000;

// Request to create pool saved in state to manage during acknowledgement
#[cw_serde]
pub struct PoolCreateRequest {
    // Request sender
    pub sender: String,
    // Pool request id
    pub tx_id: String,
    // Pool Pair
    pub pair_info: PairWithDenomAndAmount,
    pub lp_token_instantiate_msg: cw20_base::msg::InstantiateMsg,
}

// Request to create pool saved in state to manage during acknowledgement
#[cw_serde]
pub struct PoolWithLiquidityCreateRequest {
    // Request sender
    pub sender: String,
    // Pool request id
    pub tx_id: String,
    // Pool Pair
    pub pair_info: PairWithDenomAndAmount,
    pub lp_token_instantiate_msg: cw20_base::msg::InstantiateMsg,
}

#[cw_serde]
/// Used for registering and deregistering denoms
pub struct DenomRequest {
    // Request sender
    pub sender: String,
    // Escrow request id
    pub tx_id: String,
    // Escrow Token
    pub token: TokenWithDenom,
}

#[cw_serde]
pub struct VlpSwapResponse {
    pub sender: CrossChainUser,
    pub tx_id: String,
    pub asset_out: Token,
    pub amount_out: Uint128,
}

// Struct to handle Acknowledgement Response for a Pool Creation Request
#[cw_serde]
pub struct PoolCreationResponse {
    pub vlp_contract: String,
    pub tx_id: String,
    pub mint_lp_tokens: Uint128,
    pub sender: CrossChainUser,
}

#[cw_serde]
pub struct RegisterDenomResponse {}

#[cw_serde]
pub struct DeRegisterDenomResponse {}

#[cw_serde]
pub enum PoolConfig {
    Stable { amp_factor: Option<Uint64> },
    ConstantProduct {},
}

#[cw_serde]
pub struct VlpRemoveLiquidityResponse {
    pub liquidity_released: PairWithAmount,
    pub burn_lp_tokens: Uint128,
    pub tx_id: String,
    pub sender: CrossChainUser,
    pub vlp_address: String,
}

#[cw_serde]
pub struct State {
    // Token Pair Info
    pub pair: Pair,
    // Router Contract
    pub router: String,
    // Virtual Coin Contract
    pub virtual_balance: String,
    // Fee per swap for each transaction
    pub fee: Fee,
    // Total lp and euclid fees collected
    pub total_fees_collected: TotalFees,
    // The last timestamp where the balances for each token have been updated
    pub last_updated: u64,
    // total number of LP tokens issued
    pub total_lp_tokens: Uint128,
    pub admin: String,
}

#[cw_serde]
pub struct SwapResult {
    pub return_amount: Uint128,
    pub spread_amount: Uint128,
}

// Function to calculate the asset to be recieved after a swap
pub fn calculate_cp_swap(
    swap_amount: Uint128,
    reserve_in: Uint128,
    reserve_out: Uint128,
) -> Result<SwapResult, ContractError> {
    let reserve_in = Uint512::from(reserve_in);
    let reserve_out = Uint512::from(reserve_out);
    // Calculate the k constant product
    let k = reserve_in.checked_mul(reserve_out)?;
    // Calculate the new reserve of token 1
    let new_reserve_in = reserve_in.checked_add(swap_amount.into())?;
    // Calculate the new reserve of token 2
    let new_reserve_out = k.checked_div(new_reserve_in)?;

    // Calculate the amount of token 2 to be recieved
    let token_2_recieved = reserve_out.checked_sub(new_reserve_out)?;
    let mut token_2_recieved =
        Uint128::try_from(token_2_recieved).map_err(|_| ContractError::new("Overflow"))?;

    let ideal_return_amount = reserve_out
        .checked_mul(swap_amount.into())?
        .checked_div(reserve_in)?;
    let ideal_return_amount =
        Uint128::try_from(ideal_return_amount).map_err(|_| ContractError::new("Overflow"))?;

    if ideal_return_amount < token_2_recieved {
        // If ideal return amount is less than actual return amount, then set the spread amount to 0 and return the ideal return amount as the return amount
        // This is to prevent the spread amount from being negative which was caused due to precision loss in the calculation
        token_2_recieved = ideal_return_amount;
    }
    let spread_amount = ideal_return_amount.checked_sub(token_2_recieved)?;

    Ok(SwapResult {
        return_amount: token_2_recieved,
        spread_amount,
    })
}

pub fn calculate_lp_allocation(
    token_1_amount: Uint128,
    token_2_amount: Uint128,
    total_liquidity_1: Uint128,
    total_liquidity_2: Uint128,
    total_lp_supply: Uint128,
) -> Result<Uint128, ContractError> {
    let token_1_amount = Uint512::from(token_1_amount);
    let token_2_amount = Uint512::from(token_2_amount);
    let total_liquidity_1 = Uint512::from(total_liquidity_1);
    let total_liquidity_2 = Uint512::from(total_liquidity_2);
    let total_lp_supply = Uint512::from(total_lp_supply);

    // IF LP supply is 0 use original function
    if total_lp_supply.is_zero() {
        let sq_root = Uint128::try_from(Isqrt::isqrt(token_1_amount.checked_mul(token_2_amount)?))
            .map_err(|_| ContractError::new("Overflow total supply"))?;
        return Ok(sq_root);
    }

    let lp_allocation = token_1_amount
        .checked_mul(total_lp_supply)?
        .checked_div(total_liquidity_1)?
        .min(
            token_2_amount
                .checked_mul(total_lp_supply)?
                .checked_div(total_liquidity_2)?,
        );

    Uint128::try_from(lp_allocation).map_err(|_| ContractError::new("Overflow lp allocation"))
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
    state_storage: &Item<State>, // Pass as a reference
    lp_fee_bps: Option<u64>,
    euclid_fee_bps: Option<u64>,
    recipient: Option<CrossChainUser>,
) -> Result<Response, ContractError> {
    let mut state = state_storage.load(deps.storage)?;
    ensure!(
        info.sender.as_str() == state.admin,
        ContractError::Unauthorized {}
    );

    state.fee.lp_fee_bps = lp_fee_bps.unwrap_or(state.fee.lp_fee_bps);
    state.fee.euclid_fee_bps = euclid_fee_bps.unwrap_or(state.fee.euclid_fee_bps);

    state.fee.validate()?;

    state.fee.recipient = recipient.unwrap_or(state.fee.recipient);

    state_storage.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("action", "update_fee")
        .add_event(simple_event()))
}

#[allow(clippy::too_many_arguments)]
pub fn update_state(
    deps: DepsMut,
    info: MessageInfo,
    state_storage: &Item<State>,               // Reference to STATE
    amp_factor_storage: Option<&Item<Uint64>>, // Optional reference to AMP_FACTOR
    router: Option<String>,
    virtual_balance: Option<String>,
    fee: Option<Fee>,
    last_updated: Option<u64>,
    admin: Option<String>,
    amp_factor: Option<Uint64>,
) -> Result<Response, ContractError> {
    let state = state_storage.load(deps.storage)?;
    ensure!(
        info.sender.as_str() == state.admin,
        ContractError::Unauthorized {}
    );

    // Validate and update router address
    let verified_router = if let Some(router) = router {
        deps.api.addr_validate(&router)?;
        router
    } else {
        state.router
    };

    // Validate and update virtual balance address
    let verified_virtual_balance = if let Some(virtual_balance) = virtual_balance {
        deps.api.addr_validate(&virtual_balance)?;
        virtual_balance
    } else {
        state.virtual_balance
    };

    // Validate and update admin address
    let verified_admin = if let Some(admin) = admin {
        deps.api.addr_validate(&admin)?;
        admin
    } else {
        state.admin
    };

    let new_state = State {
        pair: state.pair,
        router: verified_router,
        virtual_balance: verified_virtual_balance,
        fee: fee.unwrap_or(state.fee),
        total_fees_collected: state.total_fees_collected,
        last_updated: last_updated.unwrap_or(state.last_updated),
        total_lp_tokens: state.total_lp_tokens,
        admin: verified_admin,
    };

    state_storage.save(deps.storage, &new_state)?;

    let mut response = Response::new().add_attribute("action", "update_state");
    if let (Some(amp_factor), Some(storage)) = (amp_factor, amp_factor_storage) {
        storage.save(deps.storage, &amp_factor)?;
        response = response.add_attribute("amp_factor_updated", amp_factor.to_string());
    }

    Ok(response)
}

#[allow(clippy::too_many_arguments)]
pub fn register_pool(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    state_storage: &Item<State>,
    chain_lp_tokens: &Map<ChainUid, Uint128>,
    amp_factor: Option<Uint64>,
    sender: CrossChainUser,
    pair: Pair,
    tx_id: String,
) -> Result<Response, ContractError> {
    let state = state_storage.load(deps.storage)?;

    ensure!(
        info.sender.as_str() == state.router,
        ContractError::Unauthorized {}
    );

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
    chain_lp_tokens.save(deps.storage, sender.chain_uid.clone(), &Uint128::zero())?;

    let ack = PoolCreationResponse {
        vlp_contract: env.contract.address.to_string(),
        tx_id: tx_id.clone(),
        mint_lp_tokens: Uint128::zero(),
        sender: sender.clone(),
    };
    let pool_type = if amp_factor.is_some() {
        "stable"
    } else {
        "constant_product"
    };

    let mut response = Response::new()
        .add_event(tx_event(
            &tx_id,
            &sender.to_sender_string(),
            TxType::PoolCreation,
        ))
        .add_attribute("action", "register_pool")
        .add_attribute("pool_chain", sender.chain_uid.to_string())
        .add_attribute("pool_type", pool_type)
        .set_data(to_json_binary(&ack)?);

    if let Some(amp_factor) = amp_factor {
        response = response.add_attribute("amp_factor", amp_factor.to_string());
    }

    Ok(response)
}

#[allow(clippy::too_many_arguments)]
pub fn remove_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    state_storage: &Item<State>,
    balances_storage: &Map<Token, Uint128>,
    chain_lp_tokens_storage: &Map<ChainUid, Uint128>,
    sender: CrossChainUser,
    lp_allocation: Uint128,
    tx_id: String,
) -> Result<Response, ContractError> {
    // Get the pool for the chain_id provided
    let mut state = state_storage.load(deps.storage)?;
    ensure!(
        info.sender.as_str() == state.router,
        ContractError::Unauthorized {}
    );
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
    let lp_share = Decimal::checked_from_ratio(lp_allocation, lp_tokens)
        .map_err(|err| ContractError::new(&err.to_string()))?;

    // Calculate tokens_1 to send
    let token_1_liquidity = total_reserve_1.checked_mul_ceil(lp_share)?;
    // Calculate tokens_2 to send
    let token_2_liquidity = total_reserve_2.checked_mul_ceil(lp_share)?;

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

    let token_1_transfer_msg = pair.token_1.create_virtual_balance_transfer_msg(
        state.virtual_balance.clone(),
        token_1_liquidity,
        None,
        sender.clone(),
        None,
        None,
    )?;

    let token_2_transfer_msg = pair.token_2.create_virtual_balance_transfer_msg(
        state.virtual_balance,
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

#[allow(clippy::too_many_arguments)]
pub fn add_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    state_storage: &Item<State>,
    balances_storage: &Map<Token, Uint128>,
    chain_lp_tokens_storage: &Map<ChainUid, Uint128>,
    collateral_lp_tokens_storage: &Item<Uint128>,
    sender: CrossChainUser,
    liquidity: PairWithAmount,
    slippage_tolerance_bps: u64,
    tx_id: String,
) -> Result<Response, ContractError> {
    let mut state = state_storage.load(deps.storage)?;
    ensure!(
        info.sender.as_str() == state.router,
        ContractError::Unauthorized {}
    );
    let mut response = Response::new();

    // Ensure tokens are received by VLP
    for token in liquidity.get_vec_token() {
        // Contract should have approval to use voucher tokens on behalf of sender
        let virtual_balance_transfer_msg = token.token.create_virtual_balance_transfer_msg(
            state.virtual_balance.clone(),
            token.amount,
            None,
            CrossChainUser {
                address: env.contract.address.to_string(),
                chain_uid: ChainUid::vsl_chain_uid()?,
            },
            // We have approval to use voucher tokens on behalf of sender
            Some(sender.clone()),
            None,
        )?;
        response = response.add_message(virtual_balance_transfer_msg);
    }

    let mut chain_lp_tokens =
        chain_lp_tokens_storage.load(deps.storage, sender.chain_uid.clone())?;

    let pair = state.pair.clone();

    let token_1_liquidity = if liquidity.token_1.token == pair.token_1 {
        liquidity.token_1.amount
    } else {
        liquidity.token_2.amount
    };

    let token_2_liquidity = if liquidity.token_2.token == pair.token_2 {
        liquidity.token_2.amount
    } else {
        liquidity.token_1.amount
    };

    // Verify that ratio of assets provided is equal to the ratio of assets in the pool
    let ratio =
        Decimal256::checked_from_ratio(token_1_liquidity, token_2_liquidity).map_err(|err| {
            ContractError::Generic {
                err: err.to_string(),
            }
        })?;

    let mut total_reserve_1 = balances_storage.load(deps.storage, pair.token_1.clone())?;
    let mut total_reserve_2 = balances_storage.load(deps.storage, pair.token_2.clone())?;

    // Lets get lq ratio, it will be the current ratio of token reserves or if its first time then it will be ratio of tokens provided
    let lq_ratio =
        Decimal256::checked_from_ratio(total_reserve_1, total_reserve_2).unwrap_or(ratio);

    // Verify slippage tolerance is between 0 and 50
    ensure!(
        slippage_tolerance_bps.le(&BPS_50_PERCENT),
        ContractError::InvalidSlippageTolerance {}
    );

    assert_slippage_tolerance(ratio, lq_ratio, slippage_tolerance_bps)?;

    //TODO Change calculate_lp_allocation to use stable swap formula
    // Calculate liquidity added share for LP provider from total liquidity
    let lp_allocation = calculate_lp_allocation(
        token_1_liquidity,
        token_2_liquidity,
        total_reserve_1,
        total_reserve_2,
        state.total_lp_tokens,
    )?;

    let is_new_pool = state.total_lp_tokens.is_zero();
    state.total_lp_tokens = state.total_lp_tokens.checked_add(lp_allocation)?;

    let lp_allocation = if is_new_pool {
        collateral_lp_tokens_storage.save(deps.storage, &Uint128::from(MINIMUM_LIQUIDITY))?;
        lp_allocation.checked_sub(Uint128::from(MINIMUM_LIQUIDITY))?
    } else {
        lp_allocation
    };

    ensure!(
        !lp_allocation.is_zero(),
        ContractError::Generic {
            err: "LP Allocation cannot be zero".to_string()
        }
    );

    chain_lp_tokens = chain_lp_tokens.checked_add(lp_allocation)?;
    chain_lp_tokens_storage.save(deps.storage, sender.chain_uid.clone(), &chain_lp_tokens)?;

    // Add to total liquidity and total lp allocation
    total_reserve_1 = total_reserve_1.checked_add(token_1_liquidity)?;
    total_reserve_2 = total_reserve_2.checked_add(token_2_liquidity)?;

    state_storage.save(deps.storage, &state)?;

    balances_storage.save(deps.storage, pair.token_1.clone(), &total_reserve_1)?;
    balances_storage.save(deps.storage, pair.token_2.clone(), &total_reserve_2)?;

    // Add current balance to SNAPSHOT MAP

    // Prepare Liquidity Response
    let liquidity_response = AddLiquidityResponse {
        mint_lp_tokens: lp_allocation,
        vlp_address: env.contract.address.to_string(),
        tx_id: tx_id.clone(),
        sender: sender.clone(),
    };

    // Prepare acknowledgement
    let acknowledgement = to_json_binary(&liquidity_response)?;

    Ok(response
        .add_event(tx_event(
            &tx_id,
            &sender.to_sender_string(),
            TxType::AddLiquidity,
        ))
        .add_event(liquidity_event(
            &pair
                .get_pair_with_amount(total_reserve_1, total_reserve_2)?
                .get_vec_token(),
            &liquidity.get_vec_token(),
            &tx_id,
        ))
        .add_attribute("action", "add_liquidity")
        .add_attribute("sender", sender.to_sender_string())
        .add_attribute("lp_allocation", lp_allocation)
        .add_attribute("liquidity_1_added", token_1_liquidity)
        .add_attribute("liquidity_2_added", token_2_liquidity)
        .set_data(acknowledgement))
}

#[cw_serde]
pub enum SwapCalculationMethod {
    Stable(Uint64),
    Regular,
}

#[cw_serde]
pub struct PreSwapResponse {
    pub lp_fee: Uint128,
    pub euclid_fee: Uint128,
    pub swap_amount: Uint128,
    pub receive_amount: Uint128,
    pub asset_out: Token,
    pub spread_amount: Uint128,
}

pub fn pre_swap(
    deps: &Deps,
    state_storage: &Item<State>,
    balances_storage: &Map<Token, Uint128>,
    asset_in: &Token,
    amount_in: Uint128,
    calculation_method: SwapCalculationMethod,
    test_fail: Option<bool>,
) -> Result<PreSwapResponse, ContractError> {
    ensure!(
        !test_fail.unwrap_or(false),
        ContractError::new("Force fail flag")
    );
    // Verify that the asset amount is non-zero
    ensure!(!amount_in.is_zero(), ContractError::ZeroAssetAmount {});

    let state = state_storage.load(deps.storage)?;

    let pair = state.pair.clone();

    ensure!(asset_in.exists(pair), ContractError::AssetDoesNotExist {});
    let asset_out = state.pair.get_other_token(asset_in.clone());

    let token_in_reserve = balances_storage.load(deps.storage, asset_in.clone())?;
    let token_out_reserve = balances_storage.load(deps.storage, asset_out.clone())?;

    // Get Fee from the state
    let fee = state.clone().fee;

    let lp_fee = amount_in.checked_mul_floor(Decimal::bps(fee.lp_fee_bps))?;
    let euclid_fee = amount_in.checked_mul_floor(Decimal::bps(fee.euclid_fee_bps))?;

    let swap_amount = amount_in.checked_sub(lp_fee.checked_add(euclid_fee)?)?;

    let (receive_amount, spread_amount) = match calculation_method {
        SwapCalculationMethod::Stable(amp_factor) => {
            let swap_result = compute_stable_swap(
                &Decimal256::from_integer(amount_in),
                &Decimal256::from_integer(token_in_reserve),
                &Decimal256::from_integer(token_out_reserve),
                amp_factor,
            )?;
            (swap_result.return_amount, swap_result.spread_amount)
        }
        SwapCalculationMethod::Regular => {
            let swap_result = calculate_cp_swap(swap_amount, token_in_reserve, token_out_reserve)?;
            (swap_result.return_amount, swap_result.spread_amount)
        }
    };

    Ok(PreSwapResponse {
        lp_fee,
        euclid_fee,
        swap_amount,
        receive_amount,
        asset_out,
        spread_amount,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn execute_swap(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    state_storage: &Item<State>,
    balances_storage: &Map<Token, Uint128>,
    sender: CrossChainUser,
    asset_in: Token,
    amount_in: Uint128,
    min_token_out: Uint128,
    tx_id: String,
    next_swaps: Vec<NextSwapVlp>,
    calculation_method: SwapCalculationMethod,
    test_fail: Option<bool>,
) -> Result<Response, ContractError> {
    let mut state = state_storage.load(deps.storage)?;

    // If the sender is the router, use the sender as the voucher sender
    // Otherwise, use the last contract caller as the voucher sender
    let voucher_sender = if info.sender.as_str() == state.router {
        sender.clone()
    } else {
        CrossChainUser {
            address: info.sender.to_string(),
            chain_uid: ChainUid::vsl_chain_uid()?,
        }
    };

    // Swap needs approval to use voucher tokens
    let transfer_voucher_msg =
        crate::msgs::virtual_balance::ExecuteMsg::Transfer(ExecuteTransfer {
            amount: amount_in,
            token_id: asset_in.to_string(),
            from: Some(voucher_sender.clone()),
            to: CrossChainUser {
                address: env.contract.address.to_string(),
                chain_uid: ChainUid::vsl_chain_uid()?,
            },
            sender: None,
            msg: None,
        });

    let transfer_voucher_msg = WasmMsg::Execute {
        contract_addr: state.virtual_balance.clone(),
        msg: to_json_binary(&transfer_voucher_msg)?,
        funds: vec![],
    };

    let mut response = Response::new();
    // Should reject full execution if failed
    response = response.add_message(transfer_voucher_msg);

    let PreSwapResponse {
        lp_fee,
        euclid_fee,
        swap_amount,
        receive_amount,
        asset_out,
        spread_amount,
    } = pre_swap(
        &deps.as_ref(),
        state_storage,
        balances_storage,
        &asset_in,
        amount_in,
        calculation_method,
        test_fail,
    )?;

    // Add the lp fee to total fees
    state
        .total_fees_collected
        .lp_fees
        .add_fee(asset_in.to_string(), lp_fee);

    // Calcuate the sum of fees
    let total_fee = lp_fee.checked_add(euclid_fee)?;

    // Verify that the receive amount is greater than 0 to be eligible for any swap
    ensure!(
        !receive_amount.is_zero(),
        ContractError::SlippageExceeded {
            amount: receive_amount,
            min_amount_out: min_token_out,
        }
    );

    let mut token_in_reserve = balances_storage.load(deps.storage, asset_in.clone())?;
    let mut token_out_reserve = balances_storage.load(deps.storage, asset_out.clone())?;

    token_in_reserve = token_in_reserve
        .checked_add(swap_amount)?
        .checked_add(lp_fee)?;
    token_out_reserve = token_out_reserve.checked_sub(receive_amount)?;

    ensure!(
        !token_out_reserve.is_zero(),
        ContractError::new("Token out reserve is zero")
    );

    balances_storage.save(deps.storage, asset_in.clone(), &token_in_reserve)?;
    balances_storage.save(deps.storage, asset_out.clone(), &token_out_reserve)?;

    // Finalize ack response to swap pool
    let swap_response = VlpSwapResponse {
        sender: sender.clone(),
        tx_id: tx_id.clone(),
        asset_out: asset_out.clone(),
        amount_out: receive_amount,
    };

    // Prepare acknowledgement
    let acknowledgement = to_json_binary(&swap_response)?;

    if !euclid_fee.is_zero() {
        // Get Fee from the state
        let fee = state.clone().fee;
        // Add the euclid fee to total fees
        state
            .total_fees_collected
            .euclid_fees
            .add_fee(asset_in.to_string(), euclid_fee);

        let euclid_fee_transfer_msg =
            crate::msgs::virtual_balance::ExecuteMsg::Transfer(ExecuteTransfer {
                amount: euclid_fee,
                token_id: asset_in.to_string(),
                to: fee.recipient,
                from: None,
                sender: None,
                msg: None,
            });

        let euclid_fee_transfer_msg = WasmMsg::Execute {
            contract_addr: state.virtual_balance.clone(),
            msg: to_json_binary(&euclid_fee_transfer_msg)?,
            funds: vec![],
        };

        response = response.add_message(euclid_fee_transfer_msg);
    }

    match next_swaps.split_first() {
        Some((next_swap, forward_swaps)) => {
            // There are more swaps
            let virtual_balance_approve_msg =
                crate::msgs::virtual_balance::ExecuteMsg::Approve(ExecuteApprove {
                    amount: swap_response.amount_out,
                    token_id: swap_response.asset_out.to_string(),

                    owner: CrossChainUser {
                        address: env.contract.address.to_string(),
                        chain_uid: ChainUid::vsl_chain_uid()?,
                    },

                    spender: CrossChainUser {
                        address: next_swap.vlp_address.clone(),
                        chain_uid: ChainUid::vsl_chain_uid()?,
                    },
                });

            let virtual_balance_approve_msg = WasmMsg::Execute {
                contract_addr: state.virtual_balance.clone(),
                msg: to_json_binary(&virtual_balance_approve_msg)?,
                funds: vec![],
            };

            let next_swap_msg = crate::msgs::vlp::ExecuteMsg::Swap {
                sender: sender.clone(),
                // Final user address and chain id

                // Carry forward amount to next swap
                asset_in: swap_response.asset_out,
                amount_in: swap_response.amount_out,
                min_token_out,
                tx_id: tx_id.clone(),
                next_swaps: forward_swaps.to_vec(),
                test_fail: next_swap.test_fail,
            };
            let next_swap_msg = WasmMsg::Execute {
                contract_addr: next_swap.vlp_address.clone(),
                msg: to_json_binary(&next_swap_msg)?,
                funds: vec![],
            };

            let next_swap_msg = SubMsg::reply_always(next_swap_msg, NEXT_SWAP_REPLY_ID);

            response = response
                .add_attribute("swap_type", "forward_swap")
                .add_attribute("forward_to", next_swap.vlp_address.clone())
                .add_message(virtual_balance_approve_msg)
                .add_submessage(next_swap_msg);
        }
        None => {
            //Its the last swap

            // Verify that the receive amount is >= min amount as its last swap
            ensure!(
                receive_amount.ge(&min_token_out),
                ContractError::SlippageExceeded {
                    amount: receive_amount,
                    min_amount_out: min_token_out,
                }
            );

            let virtual_balance_transfer_msg =
                crate::msgs::virtual_balance::ExecuteMsg::Transfer(ExecuteTransfer {
                    amount: swap_response.amount_out,
                    token_id: swap_response.asset_out.to_string(),

                    // Destination Address
                    to: sender.clone(),
                    sender: None,
                    from: None,
                    msg: None,
                });

            let virtual_balance_transfer_msg = WasmMsg::Execute {
                contract_addr: state.virtual_balance.clone(),
                msg: to_json_binary(&virtual_balance_transfer_msg)?,
                funds: vec![],
            };

            response = response
                .add_attribute("swap_type", "final_swap")
                .add_attribute("receiver_address", sender.address.clone())
                .add_attribute("receiver_chain_id", sender.chain_uid.to_string())
                .add_message(virtual_balance_transfer_msg);
        }
    };

    // Save changes to total fees in state
    state_storage.save(deps.storage, &state)?;

    Ok(response
        .add_event(tx_event(&tx_id, &sender.to_sender_string(), TxType::Swap))
        .add_event(liquidity_event(
            &[
                asset_in.with_amount(token_in_reserve),
                asset_out.with_amount(token_out_reserve),
            ],
            &[
                asset_in.with_amount(swap_amount.checked_add(lp_fee)?),
                asset_out.with_amount(receive_amount),
            ],
            &tx_id,
        ))
        .add_attribute("action", "swap")
        .add_attribute("amount_in", amount_in)
        .add_attribute("asset_in", asset_in.to_string())
        .add_attribute("asset_out", asset_out.to_string())
        .add_attribute("total_fee", total_fee)
        .add_attribute("euclid_fee", euclid_fee)
        .add_attribute("lp_fee", lp_fee)
        .add_attribute("receive_amount", receive_amount)
        .add_attribute("spread_amount", spread_amount)
        .set_data(acknowledgement))
}

#[cw_serde]
pub struct GetSwapResponse {
    pub amount_out: Uint128,
    pub asset_out: Token,
    pub spread_amount: Uint128,
    pub lp_fee: Uint128,
    pub euclid_fee: Uint128,
}

pub fn simulate_swap(
    deps: Deps,
    state_storage: &Item<State>,
    balances_storage: &Map<Token, Uint128>,
    asset_in: Token,
    amount_in: Uint128,
    calculation_method: SwapCalculationMethod,
) -> Result<GetSwapResponse, ContractError> {
    let pre_swap_response = pre_swap(
        &deps,
        state_storage,
        balances_storage,
        &asset_in,
        amount_in,
        calculation_method,
        None,
    )?;

    Ok(GetSwapResponse {
        amount_out: pre_swap_response.receive_amount,
        asset_out: pre_swap_response.asset_out,
        spread_amount: pre_swap_response.spread_amount,
        lp_fee: pre_swap_response.lp_fee,
        euclid_fee: pre_swap_response.euclid_fee,
    })
}

pub fn calculate_amount_from_shares(
    reserve: Uint128,
    shares: Uint128,
    total_shares: Uint128,
) -> Result<Uint128, ContractError> {
    let amount = reserve.checked_multiply_ratio(shares, total_shares)?;
    Ok(amount)
}
