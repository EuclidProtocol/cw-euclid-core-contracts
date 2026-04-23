#![cfg(not(target_arch = "wasm32"))]

use crate::helpers::chains::get_escrow_addr;
use crate::helpers::multi_chain::MultiChainEnv;
use crate::helpers::relayer::relay_factory_router_factory;
use cosmwasm_std::{coin, Addr, Coin, Uint128};
use euclid::chain::ChainUid;
use euclid::cross_chain_user::CrossChainUser;
use euclid::fee::PartnerFee;
use euclid::msgs::cross_chain_config::CrossChainConfig;
use euclid::msgs::factory::ExecuteSwapRequest;
use euclid::msgs::vlp::base::PoolConfig;
use euclid::recipient::Recipient;
use euclid::swap::NextSwapPair;
use euclid::token::PairWithDenomAndAmount;
use euclid::token::Token;
use euclid::token::TokenType;
use euclid::token::TokenWithDenom;
use euclid::utils::pagination::Pagination;
use euclid::voucher::BalanceKey;

use super::app::EuclidApp;

pub fn register_token(
    factory_addr: &Addr,
    factory_chain_id: &str,
    router_addr: &Addr,
    router_chain_id: &str,
    env: &mut MultiChainEnv,
    token: TokenWithDenom,
) -> Result<(), anyhow::Error> {
    let factory_chain_uid = {
        let factory_state: euclid::msgs::factory::StateResponse =
            env.chain(factory_chain_id).query(factory_addr, &euclid::msgs::factory::QueryMsg::GetState {});
        factory_state.chain_uid
    };

    let sender = env.chain(factory_chain_id).sender();
    let tx_response = env.chain_mut(factory_chain_id).execute(
        &sender,
        factory_addr,
        &euclid::msgs::factory::ExecuteMsg::RegisterDenom {
            token_with_denom: token.clone(),
            cross_chain_config: CrossChainConfig::default(),
        },
        &[],
    );

    relay_factory_router_factory(
        tx_response.events,
        factory_chain_id,
        factory_addr,
        &factory_chain_uid,
        router_chain_id,
        router_addr,
        env,
    )?;

    let escrow_response: euclid::msgs::factory::GetEscrowResponse = env.chain(factory_chain_id).query(
        factory_addr,
        &euclid::msgs::factory::QueryMsg::GetEscrow {
            token_id: token.token.to_string(),
        },
    );
    assert!(
        escrow_response.denoms.iter().any(|d| d == &token.token_type),
        "Escrow found but denom not registered"
    );

    Ok(())
}

pub fn deposit_token(
    factory_addr: &Addr,
    factory_chain_id: &str,
    router_addr: &Addr,
    router_chain_id: &str,
    env: &mut MultiChainEnv,
    token: TokenWithDenom,
    amount: Uint128,
    recipients: Vec<Recipient>,
) -> Result<(), anyhow::Error> {
    let factory_chain_uid = {
        let factory_state: euclid::msgs::factory::StateResponse =
            env.chain(factory_chain_id).query(factory_addr, &euclid::msgs::factory::QueryMsg::GetState {});
        factory_state.chain_uid
    };

    let sender = env.chain(factory_chain_id).sender();
    let mut funds = vec![];
    faucet(env.chain_mut(factory_chain_id), &sender, amount.u128(), token.token_type.clone(), &mut funds);

    let escrow_addr = get_escrow_addr(env.chain(factory_chain_id), factory_addr, token.token.as_str());
    let old_escrow_state: euclid::msgs::escrow::StateResponse =
        env.chain(factory_chain_id).query(&escrow_addr, &euclid::msgs::escrow::QueryMsg::State {});

    let old_router_escrow_balance: euclid::msgs::router::TokenEscrowsResponse =
        env.chain(router_chain_id).query(
            router_addr,
            &euclid::msgs::router::QueryMsg::QueryTokenEscrows {
                token: token.token.clone(),
                pagination: Pagination::new(Some(factory_chain_uid.clone()), None, None, Some(1)),
            },
        );
    let old_balance = match old_router_escrow_balance.chains.first() {
        Some(chain) => chain.balance,
        None => Uint128::zero(),
    };

    let tx_response = env.chain_mut(factory_chain_id).execute(
        &sender,
        factory_addr,
        &euclid::msgs::factory::msg::ExecuteMsg::DepositToken {
            asset_in: token.clone(),
            amount_in: amount,
            recipients,
            cross_chain_config: CrossChainConfig::default(),
        },
        &funds,
    );

    relay_factory_router_factory(
        tx_response.events,
        factory_chain_id,
        factory_addr,
        &factory_chain_uid,
        router_chain_id,
        router_addr,
        env,
    )?;

    let new_router_escrow_balance: euclid::msgs::router::TokenEscrowsResponse =
        env.chain(router_chain_id).query(
            router_addr,
            &euclid::msgs::router::QueryMsg::QueryTokenEscrows {
                token: token.token.clone(),
                pagination: Pagination::new(Some(factory_chain_uid.clone()), None, None, Some(1)),
            },
        );
    let new_balance = match new_router_escrow_balance.chains.first() {
        Some(chain) => chain.balance,
        None => Uint128::zero(),
    };
    assert_eq!(new_balance, old_balance + amount, "Router escrow balance not updated properly");

    let new_escrow_state: euclid::msgs::escrow::StateResponse =
        env.chain(factory_chain_id).query(&escrow_addr, &euclid::msgs::escrow::QueryMsg::State {});
    assert_eq!(
        new_escrow_state.total_amount,
        old_escrow_state.total_amount + amount,
        "Escrow balance not updated properly"
    );

    Ok(())
}

