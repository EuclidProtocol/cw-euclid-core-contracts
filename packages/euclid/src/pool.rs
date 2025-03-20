use crate::msgs::stable_vlp::DEFAULT_AMP_FACTOR;
use crate::{
    chain::{ChainUid, CrossChainUser},
    error::ContractError,
    events::{liquidity_event, simple_event, tx_event, TxType},
    fee::{Fee, TotalFees, BPS_50_PERCENT, MAX_FEE_BPS},
    liquidity::AddLiquidityResponse,
    msgs::{
        stable_vlp::compute_swap,
        virtual_balance::{ExecuteApprove, ExecuteTransfer},
        vlp::calculate_swap,
    },
    swap::NextSwapVlp,
    token::{Pair, PairWithAmount, PairWithDenomAndAmount, Token, TokenWithDenom},
    utils::math::Decimal256Ext,
};
pub const VIRTUAL_BALANCE_TRANSFER_REPLY_ID: u64 = 1;
pub const NEXT_SWAP_REPLY_ID: u64 = 2;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    ensure, to_json_binary, Decimal, Decimal256, DepsMut, Env, Isqrt, MessageInfo, Response,
    SubMsg, Uint128, Uint64, WasmMsg,
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
pub struct DenomRegisterDeregisterRequest {
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

pub fn calculate_lp_allocation(
    token_1_amount: Uint128,
    token_2_amount: Uint128,
    total_liquidity_1: Uint128,
    total_liquidity_2: Uint128,
    total_lp_supply: Uint128,
) -> Result<Uint128, ContractError> {
    // IF LP supply is 0 use original function
    if total_lp_supply.is_zero() {
        let sq_root = Isqrt::isqrt(token_1_amount.checked_mul(token_2_amount)?);
        return Ok(sq_root.checked_sub(Uint128::new(MINIMUM_LIQUIDITY))?);
    }

    let lp_allocation = token_1_amount
        .checked_multiply_ratio(total_lp_supply, total_liquidity_1)?
        .min(token_2_amount.checked_multiply_ratio(total_lp_supply, total_liquidity_2)?);

    Ok(lp_allocation)
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
    ensure!(info.sender == state.admin, ContractError::Unauthorized {});

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
        .add_event(simple_event()))
}

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
    let mut response = Response::new().add_attribute("action", "update_state");

    let state = state_storage.load(deps.storage)?;
    ensure!(info.sender == state.admin, ContractError::Unauthorized {});

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

    if let Some(amp_factor) = amp_factor {
        if let Some(storage) = amp_factor_storage {
            storage.save(deps.storage, &amp_factor)?;
            response = response.add_attribute("amp_factor_updated", amp_factor.to_string());
        }
    }

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

    Ok(response)
}

pub fn register_pool(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    state_storage: &Item<State>,
    chain_lp_tokens: &Map<ChainUid, Uint128>,
    amp_factor_storage: Option<&Item<Uint64>>,
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
    chain_lp_tokens.save(deps.storage, sender.chain_uid.clone(), &Uint128::zero())?;

    let ack = PoolCreationResponse {
        vlp_contract: env.contract.address.to_string(),
        tx_id: tx_id.clone(),
        mint_lp_tokens: Uint128::zero(),
        sender: sender.clone(),
    };
    let pool_type = if amp_factor_storage.is_some() {
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

    if let Some(amp_factor_storage) = amp_factor_storage {
        response = response.add_attribute(
            "amp_factor",
            amp_factor_storage.load(deps.storage)?.to_string(),
        );
    }

    Ok(response)
}

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

    let vlp_cross_chain_struct = CrossChainUser {
        address: env.contract.address.to_string(),
        chain_uid: ChainUid::vsl_chain_uid()?,
    };

    let token_1_transfer_msg = pair.token_1.create_virtual_balance_transfer_msg(
        state.virtual_balance.clone(),
        token_1_liquidity,
        vlp_cross_chain_struct.clone(),
        sender.clone(),
    )?;

    let token_2_transfer_msg = pair.token_2.create_virtual_balance_transfer_msg(
        state.virtual_balance,
        token_2_liquidity,
        vlp_cross_chain_struct,
        sender.clone(),
    )?;

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            &sender.to_sender_string(),
            TxType::RemoveLiquidity,
        ))
        .add_submessage(SubMsg::reply_always(
            token_1_transfer_msg,
            VIRTUAL_BALANCE_TRANSFER_REPLY_ID,
        ))
        .add_submessage(SubMsg::reply_always(
            token_2_transfer_msg,
            VIRTUAL_BALANCE_TRANSFER_REPLY_ID,
        ))
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

