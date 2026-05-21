use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Int256, Uint128, Uint512};
use cw_storage_plus::{Item, Map};
use euclid::{
    admin::EuclidAdmin,
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    deposit::DepositTokenRequest,
    fee::DenomFees,
    liquidity::{AddLiquidityRequest, RemoveLiquidityRequest},
    msgs::vlp::base::PoolKey,
    swap::SwapRequest,
    token::{PairWithDenomAndAmount, Token, TokenWithDenom, TokenWithDenomAndAmount},
};

#[cw_serde]
pub struct State {
    // The Router Contract Address on the Virtual Settlement Layer
    pub router_contract: String,
    pub relayer_contract: Addr,
    // Escrow Code ID
    pub escrow_code_id: u64,
    // LP Token Code ID
    pub lp_code_id: u64,
    // Position Token Code ID
    pub position_token_code_id: u64,
    // The Unique Chain Identifier
    // THIS IS DIFFERENT THAN THE CHAIN_ID OF THE CHAIN, THIS REPRESENTS A UNIQUE IDENTIFIER FOR THE CHAIN
    // IN THE EUCLID ECOSYSTEM
    pub chain_uid: ChainUid,
    pub is_native: bool,
}

pub const STATE: Item<State> = Item::new("state");
pub const ADMIN: Item<EuclidAdmin> = Item::new("admin");

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
// Map pool key to vlp address - for concentrated pools
pub const POOL_KEY_TO_VLP: Map<String, String> = Map::new("pool_key_to_vlp");

// Map vlp to LP Allocations. (Might not be needed anymore as LP token will have this data or can be updated to store this data)
pub const VLP_TO_LP_SHARES: Map<String, Int256> = Map::new("vlp_to_lp_shares");

// New Factory states
pub const TOKEN_TO_ESCROW: Map<Token, Addr> = Map::new("token_to_escrow");

// New LP Token states. Only applicable for constant product and stable pools
pub const VLP_TO_LP_TOKEN: Map<String, Addr> = Map::new("vlp_to_lp_token");

// Common position token contract for all concentrated pools
pub const POSITION_TOKEN_CONTRACT: Item<Addr> = Item::new("position_token_contract");

// Address of the pool_factory companion contract on this chain. Written by
// either the Sirius drain-and-cut migration or `SetPoolFactory` on a fresh
// chain. While `POOL_FACTORY_INITIALISED == false`, main Factory keeps owning
// the pool code paths; once flipped, pool ops delegate to this address.
pub const POOL_FACTORY_ADDRESS: Item<Addr> = Item::new("pool_factory_address");
pub const POOL_FACTORY_INITIALISED: Item<bool> = Item::new("pool_factory_initialised");

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
pub struct DenomRegisterDeregisterRequest {
    pub tx_id: String,
    pub sender: Addr,
    pub token: TokenWithDenom,
}
pub const PENDING_DENOM_REGISTER_DEREGISTER_REQUESTS: Map<
    (Addr, String),
    DenomRegisterDeregisterRequest,
> = Map::new("request_denom_register_deregister");

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

#[cw_serde]
pub struct ConcentratedPoolCreateRequest {
    pub tx_id: String,
    pub sender: Addr,
    pub pair_info: PairWithDenomAndAmount,
    pub pool_key: PoolKey,
}
pub const PENDING_CONCENTRATED_POOL_REQUESTS: Map<(Addr, String), ConcentratedPoolCreateRequest> =
    Map::new("pending_concentrated_pool_requests");

#[cw_serde]
pub struct ConcentratedAddLiquidityRequest {
    pub tx_id: String,
    pub sender: Addr,
    pub pair_info: PairWithDenomAndAmount,
    pub pool_key: PoolKey,
    pub lower_tick_index: i64,
    pub upper_tick_index: i64,
    pub position_id: Option<u128>,
}
pub const PENDING_CONCENTRATED_ADD_LIQUIDITY: Map<(Addr, String), ConcentratedAddLiquidityRequest> =
    Map::new("pending_concentrated_add_liquidity");

#[cw_serde]
pub struct ConcentratedRemoveLiquidityRequest {
    pub tx_id: String,
    pub sender: Addr,
    pub pool_key: PoolKey,
    pub position_id: u128,
    pub liquidity_delta: cosmwasm_std::Uint128,
}
pub const PENDING_CONCENTRATED_REMOVE_LIQUIDITY: Map<
    (Addr, String),
    ConcentratedRemoveLiquidityRequest,
> = Map::new("pending_concentrated_remove_liquidity");

#[cw_serde]
pub struct ConcentratedCollectFeesRequest {
    pub tx_id: String,
    pub sender: Addr,
    pub pool_key: PoolKey,
    pub position_id: u128,
    pub recipient: CrossChainUser,
}
pub const PENDING_CONCENTRATED_COLLECT_FEES: Map<(Addr, String), ConcentratedCollectFeesRequest> =
    Map::new("pending_concentrated_collect_fees");

#[cw_serde]
pub struct ConcentratedCollectProtocolFeesRequest {
    pub tx_id: String,
    pub sender: Addr,
    pub pool_key: PoolKey,
    pub recipient: CrossChainUser,
    pub amount_0_requested: Uint128,
    pub amount_1_requested: Uint128,
}
pub const PENDING_CONCENTRATED_COLLECT_PROTOCOL_FEES: Map<
    (Addr, String),
    ConcentratedCollectProtocolFeesRequest,
> = Map::new("pending_concentrated_collect_protocol_fees");

pub const PENDING_DEPOSIT_TOKEN: Map<Token, TokenWithDenomAndAmount> =
    Map::new("pending_deposit_token");

pub fn pool_key_to_map_key(pool_key: &PoolKey) -> String {
    let (fee_tier_bps, tick_spacing) = match pool_key.pool_type {
        euclid::msgs::vlp::base::PoolType::Concentrated {
            fee_tier_bps,
            tick_spacing,
        } => (fee_tier_bps, tick_spacing),
        _ => (0, 0),
    };
    format!(
        "{}\0{}\0{}\0{}",
        pool_key.pair.token_1, pool_key.pair.token_2, fee_tier_bps, tick_spacing
    )
}
#[cfg(test)]
mod tests {
    use cosmwasm_std::{testing::mock_dependencies, Addr};
    use euclid::token::{Pair, Token};

    use crate::testing::helpers::{init, seed_vlp};

    use super::{PAIR_TO_VLP, VLP_TO_LP_TOKEN};

    // -----------------------------------------------------------------------
    // State invariant: PAIR_TO_VLP and VLP_TO_LP_TOKEN are independent
    // -----------------------------------------------------------------------

    #[test]
    fn test_state_seed_vlp_and_lp_token_independently() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        seed_vlp(&mut deps, "aaa", "bbb", "vlp_addr_1");
        VLP_TO_LP_TOKEN
            .save(
                deps.as_mut().storage,
                "vlp_addr_1".to_string(),
                &Addr::unchecked("lp_token_1"),
            )
            .unwrap();

        let pair = Pair::new(
            Token::create("aaa".to_string()).unwrap(),
            Token::create("bbb".to_string()).unwrap(),
        )
        .unwrap();
        let vlp = PAIR_TO_VLP.load(&deps.storage, pair.get_tupple()).unwrap();
        assert_eq!(vlp, "vlp_addr_1");

        let lp = VLP_TO_LP_TOKEN
            .load(&deps.storage, "vlp_addr_1".to_string())
            .unwrap();
        assert_eq!(lp, Addr::unchecked("lp_token_1"));
    }
}
