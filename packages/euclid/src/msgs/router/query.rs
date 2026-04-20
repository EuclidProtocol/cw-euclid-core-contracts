use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Uint128};

use crate::{
    admin::EuclidAdmin,
    chain::{Chain, ChainUid},
    msgs::vlp::{base::PoolKey, concentrated::msg::PositionResponse},
    swap::NextSwapPair,
    token::{Pair, Token, TokenType},
    utils::pagination::Pagination,
};

#[cw_serde]
#[derive(cw_orch::QueryFns, QueryResponses)]
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

    #[returns(TokenEscrowsResponse)]
    QueryTokenEscrows {
        token: Token,
        pagination: Pagination<ChainUid>,
    },
    #[returns(AllEscrowsResponse)]
    QueryAllEscrows { pagination: Pagination<String> },

    #[returns(AllTokensResponse)]
    QueryAllTokens { pagination: Pagination<Token> },

    #[returns(QueryTokenDenomsResponse)]
    QueryTokenDenoms { token: Token },

    #[returns(QueryRelayerAddressesResponse)]
    QueryRelayerAddresses {},
    #[returns(ReleaseFeesQueryResponse)]
    GetReleaseFees {
        pagination: Pagination<(Token, ChainUid)>,
    },
    #[returns(ClpPositionInfoResponse)]
    GetClpPositionInfo { position_id: Uint128 },
}

#[cw_serde]
pub struct QuerySimulateSwap {
    pub asset_in: Token,
    pub amount_in: Uint128,
    pub asset_out: Token,
    pub min_amount_out: Uint128,
    pub swaps: Vec<NextSwapPair>,
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
    pub amount_out: Uint128,
    pub asset_out: Token,
}

#[cw_serde]
pub struct TokenEscrowsResponse {
    pub chains: Vec<TokenEscrowChainResponse>,
}

#[cw_serde]
pub struct TokenEscrowChainResponse {
    pub chain_uid: ChainUid,
    pub balance: Uint128,
}

#[cw_serde]
pub struct EscrowResponse {
    pub token: Token,
    pub chain_uid: ChainUid,
    pub balance: Uint128,
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
    pub fee: Uint128,
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