pub fn transfer_token_vcoin(
    factory_addr: &Addr,
    factory_chain_id: &str,
    router_addr: &Addr,
    router_chain_id: &str,
    env: &mut MultiChainEnv,
    token: Token,
    amount: Uint128,
    recipients: Vec<Recipient>,
) -> Result<(), anyhow::Error> {
    let router_state: euclid::msgs::router::StateResponse =
        env.chain(router_chain_id).query(router_addr, &euclid::msgs::router::QueryMsg::GetState {});
    let virtual_balance_address = router_state.virtual_balance_address;

    let factory_state: euclid::msgs::factory::StateResponse =
        env.chain(factory_chain_id).query(factory_addr, &euclid::msgs::factory::QueryMsg::GetState {});
    let factory_sender = env.chain(factory_chain_id).sender();
    let sender_user = CrossChainUser::new(factory_state.chain_uid.clone(), factory_sender.to_string());
    let factory_chain_uid = factory_state.chain_uid;

    let old_balance: euclid::msgs::virtual_balance::GetBalanceResponse = env.chain(router_chain_id).query(
        &virtual_balance_address,
        &euclid::msgs::virtual_balance::QueryMsg::GetBalance {
            balance_key: BalanceKey {
                cross_chain_user: sender_user.clone(),
                token_id: token.to_string(),
            },
        },
    );

    let tx_response = env.chain_mut(factory_chain_id).execute(
        &factory_sender,
        factory_addr,
        &euclid::msgs::factory::ExecuteMsg::TransferVoucher {
            token_id: token.clone(),
            amount,
            from: None,
            recipients,
            cross_chain_config: CrossChainConfig::default(),
        },
        &[],
    );

    relay_factory_router_factory(
        tx_response.events,
        factory_chain_id,
        factory_addr,
        &factory_chain_uid,
        router_chain_id,
        router_addr,
        env,
    )?;

    let new_balance: euclid::msgs::virtual_balance::GetBalanceResponse = env.chain(router_chain_id).query(
        &virtual_balance_address,
        &euclid::msgs::virtual_balance::QueryMsg::GetBalance {
            balance_key: BalanceKey {
                cross_chain_user: sender_user.clone(),
                token_id: token.to_string(),
            },
        },
    );

    assert_eq!(
        new_balance.amount.u128() + amount.u128(),
        old_balance.amount.u128(),
        "Virtual balance not transferred properly"
    );
    Ok(())
}

pub fn faucet(
    app: &mut EuclidApp,
    address: &Addr,
    amount: u128,
    token_type: TokenType,
    funds: &mut Vec<Coin>,
) {
    match token_type {
        TokenType::Native { denom } => {
            app.add_balance(address, vec![coin(amount, denom.clone())]);
            funds.push(coin(amount, denom));
        }
        TokenType::Smart { contract_address } => {
            let lp_addr = Addr::unchecked(contract_address);
            let sender = app.sender();
            app.execute(
                &sender,
                &lp_addr,
                &euclid::msgs::lp_token::msg::ExecuteMsg::IncreaseAllowance {
                    spender: address.to_string(),
                    amount: Uint128::from(amount),
                    expires: None,
                },
                &[],
            );
        }
        _ => {}
    };
}

