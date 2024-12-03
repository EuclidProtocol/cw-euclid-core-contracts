use cosmwasm_schema::cw_serde;
use cosmwasm_schema::QueryResponses;
use cosmwasm_std::{Addr, Binary, Uint128};
use secret_toolkit::utils::InitCallback;
use snip20_reference_impl::msg::{
    ExecuteMsg as Snip20ExecuteMsg, InstantiateMsg as Snip20InstantiateMsg,
    QueryMsg as Snip20QueryMsg,
};
use snip20_reference_impl::msg::{InitConfig, InitialBalance};

use crate::token::Pair;

#[cw_serde]
pub struct InstantiateMsg {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub initial_balances: Vec<InitialBalance>,
    pub admin: Option<String>,
    pub prng_seed: Binary,
    pub config: Option<InitConfig>,
    pub supported_denoms: Option<Vec<String>>,
    // Custom Params
    pub vlp: String,
    pub factory: Addr,
    pub token_pair: Pair,
}

impl InitCallback for InstantiateMsg {
    const BLOCK_SIZE: usize = 256;
}

impl From<InstantiateMsg> for Snip20InstantiateMsg {
    fn from(msg: InstantiateMsg) -> Self {
        Snip20InstantiateMsg {
            name: msg.name,
            symbol: msg.symbol,
            decimals: msg.decimals,
            initial_balances: Some(msg.initial_balances),
            admin: msg.admin,
            prng_seed: msg.prng_seed,
            config: msg.config,
            supported_denoms: msg.supported_denoms,
        }
    }
}

#[cw_serde]
pub enum ExecuteMsg {
    UpdateState {
        token_pair: Option<Pair>,
        factory_address: Option<Addr>,
        vlp: Option<String>,
    },
    /// Transfer is a base message to move tokens to another account without triggering actions
    Transfer {
        recipient: String,
        amount: Uint128,
        memo: Option<String>,
        decoys: Option<Vec<Addr>>,
        entropy: Option<Binary>,
        padding: Option<String>,
    },
    /// Burn is a base message to destroy tokens forever
    Burn {
        amount: Uint128,
        memo: Option<String>,
        decoys: Option<Vec<Addr>>,
        entropy: Option<Binary>,
        padding: Option<String>,
    },
    /// Send is a base message to transfer tokens to a contract and trigger an action
    /// on the receiving contract.
    Send {
        recipient: String,
        recipient_code_hash: Option<String>,
        amount: Uint128,
        msg: Option<Binary>,
        memo: Option<String>,
        decoys: Option<Vec<Addr>>,
        entropy: Option<Binary>,
        padding: Option<String>,
    },
    /// Only with "approval" extension. Allows spender to access an additional amount tokens
    /// from the owner's (env.sender) account. If expires is Some(), overwrites current allowance
    /// expiration with this one.
    IncreaseAllowance {
        spender: String,
        amount: Uint128,
        expiration: Option<u64>,
        padding: Option<String>,
    },
    /// Only with "approval" extension. Lowers the spender's access of tokens
    /// from the owner's (env.sender) account by amount. If expires is Some(), overwrites current
    /// allowance expiration with this one.
    DecreaseAllowance {
        spender: String,
        amount: Uint128,
        expiration: Option<u64>,
        padding: Option<String>,
    },
    /// Only with "approval" extension. Transfers amount tokens from owner -> recipient
    /// if `env.sender` has sufficient pre-approval.
    TransferFrom {
        owner: String,
        recipient: String,
        amount: Uint128,
        memo: Option<String>,
        decoys: Option<Vec<Addr>>,
        entropy: Option<Binary>,
        padding: Option<String>,
    },
    /// Only with "approval" extension. Sends amount tokens from owner -> contract
    /// if `env.sender` has sufficient pre-approval.
    SendFrom {
        owner: String,
        recipient: String,
        recipient_code_hash: Option<String>,
        amount: Uint128,
        msg: Option<Binary>,
        memo: Option<String>,
        decoys: Option<Vec<Addr>>,
        entropy: Option<Binary>,
        padding: Option<String>,
    },
    /// Only with "approval" extension. Destroys tokens forever
    BurnFrom {
        owner: String,
        amount: Uint128,
        memo: Option<String>,
        decoys: Option<Vec<Addr>>,
        entropy: Option<Binary>,
        padding: Option<String>,
    },
    /// Only with the "mintable" extension. If authorized, creates amount new tokens
    /// and adds to the recipient balance.
    Mint {
        recipient: String,
        amount: Uint128,
        memo: Option<String>,
        decoys: Option<Vec<Addr>>,
        entropy: Option<Binary>,
        padding: Option<String>,
    },
    // Only with the "marketing" extension. If authorized, updates marketing metadata.
    // Setting None/null for any of these will leave it unchanged.
    // Setting Some("") will clear this field on the contract storage
    // UpdateMarketing {
    //     /// A URL pointing to the project behind this token.
    //     project: Option<String>,
    //     /// A longer description of the token and it's utility. Designed for tooltips or such
    //     description: Option<String>,
    //     /// The address (if any) who can update this data structure
    //     marketing: Option<String>,
    // },
    // If set as the "marketing" role on the contract, upload a new URL, SVG, or PNG for the token
    // UploadLogo(Logo),
}

