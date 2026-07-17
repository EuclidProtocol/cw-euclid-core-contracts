// `QueryMsg::GetAllEscrows` is `#[deprecated]` (kept only for migration), but the
// `QueryFns`/`QueryResponses` derives generate sibling impls that enumerate every
// variant and so re-reference it. A variant- or enum-level `#[allow]` can't reach
// that generated code, so the allow lives at module scope. External callers still
// get the deprecation warning when they use the variant in their own crates.
#![allow(deprecated)]

use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Uint128, Uint256};

use crate::{
    admin::EuclidAdmin,
    chain::{Chain, ChainUid},
    cross_chain_user::CrossChainUser,
    msgs::vlp::{base::PoolKey, concentrated::msg::PositionResponse},
    swap::NextSwapPair,
    token::{Pair, Token, TokenType},
    utils::pagination::Pagination,
};

#[cw_serde]
#[derive(cw_orch::QueryFns, QueryResponses)]
#[cfg_attr(feature = "cross-vm", derive(cross_vm_macros::CwQueryFns))]
#[cfg_attr(feature = "cross-vm", cross_vm(trait_name = "RouterQueryFns"))]
pub enum QueryMsg {
    #[returns(StateResponse)]
    GetState {},
    #[returns(ChainResponse)]
    GetChain { chain_uid: ChainUid },
    #[returns(AllChainResponse)]
    GetAllChains {},
    #[returns(VlpResponse)]
    GetVlp { pair: Pair },
    #[returns(PoolKeyVlpResponse)]
    GetVlpByPoolKey { pool_key: PoolKey },
    #[returns(AllVlpResponse)]
    GetAllVlps {
        pagination: Pagination<(String, String)>,
    },
    #[returns(SimulateSwapResponse)]
    SimulateSwap(QuerySimulateSwap),

    #[returns(QueryRelayerAddressesResponse)]
    QueryRelayerAddresses {},
    #[returns(ReleaseFeesQueryResponse)]
    GetReleaseFees {
        pagination: Pagination<(Token, ChainUid)>,
    },
    #[returns(ClpPositionInfoResponse)]
    GetClpPositionInfo { position_id: Uint128 },

    #[deprecated(note = "ESCROW_BALANCES moved to virtual_balance. Used only during migration.")]
    #[returns(AllEscrowsResponse)]
    GetAllEscrows {},
    #[returns(LockedChainsResponse)]
    GetLockedChains {},
    #[returns(FeeStateResponse)]
    GetFeeState {},
    #[returns(DefaultReleaseFeeResponse)]
    GetDefaultReleaseFee {},
    #[returns(ChainTimeoutResponse)]
    GetChainTimeout { chain_uid: ChainUid },
    #[returns(EuclidFeeOverrideResponse)]
    GetEuclidFeeOverride { user: CrossChainUser },
    #[returns(crate::build_info::BuildInfoResponse)]
    GetBuildInfo {},
}

#[cw_serde]
pub struct QuerySimulateSwap {
    pub asset_in: Token,
    pub amount_in: Uint256,
    pub asset_out: Token,
    pub min_amount_out: Uint256,
    pub swaps: Vec<NextSwapPair>,
    /// Optional swapping wallet. When present, the Router resolves its
    /// per-wallet Euclid-fee override and threads it through the simulation so
    /// the quoted Euclid fee equals what execution would charge. Absent (the
    /// default for older callers) keeps the current full-fee quote.
    #[serde(default)]
    pub sender: Option<CrossChainUser>,
}

#[cw_serde]
pub struct StateResponse {
    pub admins: EuclidAdmin,
    pub constant_product_vlp_code_id: u64,
    pub stable_vlp_code_id: u64,
    pub concentrated_vlp_code_id: u64,
    pub virtual_balance_address: Addr,
    pub locked: bool,
}

#[cw_serde]
pub struct AllVlpResponse {
    pub vlps: Vec<VlpResponse>,
}

#[cw_serde]
pub struct VlpResponse {
    pub vlp: String,
    pub token_1: Token,
    pub token_2: Token,
    pub pool_key: Option<PoolKey>,
}

#[cw_serde]
pub struct PoolKeyVlpResponse {
    pub vlp: String,
    pub pool_key: PoolKey,
}

