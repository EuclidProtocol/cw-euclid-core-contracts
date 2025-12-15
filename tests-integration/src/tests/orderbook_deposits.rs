#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{Addr, Empty, Uint128};
use cw_multi_test::{App, Contract, ContractWrapper, Executor};
use euclid::{
    chain::{ChainUid, CrossChainUser},
    msgs::virtual_balance::{
        ExecuteApprove, ExecuteMint, ExecuteMsg as VirtualBalanceExecuteMsg,
        InstantiateMsg as VirtualBalanceInstantiateMsg,
    },
    virtual_balance::BalanceKey,
};
use orderbook_deposits::msg::{
    AssetDepositResponse, QueryMsg as OrderbookQueryMsg, StateResponse, UserDepositResponse,
    WhitelistListResponse,
};
use orderbook_deposits::msg::{
    ExecuteMsg as OrderbookExecuteMsg, InstantiateMsg as OrderbookInstantiateMsg,
};

fn orderbook_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new(
        orderbook_deposits::contract::execute,
        orderbook_deposits::contract::instantiate,
        orderbook_deposits::contract::query,
    ))
}

fn virtual_balance_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new(
        virtual_balance::contract::execute,
        virtual_balance::contract::instantiate,
        virtual_balance::contract::query,
    ))
}

#[test]
fn deposit_and_query_flow() {
    let mut app = App::default();

    let router = app.api().addr_make("router");
    let depositor = app.api().addr_make("depositor");
    let token_id = "token1".to_string();
    let deposit_amount = Uint128::new(500);
    let chain_uid = ChainUid::vsl_chain_uid().unwrap();

    let vb_code_id = app.store_code(virtual_balance_contract());
    let ob_code_id = app.store_code(orderbook_contract());

    // Instantiate virtual balance with router as instantiator (also router field)
    let virtual_balance_addr = app
        .instantiate_contract(
            vb_code_id,
            router.clone(),
            &VirtualBalanceInstantiateMsg {
                router: router.clone(),
                admin: Some(router.clone()),
            },
            &[],
            "virtual_balance",
            None,
        )
        .unwrap();

    // Instantiate orderbook deposits pointing to virtual balance
    let orderbook_addr = app
        .instantiate_contract(
            ob_code_id,
            router.clone(),
            &OrderbookInstantiateMsg {
                virtual_balance: virtual_balance_addr.to_string(),
                admin: Some(router.to_string()),
            },
            &[],
            "orderbook_deposits",
            None,
        )
        .unwrap();

    // Whitelist token as admin
    app.execute_contract(
        router.clone(),
        orderbook_addr.clone(),
        &OrderbookExecuteMsg::SetWhitelist {
            token_id: token_id.clone(),
            whitelisted: true,
        },
        &[],
    )
    .unwrap();

    // Mint virtual balance to depositor (router authority)
    app.execute_contract(
        router.clone(),
        virtual_balance_addr.clone(),
        &VirtualBalanceExecuteMsg::Mint(ExecuteMint {
            amount: deposit_amount,
            balance_key: BalanceKey {
                cross_chain_user: CrossChainUser::new(chain_uid.clone(), depositor.to_string()),
                token_id: token_id.clone(),
            },
        }),
        &[],
    )
    .unwrap();

    // Approve orderbook to move depositor's balance
    app.execute_contract(
        depositor.clone(),
        virtual_balance_addr.clone(),
        &VirtualBalanceExecuteMsg::Approve(ExecuteApprove {
            amount: deposit_amount,
            token_id: token_id.clone(),
            spender: CrossChainUser::new(chain_uid.clone(), orderbook_addr.to_string()),
            owner: CrossChainUser::new(chain_uid.clone(), depositor.to_string()),
        }),
        &[],
    )
    .unwrap();

    // Deposit
    app.execute_contract(
        depositor.clone(),
        orderbook_addr.clone(),
        &OrderbookExecuteMsg::Deposit {
            token_id: token_id.clone(),
            amount: deposit_amount,
        },
        &[],
    )
    .unwrap();

    // Verify state query
    let state: StateResponse = app
        .wrap()
        .query_wasm_smart(orderbook_addr.clone(), &OrderbookQueryMsg::State {})
        .unwrap();
    assert_eq!(state.admin, router.to_string());
    assert_eq!(state.virtual_balance, virtual_balance_addr.to_string());
    assert_eq!(state.status, "active".to_string());

    // Verify aggregate deposit
    let asset_deposit: AssetDepositResponse = app
        .wrap()
        .query_wasm_smart(
            orderbook_addr.clone(),
            &OrderbookQueryMsg::AssetDeposit {
                token_id: token_id.clone(),
            },
        )
        .unwrap();
    assert_eq!(asset_deposit.amount, deposit_amount);

    // Verify user deposit
    let user_deposit: UserDepositResponse = app
        .wrap()
        .query_wasm_smart(
            orderbook_addr.clone(),
            &OrderbookQueryMsg::UserDeposit {
                user: depositor.to_string(),
                token_id: token_id.clone(),
            },
        )
        .unwrap();
    assert_eq!(user_deposit.amount, deposit_amount);

    // Verify whitelist listing
    let whitelisted: WhitelistListResponse = app
        .wrap()
        .query_wasm_smart(
            orderbook_addr,
            &OrderbookQueryMsg::WhitelistedAssets {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();
    assert!(whitelisted
        .assets
        .iter()
        .any(|w| w.token_id == token_id && w.whitelisted));
}
