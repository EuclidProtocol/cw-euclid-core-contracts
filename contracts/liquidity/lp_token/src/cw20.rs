use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    ensure, to_json_binary, Addr, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Order,
    Response, StdResult, Uint128, WasmMsg,
};
use cw_storage_plus::{Bound, Item, Map};
use cw_utils::Expiration;
use euclid::cw20_types::Cw20ReceiveMsg;
use euclid::{
    cw20_types::{
        AllAccountsResponse, AllAllowancesResponse, AllowanceInfo, AllowanceResponse,
        BalanceResponse, DownloadLogoResponse, EmbeddedLogo, InstantiateMarketingInfo, Logo,
        LogoInfo, MarketingInfoResponse, MinterResponse, TokenInfoResponse,
    },
    error::ContractError,
    msgs::lp_token::msg::InstantiateMsg,
};

// ---------------------------------------------------------------------------
// Storage types
// ---------------------------------------------------------------------------

#[cw_serde]
pub struct TokenInfo {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_supply: Uint128,
}

#[cw_serde]
pub struct MinterData {
    pub minter: Addr,
    pub cap: Option<Uint128>,
}

#[cw_serde]
pub struct AllowanceData {
    pub allowance: Uint128,
    pub expires: Expiration,
}

#[cw_serde]
pub struct MarketingData {
    pub project: Option<String>,
    pub description: Option<String>,
    pub marketing: Option<Addr>,
    pub logo: Option<LogoInfo>,
}

#[cw_serde]
pub struct LogoData {
    pub mime_type: String,
    pub data: Binary,
}

// Internal wrapper to serialize {"receive": {...}} for Send operations
#[cw_serde]
enum ReceiveWrapper {
    Receive(Cw20ReceiveMsg),
}

// Use the same storage keys as cw20-base for on-chain compatibility
pub const TOKEN_INFO: Item<TokenInfo> = Item::new("token_info");
pub const MINT: Item<Option<MinterData>> = Item::new("mint");
pub const BALANCES: Map<&Addr, Uint128> = Map::new("balance");
pub const ALLOWANCES: Map<(&Addr, &Addr), AllowanceData> = Map::new("allowance");
pub const ALLOWANCES_SPENDER: Map<(&Addr, &Addr), AllowanceData> = Map::new("allowance_spender");
pub const MARKETING: Item<MarketingData> = Item::new("marketing_info");
pub const LOGO: Item<LogoData> = Item::new("logo");

// ---------------------------------------------------------------------------
// Instantiate
// ---------------------------------------------------------------------------

pub fn instantiate_cw20(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: &InstantiateMsg,
) -> Result<Response, ContractError> {
    let token_info = TokenInfo {
        name: msg.name.clone(),
        symbol: msg.symbol.clone(),
        decimals: msg.decimals,
        total_supply: Uint128::zero(),
    };
    TOKEN_INFO.save(deps.storage, &token_info)?;

    let mint_data = msg
        .mint
        .as_ref()
        .map(|m| {
            Ok::<_, ContractError>(MinterData {
                minter: deps.api.addr_validate(&m.minter)?,
                cap: m.cap,
            })
        })
        .transpose()?;
    MINT.save(deps.storage, &mint_data)?;

    // Mint initial balances
    let mut total_supply = Uint128::zero();
    for coin in &msg.initial_balances {
        let addr = deps.api.addr_validate(&coin.address)?;
        BALANCES.update(deps.storage, &addr, |bal| -> StdResult<_> {
            Ok(bal.unwrap_or_default().checked_add(coin.amount)?)
        })?;
        total_supply = total_supply.checked_add(coin.amount)?;
    }

    // Check against cap
    if let Some(Some(minter)) = MINT.may_load(deps.storage)? {
        if let Some(cap) = minter.cap {
            ensure!(
                total_supply <= cap,
                ContractError::new("Initial supply exceeds minter cap")
            );
        }
    }

    TOKEN_INFO.update(deps.storage, |mut info| -> StdResult<_> {
        info.total_supply = total_supply;
        Ok(info)
    })?;

    // Marketing
    if let Some(marketing) = &msg.marketing {
        save_marketing(deps, marketing)?;
    }

    Ok(Response::default())
}

