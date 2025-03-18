use crate::{
    chain::CrossChainUser,
    error::ContractError,
    events::simple_event,
    fee::{Fee, TotalFees, MAX_FEE_BPS},
    token::{Pair, PairWithDenomAndAmount, TokenWithDenom},
};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{ensure, DepsMut, MessageInfo, Response, Uint128, Uint64};
use cw_storage_plus::Item;

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
