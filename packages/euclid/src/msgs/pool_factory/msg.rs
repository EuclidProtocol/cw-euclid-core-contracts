use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary};

use crate::{
    msgs::cross_chain_config::CrossChainConfig,
    token::{Pair, PairWithDenomAndAmount},
};

#[cw_serde]
pub struct InstantiateMsg {
    /// Main factory address on this chain. All `On*` entries require this caller.
    pub main_factory_address: String,
}

#[cw_serde]
#[cfg_attr(not(target_arch = "wasm32"), derive(cw_orch::ExecuteFns))]
pub enum ExecuteMsg {
    /// Called by main factory to delegate a CP/Stable pool creation request.
    /// pool_factory builds the outbound IBC packet and calls back into main
    /// factory's `ProxySendPacket` to dispatch it.
    OnRequestPoolCreation {
        tx_id: String,
        sender: Addr,
        pair_with_denom_and_amount: PairWithDenomAndAmount,
        pool_config: crate::msgs::vlp::base::PoolConfig,
        lp_token_name: String,
        lp_token_symbol: String,
        lp_token_decimal: u8,
        lp_token_marketing: Option<cw20_base::msg::InstantiateMarketingInfo>,
        slippage_tolerance_bps: u64,
        cross_chain_config: crate::msgs::cross_chain_config::CrossChainConfig,
    },

    /// Called by main factory to delegate a CP/Stable add-liquidity request.
    /// Main factory has already deposited the funds to escrow and generated
    /// `tx_id`. Pool factory records the pending entry, builds the outbound
    /// `RouterCrossChainExecuteMsg::AddLiquidity` packet via `outbound`, and
    /// calls back into main factory's `ProxySendPacket` for dispatch.
    OnAddLiquidity {
        tx_id: String,
        sender: Addr,
        pair_with_denom_and_amount: PairWithDenomAndAmount,
        slippage_tolerance_bps: u64,
        cross_chain_config: CrossChainConfig,
    },

    /// Called by main factory after an IBC ack arrives for a pool variant.
    /// `original_msg` is the originally sent `RouterCrossChainExecuteMsg`
    /// serialised, and `ack` is the raw acknowledgement bytes.
    OnPoolAck {
        original_msg: Binary,
        ack: Binary,
        is_native: bool,
    },

    /// One-shot migration entry. Called by main factory to push pool state
    /// into the new pool_factory contract.
    MigrateAcceptPoolState {
        pair_to_vlp: Vec<(Pair, String)>,
        vlp_to_lp_token: Vec<(String, Addr)>,
    },
}

#[cw_serde]
#[derive(cw_orch::QueryFns, QueryResponses)]
pub enum QueryMsg {
    /// Returns the VLP address for a given pair (CP/Stable pools).
    #[returns(GetVlpResponse)]
    GetVlp { pair: Pair },

    /// Returns the LP token address for a given VLP address.
    #[returns(GetLpTokenResponse)]
    GetLpToken { vlp: String },

    /// Returns the configured main factory address.
    #[returns(MainFactoryAddressResponse)]
    GetMainFactoryAddress {},
}

#[cw_serde]
pub struct GetVlpResponse {
    pub vlp_address: Option<String>,
}

#[cw_serde]
pub struct GetLpTokenResponse {
    pub token_address: Option<Addr>,
}

#[cw_serde]
pub struct MainFactoryAddressResponse {
    pub main_factory_address: Addr,
}

#[cw_serde]
pub struct MigrateMsg {}