pub fn create_pool(
    factory_addr: &Addr,
    factory_chain_id: &str,
    router_addr: &Addr,
    router_chain_id: &str,
    env: &mut MultiChainEnv,
    pair_with_denom: PairWithDenomAndAmount,
    slippage_tolerance_bps: u64,
    pool_config: PoolConfig,
) -> Result<(), anyhow::Error> {
    let sender = env.chain(factory_chain_id).sender();
    let mut funds = vec![];
    for token in pair_with_denom.get_vec_token_info() {
        faucet(env.chain_mut(factory_chain_id), &sender, token.amount.u128(), token.token_type.clone(), &mut funds);
    }

    let tx_response = env.chain_mut(factory_chain_id).execute(
        &sender,
        factory_addr,
        &euclid::msgs::factory::ExecuteMsg::RequestPoolCreation {
            pair_with_denom_and_amount: pair_with_denom.clone(),
            slippage_tolerance_bps,
            lp_token_name: "LPNAME".to_string(),
            lp_token_symbol: "LPSYMBOL".to_string(),
            lp_token_decimal: 6,
            lp_token_marketing: None,
            pool_config,
            cross_chain_config: CrossChainConfig::default(),
        },
        &funds,
    );

    let factory_chain_uid = {
        let factory_state: euclid::msgs::factory::StateResponse =
            env.chain(factory_chain_id).query(factory_addr, &euclid::msgs::factory::QueryMsg::GetState {});
        factory_state.chain_uid
    };

    relay_factory_router_factory(
        tx_response.events,
        factory_chain_id,
        factory_addr,
        &factory_chain_uid,
        router_chain_id,
        router_addr,
        env,
    )?;

    let registered_pool: Result<euclid::msgs::factory::GetVlpResponse, _> =
        env.chain(factory_chain_id).try_query(
            factory_addr,
            &euclid::msgs::factory::QueryMsg::GetVlp {
                pair: pair_with_denom.get_pair().unwrap(),
            },
        );
    assert!(registered_pool.is_ok(), "Pool not registered");
    Ok(())
}

pub fn add_liquidity(
    factory_addr: &Addr,
    factory_chain_id: &str,
    router_addr: &Addr,
    router_chain_id: &str,
    env: &mut MultiChainEnv,
    pair_with_denom: PairWithDenomAndAmount,
    slippage_tolerance_bps: u64,
    funds: Vec<Coin>,
) -> Result<(), anyhow::Error> {
    let sender = env.chain(factory_chain_id).sender();
    let tx_response = env.chain_mut(factory_chain_id).execute(
        &sender,
        factory_addr,
        &euclid::msgs::factory::ExecuteMsg::AddLiquidity {
            pair_with_denom_and_amount: pair_with_denom.clone(),
            slippage_tolerance_bps,
            cross_chain_config: CrossChainConfig::default(),
        },
        &funds,
    );

    let factory_chain_uid = {
        let factory_state: euclid::msgs::factory::StateResponse =
            env.chain(factory_chain_id).query(factory_addr, &euclid::msgs::factory::QueryMsg::GetState {});
        factory_state.chain_uid
    };

    relay_factory_router_factory(
        tx_response.events,
        factory_chain_id,
        factory_addr,
        &factory_chain_uid,
        router_chain_id,
        router_addr,
        env,
    )?;

    Ok(())
}

pub fn swap_request(
    factory_addr: &Addr,
    factory_chain_id: &str,
    router_addr: &Addr,
    router_chain_id: &str,
    env: &mut MultiChainEnv,
    asset_in: TokenWithDenom,
    amount_in: Uint128,
    asset_out: Token,
    min_amount_out: Uint128,
    swaps: Vec<NextSwapPair>,
    recipients: Vec<Recipient>,
    partner_fee: Option<PartnerFee>,
    funds: Vec<Coin>,
) -> Result<(), anyhow::Error> {
    let sender = env.chain(factory_chain_id).sender();
    let tx_response = env.chain_mut(factory_chain_id).execute(
        &sender,
        factory_addr,
        &euclid::msgs::factory::ExecuteMsg::ExecuteSwapRequest(ExecuteSwapRequest {
            amount_in,
            recipients,
            asset_in,
            asset_out,
            min_amount_out,
            swaps,
            partner_fee,
            cross_chain_config: CrossChainConfig::default(),
        }),
        &funds,
    );

    let factory_chain_uid = {
        let factory_state: euclid::msgs::factory::StateResponse =
            env.chain(factory_chain_id).query(factory_addr, &euclid::msgs::factory::QueryMsg::GetState {});
        factory_state.chain_uid
    };

    relay_factory_router_factory(
        tx_response.events,
        factory_chain_id,
        factory_addr,
        &factory_chain_uid,
        router_chain_id,
        router_addr,
        env,
    )?;

    Ok(())
}