#[cw_serde]
pub struct ChainResponse {
    pub chain: Chain,
    pub chain_uid: ChainUid,
}

#[cw_serde]
pub struct AllChainResponse {
    pub chains: Vec<ChainResponse>,
}

#[cw_serde]
pub struct SimulateSwapResponse {
    pub amount_out: Uint256,
    pub asset_out: Token,
}

#[cw_serde]
pub struct TokenEscrowsResponse {
    pub chains: Vec<TokenEscrowChainResponse>,
}

#[cw_serde]
pub struct TokenEscrowChainResponse {
    pub chain_uid: ChainUid,
    pub balance: Uint256,
}

#[cw_serde]
pub struct EscrowResponse {
    pub token: Token,
    pub chain_uid: ChainUid,
    pub balance: Uint256,
}

#[cw_serde]
pub struct AllEscrowsResponse {
    pub escrows: Vec<EscrowResponse>,
}

#[cw_serde]
pub struct AllTokensResponse {
    pub tokens: Vec<Token>,
}

#[cw_serde]
pub struct ReleaseFee {
    pub token: Token,
    pub chain_uid: ChainUid,
    pub fee: Uint256,
}

#[cw_serde]
pub struct ReleaseFeesQueryResponse {
    pub fees: Vec<ReleaseFee>,
}
#[cw_serde]
pub struct GetVlpResponse {
    pub vlp_address: String,
}

#[cw_serde]
pub struct TokenDenom {
    pub chain_uid: ChainUid,
    pub token_type: TokenType,
}

#[cw_serde]
pub struct QueryTokenDenomsResponse {
    pub denoms: Vec<TokenDenom>,
}

#[cw_serde]
pub struct QueryRelayerAddressesResponse {
    pub relayer_contract: Addr,
}

#[cw_serde]
pub struct ClpPositionInfoResponse {
    pub vlp_address: String,
    pub position: PositionResponse,
}

#[cw_serde]
pub struct LockedChainsResponse {
    pub chains: Vec<ChainUid>,
}

#[cw_serde]
pub struct FeeStateResponse {
    pub release_fee_recipient: Addr,
    pub default_fee_recipient: Addr,
}

#[cw_serde]
pub struct DefaultReleaseFeeResponse {
    pub fee: Uint256,
}

#[cw_serde]
pub struct ChainTimeoutResponse {
    pub chain_uid: ChainUid,
    pub timeout_seconds: u64,
}

#[cw_serde]
pub struct EuclidFeeOverrideResponse {
    /// `Some(bps)` if a per-wallet override is set; `None` means the wallet
    /// uses the pool's configured Euclid fee.
    pub euclid_fee_bps: Option<u64>,
}

#[cfg(test)]
mod euclid_fee_override_rollout_tests {
    //! SC-23 Issue 7 — graceful-fallback rollout safety for the simulate path.
    //! An old caller that omits `sender` decodes to `None` (full-fee quote, the
    //! current behavior); a new caller's `sender` decodes normally. No error.
    use super::*;
    use cosmwasm_std::from_json;

    #[test]
    fn simulate_swap_query_without_sender_defaults_to_none() {
        let legacy = br#"{
            "asset_in": "usdc",
            "amount_in": "1000",
            "asset_out": "eth",
            "min_amount_out": "1",
            "swaps": []
        }"#;
        let msg: QuerySimulateSwap =
            from_json(legacy).expect("legacy QuerySimulateSwap must decode");
        assert_eq!(msg.sender, None);
    }

    #[test]
    fn simulate_swap_query_with_sender_decodes() {
        let modern = br#"{
            "asset_in": "usdc",
            "amount_in": "1000",
            "asset_out": "eth",
            "min_amount_out": "1",
            "swaps": [],
            "sender": {"chain_uid": "chaina", "address": "addr1"}
        }"#;
        let msg: QuerySimulateSwap =
            from_json(modern).expect("modern QuerySimulateSwap must decode");
        assert_eq!(
            msg.sender,
            Some(CrossChainUser::new(
                ChainUid::create("chaina".to_string()).unwrap(),
                "addr1".to_string()
            ))
        );
    }
}
