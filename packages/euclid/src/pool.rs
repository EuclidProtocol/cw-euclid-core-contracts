use crate::{
    chain::{ChainUid, CrossChainUser},
    error::ContractError,
    events::{simple_event, tx_event, TxType},
    fee::{Fee, TotalFees, MAX_FEE_BPS},
    token::{Pair, PairWithDenomAndAmount, TokenWithDenom},
};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{ensure, to_json_binary, DepsMut, Env, MessageInfo, Response, Uint128, Uint64};
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
    let mut res = Response::new();
    if let Some(amp_factor_storage) = amp_factor_storage {
        res = res.add_attribute(
            "amp_factor",
            amp_factor_storage.load(deps.storage)?.to_string(),
        );
    }

    Ok(res
        .add_event(tx_event(
            &tx_id,
            &sender.to_sender_string(),
            TxType::PoolCreation,
        ))
        .add_attribute("action", "register_pool")
        .add_attribute("pool_chain", sender.chain_uid.to_string())
        .add_attribute("pool_type", "stable")
        .set_data(to_json_binary(&ack)?))
}