pub fn add_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    state_storage: &Item<State>,
    balances_storage: &Map<Token, Uint128>,
    chain_lp_tokens_storage: &Map<ChainUid, Uint128>,
    sender: CrossChainUser,
    liquidity: PairWithAmount,
    slippage_tolerance_bps: u64,
    tx_id: String,
) -> Result<Response, ContractError> {
    let mut state = state_storage.load(deps.storage)?;
    ensure!(info.sender == state.router, ContractError::Unauthorized {});
    let mut response = Response::new();

    // Ensure tokens are received by VLP
    for token in liquidity.get_vec_token() {
        // Contract should have approval to use voucher tokens on behalf of sender
        let virtual_balance_transfer_msg = token.token.create_virtual_balance_transfer_msg(
            state.virtual_balance.clone(),
            token.amount,
            sender.clone(),
            CrossChainUser {
                address: env.contract.address.to_string(),
                chain_uid: ChainUid::vsl_chain_uid()?,
            },
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

    state.total_lp_tokens = state.total_lp_tokens.checked_add(lp_allocation)?;
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
    Stable,
    Regular,
}

#[allow(clippy::too_many_arguments)]
pub fn execute_swap(
    deps: DepsMut,
    env: Env,
    state_storage: &Item<State>,
    balances_storage: &Map<Token, Uint128>,
    amp_factor_storage: Option<&Item<Uint64>>,
    sender: CrossChainUser,
    asset_in: Token,
    amount_in: Uint128,
    min_token_out: Uint128,
    tx_id: String,
    next_swaps: Vec<NextSwapVlp>,
    calculation_method: SwapCalculationMethod,
    test_fail: Option<bool>,
) -> Result<Response, ContractError> {
    ensure!(
        !test_fail.unwrap_or(false),
        ContractError::new("Force fail flag")
    );
    // Verify that the asset amount is non-zero
    ensure!(!amount_in.is_zero(), ContractError::ZeroAssetAmount {});

    let mut state = state_storage.load(deps.storage)?;

    let pair = state.pair.clone();

    ensure!(asset_in.exists(pair), ContractError::AssetDoesNotExist {});
    let asset_out = state.pair.get_other_token(asset_in.clone());

    let mut token_in_reserve = balances_storage.load(deps.storage, asset_in.clone())?;
    let mut token_out_reserve = balances_storage.load(deps.storage, asset_out.clone())?;

    // Swap needs approval to use voucher tokens
    let transfer_voucher_msg =
        crate::msgs::virtual_balance::ExecuteMsg::Transfer(ExecuteTransfer {
            amount: amount_in,
            token_id: asset_in.to_string(),
            from: sender.clone(),
            to: CrossChainUser {
                address: env.contract.address.to_string(),
                chain_uid: ChainUid::vsl_chain_uid()?,
            },
        });

    let transfer_voucher_msg = WasmMsg::Execute {
        contract_addr: state.virtual_balance.clone(),
        msg: to_json_binary(&transfer_voucher_msg)?,
        funds: vec![],
    };

    let mut response = Response::new();
    // Should reject full execution if failed
    response = response.add_message(transfer_voucher_msg);

    // Get Fee from the state
    let fee = state.clone().fee;

    let lp_fee = amount_in.checked_mul_floor(Decimal::bps(fee.lp_fee_bps))?;
    let euclid_fee = amount_in.checked_mul_floor(Decimal::bps(fee.euclid_fee_bps))?;

    // Add the lp fee to total fees
    state
        .total_fees_collected
        .lp_fees
        .add_fee(asset_in.to_string(), lp_fee);

    // Calcuate the sum of fees
    let total_fee = lp_fee.checked_add(euclid_fee)?;

    // Calculate the amount of asset to be swapped
    let swap_amount = amount_in.checked_sub(total_fee)?;

    let receive_amount = match calculation_method {
        SwapCalculationMethod::Stable {} => {
            compute_swap(
                &Decimal256::from_integer(amount_in),
                &Decimal256::from_integer(token_in_reserve),
                &Decimal256::from_integer(token_out_reserve),
                amp_factor_storage
                    .unwrap()
                    .load(deps.storage)
                    .unwrap_or(DEFAULT_AMP_FACTOR),
            )?
            .return_amount
        }
        SwapCalculationMethod::Regular => {
            calculate_swap(swap_amount, token_in_reserve, token_out_reserve)?
        }
    };

    // Verify that the receive amount is greater than 0 to be eligible for any swap
    ensure!(
        !receive_amount.is_zero(),
        ContractError::SlippageExceeded {
            amount: receive_amount,
            min_amount_out: min_token_out,
        }
    );

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
        // Add the euclid fee to total fees
        state
            .total_fees_collected
            .euclid_fees
            .add_fee(asset_in.to_string(), euclid_fee);

        let euclid_fee_transfer_msg =
            crate::msgs::virtual_balance::ExecuteMsg::Transfer(ExecuteTransfer {
                amount: euclid_fee,
                token_id: asset_in.to_string(),

                // Source Address
                from: CrossChainUser {
                    address: env.contract.address.to_string(),
                    chain_uid: ChainUid::vsl_chain_uid()?,
                },

                // Destination Address
                to: fee.recipient,
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

                    // Source Address
                    from: CrossChainUser {
                        address: env.contract.address.to_string(),
                        chain_uid: ChainUid::vsl_chain_uid()?,
                    },

                    // Destination Address
                    to: sender.clone(),
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
        .set_data(acknowledgement))
}