fn save_marketing(
    deps: DepsMut,
    marketing: &InstantiateMarketingInfo,
) -> Result<(), ContractError> {
    let logo_info = marketing.logo.as_ref().map(|logo| match logo {
        Logo::Url(url) => LogoInfo::Url(url.clone()),
        Logo::Embedded(_) => LogoInfo::Embedded,
    });

    let marketing_addr = marketing
        .marketing
        .as_ref()
        .map(|m| deps.api.addr_validate(m))
        .transpose()?;

    MARKETING.save(
        deps.storage,
        &MarketingData {
            project: marketing.project.clone(),
            description: marketing.description.clone(),
            marketing: marketing_addr,
            logo: logo_info,
        },
    )?;

    if let Some(Logo::Embedded(embedded)) = &marketing.logo {
        let logo_data = match embedded {
            EmbeddedLogo::Png(data) => LogoData {
                mime_type: "image/png".to_string(),
                data: data.clone(),
            },
            EmbeddedLogo::Svg(data) => LogoData {
                mime_type: "image/svg+xml".to_string(),
                data: data.clone(),
            },
        };
        LOGO.save(deps.storage, &logo_data)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Execute
// ---------------------------------------------------------------------------

pub fn execute_transfer(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    recipient: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    ensure!(
        !amount.is_zero(),
        ContractError::new("Transfer amount must be non-zero")
    );
    let recipient_addr = deps.api.addr_validate(&recipient)?;
    transfer_tokens(deps, &info.sender, &recipient_addr, amount)?;
    Ok(Response::new()
        .add_attribute("action", "transfer")
        .add_attribute("from", info.sender)
        .add_attribute("to", recipient)
        .add_attribute("amount", amount))
}

pub fn execute_burn(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    ensure!(
        !amount.is_zero(),
        ContractError::new("Burn amount must be non-zero")
    );
    deduct_balance(deps.storage, &info.sender, amount)?;
    TOKEN_INFO.update(deps.storage, |mut info| -> StdResult<_> {
        info.total_supply = info.total_supply.checked_sub(amount)?;
        Ok(info)
    })?;
    Ok(Response::new()
        .add_attribute("action", "burn")
        .add_attribute("from", info.sender)
        .add_attribute("amount", amount))
}

pub fn execute_mint(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    recipient: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    ensure!(
        !amount.is_zero(),
        ContractError::new("Mint amount must be non-zero")
    );
    let minter = MINT.load(deps.storage)?;
    let minter = minter.ok_or_else(|| ContractError::new("No minter configured"))?;
    ensure!(info.sender == minter.minter, ContractError::Unauthorized {});

    let mut token_info = TOKEN_INFO.load(deps.storage)?;
    token_info.total_supply = token_info.total_supply.checked_add(amount)?;
    if let Some(cap) = minter.cap {
        ensure!(
            token_info.total_supply <= cap,
            ContractError::new("Minting would exceed cap")
        );
    }
    TOKEN_INFO.save(deps.storage, &token_info)?;

    let recipient_addr = deps.api.addr_validate(&recipient)?;
    BALANCES.update(deps.storage, &recipient_addr, |bal| -> StdResult<_> {
        Ok(bal.unwrap_or_default().checked_add(amount)?)
    })?;

    Ok(Response::new()
        .add_attribute("action", "mint")
        .add_attribute("to", recipient)
        .add_attribute("amount", amount))
}

pub fn execute_send(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    contract: String,
    amount: Uint128,
    msg: Binary,
) -> Result<Response, ContractError> {
    ensure!(
        !amount.is_zero(),
        ContractError::new("Send amount must be non-zero")
    );
    let contract_addr = deps.api.addr_validate(&contract)?;
    transfer_tokens(deps, &info.sender, &contract_addr, amount)?;

    let receive_msg = Cw20ReceiveMsg {
        sender: info.sender.to_string(),
        amount,
        msg,
    };

    // Call Receive on the target contract ({"receive": {...}})
    let execute_msg = to_json_binary(&ReceiveWrapper::Receive(receive_msg))?;

    Ok(Response::new()
        .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: contract.clone(),
            msg: execute_msg,
            funds: vec![],
        }))
        .add_attribute("action", "send")
        .add_attribute("from", info.sender)
        .add_attribute("to", contract)
        .add_attribute("amount", amount))
}