impl From<ExecuteMsg> for Snip20ExecuteMsg {
    fn from(msg: ExecuteMsg) -> Self {
        match msg {
            ExecuteMsg::Transfer {
                recipient,
                amount,
                memo,
                decoys,
                entropy,
                padding,
            } => Snip20ExecuteMsg::Transfer {
                recipient: recipient.to_string(),
                amount,
                memo,
                decoys,
                entropy,
                padding,
            },
            ExecuteMsg::Burn {
                amount,
                memo,
                decoys,
                entropy,
                padding,
            } => Snip20ExecuteMsg::Burn {
                amount,
                memo,
                decoys,
                entropy,
                padding,
            },
            ExecuteMsg::Send {
                recipient,
                recipient_code_hash,
                amount,
                msg,
                memo,
                decoys,
                entropy,
                padding,
            } => Snip20ExecuteMsg::Send {
                amount,
                msg,
                recipient,
                recipient_code_hash,
                memo,
                decoys,
                entropy,
                padding,
            },
            ExecuteMsg::IncreaseAllowance {
                spender,
                amount,
                expiration,
                padding,
            } => Snip20ExecuteMsg::IncreaseAllowance {
                spender,
                amount,
                expiration,
                padding,
            },
            ExecuteMsg::DecreaseAllowance {
                spender,
                amount,
                expiration,
                padding,
            } => Snip20ExecuteMsg::DecreaseAllowance {
                spender,
                amount,
                expiration,
                padding,
            },
            ExecuteMsg::TransferFrom {
                owner,
                recipient,
                amount,
                memo,
                decoys,
                entropy,
                padding,
            } => Snip20ExecuteMsg::TransferFrom {
                owner,
                recipient,
                amount,
                memo,
                decoys,
                entropy,
                padding,
            },
            ExecuteMsg::SendFrom {
                owner,
                recipient,
                recipient_code_hash,
                amount,
                msg,
                memo,
                decoys,
                entropy,
                padding,
            } => Snip20ExecuteMsg::SendFrom {
                owner,
                amount,
                msg,
                recipient,
                recipient_code_hash,
                memo,
                decoys,
                entropy,
                padding,
            },
            ExecuteMsg::BurnFrom {
                owner,
                amount,
                memo,
                decoys,
                entropy,
                padding,
            } => Snip20ExecuteMsg::BurnFrom {
                owner,
                amount,
                memo,
                decoys,
                entropy,
                padding,
            },
            ExecuteMsg::Mint {
                recipient,
                amount,
                memo,
                decoys,
                entropy,
                padding,
            } => Snip20ExecuteMsg::Mint {
                recipient,
                amount,
                memo,
                decoys,
                entropy,
                padding,
            },
            // ExecuteMsg::UpdateMarketing {
            //     project,
            //     description,
            //     marketing,
            // } => Snip20ExecuteMsg::UpdateMarketing {
            //     project,
            //     description,
            //     marketing,
            // },
            _ => panic!("Unsupported message"),
        }
    }
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    //NOTE: Balance is included in andr_query
    /// Returns the current balance of the given address, 0 if unset.
    /// Return type: BalanceResponse.
    #[returns(snip20_reference_impl::msg::QueryAnswer)]
    Balance { address: String, key: String },
    /// Returns metadata on the contract - name, decimals, supply, etc.
    /// Return type: TokenInfoResponse.    #[returns(BalanceResponse)]
    #[returns(snip20_reference_impl::msg::QueryAnswer)]
    TokenInfo {},
    /// Only with "mintable" extension.
    /// Returns who can mint and the hard cap on maximum tokens after minting.
    /// Return type: MinterResponse.
    #[returns(snip20_reference_impl::msg::QueryAnswer)]
    Minters {},
    /// Only with "allowance" extension.
    /// Returns how much spender can use from owner account, 0 if unset.
    /// Return type: AllowanceResponse.
    #[returns(snip20_reference_impl::msg::QueryAnswer)]
    Allowance {
        owner: String,
        spender: String,
        key: String,
    },
    /// Only with "enumerable" extension (and "allowances")
    /// Returns all allowances this owner has approved. Supports pagination.
    /// Return type: AllAllowancesResponse.
    #[returns(snip20_reference_impl::msg::QueryAnswer)]
    AllAllowances {
        owner: String,
        key: String,
        page: Option<u32>,
        page_size: u32,
    },
    // Only with "enumerable" extension
    // Returns all accounts that have balances. Supports pagination.
    // Return type: AllAccountsResponse.
    // AllAccounts {
    //     start_after: Option<String>,
    //     limit: Option<u32>,
    // },
    // Only with "marketing" extension
    // Returns more metadata on the contract to display in the client:
    // - description, logo, project url, etc.
    // Return type: MarketingInfoResponse
    // MarketingInfo {},
    // Only with "marketing" extension
    // Downloads the mbeded logo data (if stored on chain). Errors if no logo data ftored for this
    // contract.
    // Return type: DownloadLogoResponse.
    // DownloadLogo {},
    // Balance { address: String },: todo!()
}

impl From<QueryMsg> for Snip20QueryMsg {
    fn from(msg: QueryMsg) -> Self {
        match msg {
            QueryMsg::Balance { address, key } => Snip20QueryMsg::Balance { address, key },
            QueryMsg::TokenInfo {} => Snip20QueryMsg::TokenInfo {},
            QueryMsg::Minters {} => Snip20QueryMsg::Minters {},
            QueryMsg::Allowance {
                owner,
                spender,
                key,
            } => Snip20QueryMsg::Allowance {
                owner,
                spender,
                key,
            },
            QueryMsg::AllAllowances {
                owner,
                key,
                page,
                page_size,
            } => Snip20QueryMsg::AllowancesGiven {
                owner,
                key,
                page,
                page_size,
            },
            // QueryMsg::AllAccounts { start_after, limit } => {
            //     Snip20QueryMsg::AllAccounts { start_after, limit }
            // }
            // QueryMsg::MarketingInfo {} => Snip20QueryMsg::MarketingInfo {},
            // QueryMsg::DownloadLogo {} => Snip20QueryMsg::DownloadLogo {},
        }
    }
}
