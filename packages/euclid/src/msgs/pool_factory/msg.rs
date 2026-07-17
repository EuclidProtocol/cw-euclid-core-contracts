use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary, Uint256};

use crate::{
    cross_chain_user::CrossChainUser,
    msgs::cross_chain_config::CrossChainConfig,
    msgs::vlp::base::PoolKey,
    token::{Pair, PairWithDenomAndAmount},
};

#[cw_serde]
pub struct InstantiateMsg {
    /// Main factory address on this chain. All `On*` entries require this caller.
    pub main_factory_address: String,
}

#[cw_serde]
#[cfg_attr(not(target_arch = "wasm32"), derive(cw_orch::ExecuteFns))]
#[cfg_attr(feature = "cross-vm", derive(cross_vm_macros::CwExecuteFns))]
#[cfg_attr(feature = "cross-vm", cross_vm(trait_name = "PoolFactoryExecuteFns"))]
pub enum ExecuteMsg {
    /// Called by main factory to delegate a CP/Stable pool creation request.
    /// pool_factory builds the outbound IBC packet and returns it via
    /// `Response::data` typed as `PoolFactoryReply::SendPacket`; main
    /// factory's reply handler runs the dispatch.
    OnRequestPoolCreation {
        tx_id: String,
        sender: Addr,
        pair_with_denom_and_amount: PairWithDenomAndAmount,
        pool_config: crate::msgs::vlp::base::PoolConfig,
        lp_token_name: String,
        lp_token_symbol: String,
        lp_token_marketing: Option<cw20_base::msg::InstantiateMarketingInfo>,
        slippage_tolerance_bps: u64,
        cross_chain_config: crate::msgs::cross_chain_config::CrossChainConfig,
    },

    /// Called by main factory to delegate a CP/Stable add-liquidity request.
    /// Main factory has already deposited the funds to escrow and generated
    /// `tx_id`. Pool factory records the pending entry, builds the outbound
    /// `RouterReceiveMsg::AddLiquidity` packet via `outbound`, and
    /// returns it via `Response::data` typed as `PoolFactoryReply::SendPacket`
    /// for main factory's reply handler to dispatch.
    OnAddLiquidity {
        tx_id: String,
        sender: Addr,
        pair_with_denom_and_amount: PairWithDenomAndAmount,
        slippage_tolerance_bps: u64,
        cross_chain_config: CrossChainConfig,
    },

    /// Called by main factory to delegate a CP/Stable remove-liquidity
    /// request. Main factory has already received the LP cw20 tokens via the
    /// `cw20::Send` hook (factory now holds them) and generated `tx_id`.
    /// Pool factory records the pending entry, builds the outbound
    /// `RouterReceiveMsg::RemoveLiquidity` packet via `outbound`,
    /// and returns it via `Response::data` typed as
    /// `PoolFactoryReply::SendPacket` for main factory's reply handler to
    /// dispatch.
    OnRemoveLiquidity {
        tx_id: String,
        sender: Addr,
        pair: Pair,
        lp_allocation: Uint256,
        lp_token: Addr,
        recipient: CrossChainUser,
        cross_chain_config: CrossChainConfig,
    },

    /// Called by main factory to delegate a CLP (concentrated) pool creation
    /// request. Main factory has validated the request and generated `tx_id`.
    /// Pool factory records the pending entry, builds the outbound
    /// `RouterReceiveMsg::RequestConcentratedPoolCreation` packet via
    /// `outbound`, and returns it via `Response::data` typed as
    /// `PoolFactoryReply::SendPacket` for main factory's reply handler to
    /// dispatch.
    OnRequestConcentratedPoolCreation {
        tx_id: String,
        sender: Addr,
        pair_with_denom_and_amount: PairWithDenomAndAmount,
        pool_key: PoolKey,
        slippage_tolerance_bps: u64,
        initial_tick: Option<i64>,
        cross_chain_config: CrossChainConfig,
    },

    /// Called by main factory after an IBC ack arrives for a pool variant.
    /// `original_msg` is the originally sent `RouterReceiveMsg`
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
        /// Mirror of main factory's CLP `POOL_KEY_TO_VLP`. Optional in the
        /// Slice 4 carry-over shape so existing Slice 1–3 migration tests
        /// continue to pass with the empty default; Slice 8 will require
        /// non-empty entries for chains that have CLP pools.
        concentrated_vlps: Option<Vec<(PoolKey, String)>>,
        /// Mirror of main factory's singleton position-token NFT contract.
        /// Optional for the same reason as `concentrated_vlps`.
        position_token_contract: Option<Addr>,
    },
}

#[cw_serde]
#[derive(cw_orch::QueryFns, QueryResponses)]
#[cfg_attr(feature = "cross-vm", derive(cross_vm_macros::CwQueryFns))]
#[cfg_attr(feature = "cross-vm", cross_vm(trait_name = "PoolFactoryQueryFns"))]
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

    /// Returns the VLP address for a given concentrated pool key.
    #[returns(GetConcentratedVlpResponse)]
    GetConcentratedVlp { pool_key: PoolKey },

    /// Returns the singleton position-token NFT contract address recorded on
    /// pool factory. May be `None` while the Slice 4 carry-over keeps main
    /// factory authoritative.
    #[returns(PositionTokenContractResponse)]
    GetPositionTokenContract {},

    #[returns(crate::build_info::BuildInfoResponse)]
    GetBuildInfo {},
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
pub struct GetConcentratedVlpResponse {
    pub vlp_address: Option<String>,
    pub pool_key: PoolKey,
}

#[cw_serde]
pub struct PositionTokenContractResponse {
    pub position_token_contract: Option<Addr>,
}

#[cw_serde]
pub struct MigrateMsg {}