pub fn execute_increase_allowance(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    spender: String,
    amount: Uint128,
    expires: Option<Expiration>,
) -> Result<Response, ContractError> {
    let spender_addr = deps.api.addr_validate(&spender)?;
    ensure!(
        info.sender != spender_addr,
        ContractError::new("Cannot set own allowance")
    );
    let expires = expires.unwrap_or_default();
    ensure!(
        !expires.is_expired(&env.block),
        ContractError::new("Allowance already expired")
    );

    ALLOWANCES.update(
        deps.storage,
        (&info.sender, &spender_addr),
        |existing| -> StdResult<_> {
            let mut data = existing.unwrap_or(AllowanceData {
                allowance: Uint128::zero(),
                expires: Expiration::Never {},
            });
            data.allowance = data.allowance.checked_add(amount)?;
            data.expires = expires;
            Ok(data)
        },
    )?;
    ALLOWANCES_SPENDER.update(
        deps.storage,
        (&spender_addr, &info.sender),
        |existing| -> StdResult<_> {
            let mut data = existing.unwrap_or(AllowanceData {
                allowance: Uint128::zero(),
                expires: Expiration::Never {},
            });
            data.allowance = data.allowance.checked_add(amount)?;
            data.expires = expires;
            Ok(data)
        },
    )?;

    Ok(Response::new()
        .add_attribute("action", "increase_allowance")
        .add_attribute("owner", info.sender)
        .add_attribute("spender", spender)
        .add_attribute("amount", amount))
}

pub fn execute_decrease_allowance(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    spender: String,
    amount: Uint128,
    expires: Option<Expiration>,
) -> Result<Response, ContractError> {
    let spender_addr = deps.api.addr_validate(&spender)?;
    ensure!(
        info.sender != spender_addr,
        ContractError::new("Cannot set own allowance")
    );
    let expires = expires.unwrap_or_default();
    ensure!(
        !expires.is_expired(&env.block),
        ContractError::new("Allowance already expired")
    );

    let key = (&info.sender, &spender_addr);
    let mut data = ALLOWANCES
        .load(deps.storage, key)
        .map_err(|_| ContractError::new("No allowance set"))?;

    if amount >= data.allowance {
        ALLOWANCES.remove(deps.storage, key);
        ALLOWANCES_SPENDER.remove(deps.storage, (&spender_addr, &info.sender));
    } else {
        data.allowance = data.allowance.checked_sub(amount)?;
        data.expires = expires;
        ALLOWANCES.save(deps.storage, key, &data)?;
        ALLOWANCES_SPENDER.save(deps.storage, (&spender_addr, &info.sender), &data)?;
    }

    Ok(Response::new()
        .add_attribute("action", "decrease_allowance")
        .add_attribute("owner", info.sender)
        .add_attribute("spender", spender)
        .add_attribute("amount", amount))
}

pub fn execute_transfer_from(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    owner: String,
    recipient: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    ensure!(
        !amount.is_zero(),
        ContractError::new("Transfer amount must be non-zero")
    );
    let owner_addr = deps.api.addr_validate(&owner)?;
    let recipient_addr = deps.api.addr_validate(&recipient)?;
    deduct_allowance(deps.storage, &env, &owner_addr, &info.sender, amount)?;
    transfer_tokens(deps, &owner_addr, &recipient_addr, amount)?;
    Ok(Response::new()
        .add_attribute("action", "transfer_from")
        .add_attribute("from", owner)
        .add_attribute("to", recipient)
        .add_attribute("by", info.sender)
        .add_attribute("amount", amount))
}

pub fn execute_send_from(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    owner: String,
    contract: String,
    amount: Uint128,
    msg: Binary,
) -> Result<Response, ContractError> {
    ensure!(
        !amount.is_zero(),
        ContractError::new("Send amount must be non-zero")
    );
    let owner_addr = deps.api.addr_validate(&owner)?;
    let contract_addr = deps.api.addr_validate(&contract)?;
    deduct_allowance(deps.storage, &env, &owner_addr, &info.sender, amount)?;
    transfer_tokens(deps, &owner_addr, &contract_addr, amount)?;

    let receive_msg = Cw20ReceiveMsg {
        sender: info.sender.to_string(),
        amount,
        msg,
    };

    Ok(Response::new()
        .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: contract.clone(),
            msg: to_json_binary(&ReceiveWrapper::Receive(receive_msg))?,
            funds: vec![],
        }))
        .add_attribute("action", "send_from")
        .add_attribute("from", owner)
        .add_attribute("to", contract)
        .add_attribute("by", info.sender)
        .add_attribute("amount", amount))
}

