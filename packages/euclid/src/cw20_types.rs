use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Binary, Uint128};
use cw_utils::Expiration;

// ── Core receive message ───────────────────────────────────────────────────────

#[cw_serde]
pub struct Cw20ReceiveMsg {
    pub sender: String,
    pub amount: Uint128,
    pub msg: Binary,
}

// ── InstantiateMsg helper types ───────────────────────────────────────────────

#[cw_serde]
pub struct Cw20Coin {
    pub address: String,
    pub amount: Uint128,
}

#[cw_serde]
pub enum EmbeddedLogo {
    Svg(Binary),
    Png(Binary),
}

#[cw_serde]
pub enum Logo {
    Url(String),
    Embedded(EmbeddedLogo),
}

#[cw_serde]
pub struct MinterResponse {
    pub minter: String,
    pub cap: Option<Uint128>,
}

#[cw_serde]
pub struct InstantiateMarketingInfo {
    pub project: Option<String>,
    pub description: Option<String>,
    pub marketing: Option<String>,
    pub logo: Option<Logo>,
}

// ── ExecuteMsg (mirrors cw20-base JSON format) ────────────────────────────────

#[cw_serde]
pub enum Cw20ExecuteMsg {
    Transfer {
        recipient: String,
        amount: Uint128,
    },
    Burn {
        amount: Uint128,
    },
    Send {
        contract: String,
        amount: Uint128,
        msg: Binary,
    },
    IncreaseAllowance {
        spender: String,
        amount: Uint128,
        expires: Option<Expiration>,
    },
    DecreaseAllowance {
        spender: String,
        amount: Uint128,
        expires: Option<Expiration>,
    },
    TransferFrom {
        owner: String,
        recipient: String,
        amount: Uint128,
    },
    SendFrom {
        owner: String,
        contract: String,
        amount: Uint128,
        msg: Binary,
    },
    BurnFrom {
        owner: String,
        amount: Uint128,
    },
    Mint {
        recipient: String,
        amount: Uint128,
    },
    UpdateMarketing {
        project: Option<String>,
        description: Option<String>,
        marketing: Option<String>,
    },
    UploadLogo(Logo),
}

// ── InstantiateMsg / QueryMsg stubs ──────────────────────────────────────────

#[cw_serde]
pub struct Cw20InstantiateMsg {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub initial_balances: Vec<Cw20Coin>,
    pub mint: Option<MinterResponse>,
    pub marketing: Option<InstantiateMarketingInfo>,
}

#[cw_serde]
pub enum Cw20QueryMsg {
    Balance { address: String },
    TokenInfo {},
    Minter {},
    Allowance { owner: String, spender: String },
    AllAllowances {
        owner: String,
        start_after: Option<String>,
        limit: Option<u32>,
    },
    AllAccounts {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    MarketingInfo {},
    DownloadLogo {},
}

// ── Query response types ──────────────────────────────────────────────────────

#[cw_serde]
pub struct BalanceResponse {
    pub balance: Uint128,
}

#[cw_serde]
pub struct TokenInfoResponse {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_supply: Uint128,
}

#[cw_serde]
pub struct AllowanceResponse {
    pub allowance: Uint128,
    pub expires: Expiration,
}

#[cw_serde]
pub struct AllowanceInfo {
    pub spender: String,
    pub allowance: Uint128,
    pub expires: Expiration,
}

#[cw_serde]
pub struct AllAllowancesResponse {
    pub allowances: Vec<AllowanceInfo>,
}

#[cw_serde]
pub struct AllAccountsResponse {
    pub accounts: Vec<String>,
}

#[cw_serde]
pub enum LogoInfo {
    Url(String),
    Embedded,
}

#[cw_serde]
pub struct MarketingInfoResponse {
    pub project: Option<String>,
    pub description: Option<String>,
    pub marketing: Option<Addr>,
    pub logo: Option<LogoInfo>,
}

#[cw_serde]
pub struct DownloadLogoResponse {
    pub mime_type: String,
    pub data: Binary,
}
