use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Int256, Uint512};
use cw_storage_plus::{Item, Map};
use euclid::{
    chain::ChainUid,
    deposit::DepositTokenRequest,
    fee::DenomFees,
    liquidity::{AddLiquidityRequest, RemoveLiquidityRequest},
    swap::SwapRequest,
    token::{PairWithDenomAndAmount, Token, TokenWithDenom, TokenWithDenomAndAmount},
};

#[cw_serde]
pub struct State {
    // The Router Contract Address on the Virtual Settlement Layer
    pub router_contract: String,
    pub relayer_contract: Addr,
    // Contract admin
    pub admin: String,
    // Escrow Code ID
    pub escrow_code_id: u64,
    // LP Token Code ID
    pub lp_code_id: u64,
    // The Unique Chain Identifier
    // THIS IS DIFFERENT THAN THE CHAIN_ID OF THE CHAIN, THIS REPRESENTS A UNIQUE IDENTIFIER FOR THE CHAIN
    // IN THE EUCLID ECOSYSTEM
    pub chain_uid: ChainUid,
    pub is_native: bool,
}

pub const STATE: Item<State> = Item::new("state");

#[cw_serde]
pub struct FeeState {
    pub rate_limit_fee_recipient: Addr,
    pub rate_limit_fee_denom: String,

    // Total rate limit fee collected till now
    pub rate_limit_fee_collected: Uint512,
    // Total partner fees collected till now
    pub partner_fees_collected: DenomFees,
}

pub const FEE_STATE: Item<FeeState> = Item::new("fee_state");

// Map Pair to vlp address
pub const PAIR_TO_VLP: Map<(String, String), String> = Map::new("pair_to_vlp");

// Map vlp to LP Allocations
pub const VLP_TO_LP_SHARES: Map<String, Int256> = Map::new("vlp_to_lp_shares");

// New Factory states
pub const TOKEN_TO_ESCROW: Map<Token, Addr> = Map::new("token_to_escrow");

// New LP Token states
pub const VLP_TO_LP_TOKEN: Map<String, Addr> = Map::new("vlp_to_lp_token");

#[cw_serde]
pub struct PoolCreateRequest {
    pub tx_id: String,
    pub sender: Addr,
    pub pair_info: PairWithDenomAndAmount,
    pub lp_token_instantiate_msg: cw20_base::msg::InstantiateMsg,
}
// Map for pending pool requests for user
pub const PENDING_POOL_REQUESTS: Map<(Addr, String), PoolCreateRequest> =
    Map::new("request_to_pool");

#[cw_serde]
pub struct DenomRequest {
    pub tx_id: String,
    pub sender: Addr,
    pub token: TokenWithDenom,
}
pub const PENDING_DENOM_REQUESTS: Map<(Addr, String), DenomRequest> =
    Map::new("pending_denom_requests");

// Map for pending swaps for user
pub const PENDING_SWAPS: Map<(Addr, String), SwapRequest> = Map::new("pending_swaps");

// Map for pending token deposits for user
pub const PENDING_TOKEN_DEPOSIT: Map<(Addr, String), DepositTokenRequest> =
    Map::new("pending_token_deposit");

// Map for PENDING liquidity transactions
pub const PENDING_ADD_LIQUIDITY: Map<(Addr, String), AddLiquidityRequest> =
    Map::new("pending_add_liquidity");
// Map for PENDING liquidity transactions
pub const PENDING_REMOVE_LIQUIDITY: Map<(Addr, String), RemoveLiquidityRequest> =
    Map::new("pending_remove_liquidity");

pub const PENDING_DEPOSIT_TOKEN: Map<Token, TokenWithDenomAndAmount> =
    Map::new("pending_deposit_token");