pub fn execute_burn_from(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    owner: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    ensure!(
        !amount.is_zero(),
        ContractError::new("Burn amount must be non-zero")
    );
    let owner_addr = deps.api.addr_validate(&owner)?;
    deduct_allowance(deps.storage, &env, &owner_addr, &info.sender, amount)?;
    deduct_balance(deps.storage, &owner_addr, amount)?;
    TOKEN_INFO.update(deps.storage, |mut info| -> StdResult<_> {
        info.total_supply = info.total_supply.checked_sub(amount)?;
        Ok(info)
    })?;
    Ok(Response::new()
        .add_attribute("action", "burn_from")
        .add_attribute("from", owner)
        .add_attribute("by", info.sender)
        .add_attribute("amount", amount))
}

pub fn execute_update_marketing(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    project: Option<String>,
    description: Option<String>,
    marketing: Option<String>,
) -> Result<Response, ContractError> {
    let mut data = MARKETING.may_load(deps.storage)?.unwrap_or(MarketingData {
        project: None,
        description: None,
        marketing: None,
        logo: None,
    });
    if let Some(ref marketing_addr) = data.marketing {
        ensure!(
            info.sender == *marketing_addr,
            ContractError::Unauthorized {}
        );
    } else {
        return Err(ContractError::Unauthorized {});
    }
    if project.is_some() {
        data.project = project;
    }
    if description.is_some() {
        data.description = description;
    }
    if let Some(m) = marketing {
        data.marketing = if m.is_empty() {
            None
        } else {
            Some(deps.api.addr_validate(&m)?)
        };
    }
    MARKETING.save(deps.storage, &data)?;
    Ok(Response::new().add_attribute("action", "update_marketing"))
}

pub fn execute_upload_logo(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    logo: Logo,
) -> Result<Response, ContractError> {
    let data = MARKETING.may_load(deps.storage)?.unwrap_or(MarketingData {
        project: None,
        description: None,
        marketing: None,
        logo: None,
    });
    if let Some(ref marketing_addr) = data.marketing {
        ensure!(
            info.sender == *marketing_addr,
            ContractError::Unauthorized {}
        );
    } else {
        return Err(ContractError::Unauthorized {});
    }

    let logo_info = match &logo {
        Logo::Url(url) => LogoInfo::Url(url.clone()),
        Logo::Embedded(_) => LogoInfo::Embedded,
    };
    MARKETING.save(
        deps.storage,
        &MarketingData {
            logo: Some(logo_info),
            ..data
        },
    )?;

    if let Logo::Embedded(embedded) = logo {
        let logo_data = match embedded {
            EmbeddedLogo::Png(data) => LogoData {
                mime_type: "image/png".to_string(),
                data,
            },
            EmbeddedLogo::Svg(data) => LogoData {
                mime_type: "image/svg+xml".to_string(),
                data,
            },
        };
        LOGO.save(deps.storage, &logo_data)?;
    }

    Ok(Response::new().add_attribute("action", "upload_logo"))
}

// ---------------------------------------------------------------------------
// Query
// ---------------------------------------------------------------------------

pub fn query_balance(deps: Deps, address: String) -> Result<Binary, ContractError> {
    let addr = deps.api.addr_validate(&address)?;
    let balance = BALANCES.may_load(deps.storage, &addr)?.unwrap_or_default();
    Ok(to_json_binary(&BalanceResponse { balance })?)
}

pub fn query_token_info(deps: Deps) -> Result<Binary, ContractError> {
    let info = TOKEN_INFO.load(deps.storage)?;
    Ok(to_json_binary(&TokenInfoResponse {
        name: info.name,
        symbol: info.symbol,
        decimals: info.decimals,
        total_supply: info.total_supply,
    })?)
}

pub fn query_minter(deps: Deps) -> Result<Binary, ContractError> {
    let minter = MINT.load(deps.storage)?;
    let resp = minter.map(|m| MinterResponse {
        minter: m.minter.to_string(),
        cap: m.cap,
    });
    Ok(to_json_binary(&resp)?)
}

