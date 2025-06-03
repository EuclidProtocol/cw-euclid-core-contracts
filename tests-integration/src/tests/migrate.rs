#![cfg(not(target_arch = "wasm32"))]
use std::collections::HashMap;

use cosmwasm_std::{coin, Addr, Coin, Uint128, Uint64};
use cw_orch::prelude::{ContractInstance, CwOrchExecute, CwOrchQuery, Environment};
use cw_orch::prelude::{CwOrchInstantiate, CwOrchUpload};
use cw_orch_interchain::types::IbcPacketOutcome;
use cw_orch_interchain::{prelude::*, InterchainEnv};
use escrow::mock::mock_escrow;
use euclid::msgs::virtual_balance::VBalanceMigrateMsg;
use euclid::{
    chain::{ChainUid, CrossChainUser, CrossChainUserWithLimit},
    error::ContractError,
    fee::{DenomFees, PartnerFee, BPS_100_PERCENT, BPS_1_PERCENT, MAX_PARTNER_FEE_BPS},
    msgs::{
        escrow::StateResponse as EscrowStateResponse,
        factory::{AllPoolsResponse, ExecuteSwapRequest, StateResponse},
        router::{QueryMsgFns, TokenDenom, TokenDenomsResponse, VlpResponse},
        vlp::GetLiquidityResponse,
    },
    pool::PoolConfig,
    swap::NextSwapPair,
    token::{
        Pair, PairWithDenomAndAmount, Token, TokenType, TokenWithDenom, TokenWithDenomAndAmount,
    },
};
use factory::mock::{mock_factory, MockFactory};
use migration::MigrationContract;
use mock::{mock::mock_app, mock_builder::MockEuclidBuilder};

use crate::helpers::{
    chains::{get_escrow, get_virtual_balance, get_vlp, setup_factory, setup_router},
    factory::{add_liquidity, create_pool, faucet, register_token, swap_request},
    relayer::relay_factory_router_factory,
};
#[test]
fn test_migrate_vbalance() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![
        ("osmosis", &sender),
        ("nibiru", &sender),
        ("hub", &sender),
    ]);
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router_contract = setup_router(&router_chain).unwrap();
    let factory = setup_factory(&interchain, "osmosis", "nibiru", &router_contract).unwrap();

    let nibiru = interchain.get_chain("nibiru").unwrap();
    let hub = interchain.get_chain("hub").unwrap();

    let router_state = router_contract.get_state().unwrap();
    let virtual_balance_contract =
        get_virtual_balance(&nibiru, &router_state.virtual_balance_address.unwrap());
    // Without manually modifying the vbalance contract, the state on nibiru and hub will be the same
    virtual_balance_contract
        .execute(
            &euclid::msgs::virtual_balance::ExecuteMsg::UpdateState {
                router: Some("nibiru_router".to_string()),
                admin: None,
                migration_contract: None,
            },
            None,
        )
        .unwrap();

    let migration = MigrationContract::new(nibiru.clone());
    migration.upload().unwrap();

    migration
        .instantiate(
            &euclid::msgs::migrator::InstantiateMsg {
                router: router_contract.address().unwrap().to_string(),
                virtual_balance: virtual_balance_contract.address().unwrap().to_string(),
                admin: sender.clone(),
            },
            None,
            None,
        )
        .unwrap();

    // HUB UPLOADS //

    let router_hub_chain = interchain.get_chain("hub").unwrap();
    let router_contract_hub = setup_router(&router_hub_chain).unwrap();

    let router_state = router_contract_hub.get_state().unwrap();
    let virtual_balance_contract_hub =
        get_virtual_balance(&hub, &router_state.virtual_balance_address.unwrap());

    let migration_hub = MigrationContract::new(hub.clone());
    migration_hub.upload().unwrap();

    migration_hub
        .instantiate(
            &euclid::msgs::migrator::InstantiateMsg {
                router: router_contract_hub.address().unwrap().to_string(),
                virtual_balance: virtual_balance_contract.address().unwrap().to_string(),
                admin: sender.clone(),
            },
            None,
            None,
        )
        .unwrap();
    // SETUP CHANNEL //
    // Set up channel from nibiru to hub
    let channel_receipt = interchain
        .create_contract_channel(&migration, &migration_hub, "migrate-1", None)
        .unwrap();

    // After channel creation is complete, we get the channel id, which is necessary for ICA remote execution
    let nibiru_channel = channel_receipt
        .interchain_channel
        .get_chain("nibiru")
        .unwrap()
        .channel
        .unwrap();
    println!("nibiru_channel: {:?}", nibiru_channel);
    //
    // MIGRATE //

    // The hub's vbalance state before the migration
    let hub_vbalance_state_before: VBalanceMigrateMsg = virtual_balance_contract_hub
        .query(&euclid::msgs::virtual_balance::QueryMsg::GetMigrateData {})
        .unwrap();
    println!("hub_vbalance_state: {:?}", hub_vbalance_state_before);

    let migrate_msg = euclid::msgs::migrator::ExecuteMsg::MigrateVBalance {
        vbalance_address: virtual_balance_contract_hub.address().unwrap().to_string(),
        channel_id: nibiru_channel.to_string(),
        timeout: None,
    };
    let migration_request = migration.execute(&migrate_msg, None).unwrap();
    println!("migration_request: {:?}", migration_request);

    let packet_lifetime = interchain
        .await_packets("nibiru", migration_request)
        .unwrap();

    // The hub's vbalance state after the migration
    let hub_vbalance_state_after: VBalanceMigrateMsg = virtual_balance_contract_hub
        .query(&euclid::msgs::virtual_balance::QueryMsg::GetMigrateData {})
        .unwrap();
    println!("hub_vbalance_state: {:?}", hub_vbalance_state_after);

    // The hub's vbalance state should be different after the migration
    assert_ne!(hub_vbalance_state_before, hub_vbalance_state_after);

    // funds.clear();
    // faucet(
    //     &chain,
    //     chain.sender.as_str(),
    //     1000,
    //     asset_in.token_type.clone(),
    //     &mut funds,
    // );

    // let new_partner_eucl_balance = factory
    //     .environment()
    //     .query_balance(partner_fee_recipient.clone(), "eucl")
    //     .unwrap();
    // assert_eq!(new_partner_eucl_balance, old_partner_eucl_balance);
}