pub fn query_allowance(
    deps: Deps,
    owner: String,
    spender: String,
) -> Result<Binary, ContractError> {
    let owner_addr = deps.api.addr_validate(&owner)?;
    let spender_addr = deps.api.addr_validate(&spender)?;
    let data = ALLOWANCES
        .may_load(deps.storage, (&owner_addr, &spender_addr))?
        .unwrap_or(AllowanceData {
            allowance: Uint128::zero(),
            expires: Expiration::Never {},
        });
    Ok(to_json_binary(&AllowanceResponse {
        allowance: data.allowance,
        expires: data.expires,
    })?)
}

const DEFAULT_LIMIT: u32 = 10;
const MAX_LIMIT: u32 = 30;

pub fn query_all_allowances(
    deps: Deps,
    owner: String,
    start_after: Option<String>,
    limit: Option<u32>,
) -> Result<Binary, ContractError> {
    let owner_addr = deps.api.addr_validate(&owner)?;
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start_after
        .map(|s| deps.api.addr_validate(&s))
        .transpose()?;
    let start = start.as_ref().map(Bound::exclusive);

    let allowances: Vec<AllowanceInfo> = ALLOWANCES
        .prefix(&owner_addr)
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (spender, data) = item?;
            Ok(AllowanceInfo {
                spender: spender.to_string(),
                allowance: data.allowance,
                expires: data.expires,
            })
        })
        .collect::<StdResult<_>>()?;

    Ok(to_json_binary(&AllAllowancesResponse { allowances })?)
}

pub fn query_all_accounts(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> Result<Binary, ContractError> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start_after
        .map(|s| deps.api.addr_validate(&s))
        .transpose()?;
    let start = start.as_ref().map(Bound::exclusive);

    let accounts: Vec<String> = BALANCES
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| Ok(item?.0.to_string()))
        .collect::<StdResult<_>>()?;

    Ok(to_json_binary(&AllAccountsResponse { accounts })?)
}

pub fn query_marketing_info(deps: Deps) -> Result<Binary, ContractError> {
    let data = MARKETING.may_load(deps.storage)?.unwrap_or(MarketingData {
        project: None,
        description: None,
        marketing: None,
        logo: None,
    });
    Ok(to_json_binary(&MarketingInfoResponse {
        project: data.project,
        description: data.description,
        marketing: data.marketing,
        logo: data.logo,
    })?)
}

pub fn query_download_logo(deps: Deps) -> Result<Binary, ContractError> {
    let logo = LOGO
        .may_load(deps.storage)?
        .ok_or_else(|| ContractError::new("No logo stored"))?;
    Ok(to_json_binary(&DownloadLogoResponse {
        mime_type: logo.mime_type,
        data: logo.data,
    })?)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn transfer_tokens(
    deps: DepsMut,
    from: &Addr,
    to: &Addr,
    amount: Uint128,
) -> Result<(), ContractError> {
    deduct_balance(deps.storage, from, amount)?;
    BALANCES.update(deps.storage, to, |bal| -> StdResult<_> {
        Ok(bal.unwrap_or_default().checked_add(amount)?)
    })?;
    Ok(())
}

fn deduct_balance(
    storage: &mut dyn cosmwasm_std::Storage,
    addr: &Addr,
    amount: Uint128,
) -> Result<(), ContractError> {
    BALANCES.update(storage, addr, |bal| -> Result<_, ContractError> {
        let balance = bal.ok_or(ContractError::InsufficientFunds {})?;
        balance
            .checked_sub(amount)
            .map_err(|_| ContractError::InsufficientFunds {})
    })?;
    Ok(())
}

fn deduct_allowance(
    storage: &mut dyn cosmwasm_std::Storage,
    env: &Env,
    owner: &Addr,
    spender: &Addr,
    amount: Uint128,
) -> Result<(), ContractError> {
    let key = (owner, spender);
    let mut data = ALLOWANCES
        .may_load(storage, key)?
        .ok_or(ContractError::new("No allowance for this spender"))?;

    ensure!(
        !data.expires.is_expired(&env.block),
        ContractError::new("Allowance expired")
    );
    ensure!(
        data.allowance >= amount,
        ContractError::new("Insufficient allowance")
    );

    data.allowance = data.allowance.checked_sub(amount)?;
    if data.allowance.is_zero() {
        ALLOWANCES.remove(storage, key);
        ALLOWANCES_SPENDER.remove(storage, (spender, owner));
    } else {
        ALLOWANCES.save(storage, key, &data)?;
        ALLOWANCES_SPENDER.save(storage, (spender, owner), &data)?;
    }
    Ok(())
}
