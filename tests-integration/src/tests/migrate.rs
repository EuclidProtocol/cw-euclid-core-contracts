#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::Addr;
use cw_orch::prelude::{ContractInstance, CwOrchExecute, CwOrchQuery};
use cw_orch::prelude::{CwOrchInstantiate, CwOrchUpload};
use cw_orch_interchain::{prelude::*, InterchainEnv};
use euclid::fee::Fee;
use euclid::msgs::virtual_balance::VBalanceMigrateMsg;
use euclid::msgs::vlp::VlpMigrateMsg;
use euclid::{
    chain::{ChainUid, CrossChainUser},
    msgs::router::QueryMsgFns,
    token::{Pair, Token},
};
use migration::MigrationContract;
use vlp::VlpContract;

use crate::helpers::chains::{get_virtual_balance, setup_router};
use rstest::rstest;

#[derive(Debug)]
enum MigrationKind {
    VBalance,
    VLP,
}

#[rstest]
#[case(MigrationKind::VBalance)]
#[case(MigrationKind::VLP)]
fn test_migrate(#[case] kind: MigrationKind) {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![
        ("osmosis", &sender),
        ("nibiru", &sender),
        ("hub", &sender),
    ]);

    let nibiru = interchain.get_chain("nibiru").unwrap();
    let hub = interchain.get_chain("hub").unwrap();

    let router_nibiru = setup_router(&nibiru).unwrap();
    let router_state = router_nibiru.get_state().unwrap();

    let vbalance_nibiru =
        get_virtual_balance(&nibiru, &router_state.virtual_balance_address.unwrap());

    let migration = MigrationContract::new(nibiru.clone());
    migration.upload().unwrap();

    let mut vlp_nibiru = None;
    if let MigrationKind::VLP = kind {
        let vlp = VlpContract::new(nibiru.clone());
        vlp.upload().unwrap();
        vlp.instantiate(
            &euclid::msgs::vlp::InstantiateMsg {
                router: "router_nibiru".to_string(),
                virtual_balance: vbalance_nibiru.address().unwrap().to_string(),
                pair: Pair::new(
                    Token::create("1".to_string()).unwrap(),
                    Token::create("2".to_string()).unwrap(),
                )
                .unwrap(),
                fee: Fee::new(
                    1,
                    2,
                    CrossChainUser::new(
                        ChainUid::create("1".to_string()).unwrap(),
                        "useraddr".to_string(),
                    ),
                ),
                execute: None,
                admin: sender.clone(),
            },
            None,
            None,
        )
        .unwrap();
        vlp_nibiru = Some(vlp);
    }

    migration
        .instantiate(
            &euclid::msgs::migrator::InstantiateMsg {
                router: router_nibiru.address().unwrap().to_string(),
                virtual_balance: vbalance_nibiru.address().unwrap().to_string(),
                vlp: vlp_nibiru
                    .as_ref()
                    .map_or(String::default(), |v| v.addr_str().unwrap()),
                admin: sender.clone(),
            },
            None,
            None,
        )
        .unwrap();

    // HUB SETUP
    let router_hub = setup_router(&hub).unwrap();
    let hub_state = router_hub.get_state().unwrap();
    let vbalance_hub = get_virtual_balance(&hub, &hub_state.virtual_balance_address.unwrap());

    let mut vlp_hub = None;
    if let MigrationKind::VLP = kind {
        let vlp = VlpContract::new(hub.clone());
        vlp.upload().unwrap();
        vlp.instantiate(
            &euclid::msgs::vlp::InstantiateMsg {
                router: "router_nibiru".to_string(),
                virtual_balance: vbalance_nibiru.address().unwrap().to_string(),
                pair: Pair::new(
                    Token::create("3".to_string()).unwrap(),
                    Token::create("4".to_string()).unwrap(),
                )
                .unwrap(),
                fee: Fee::new(
                    1,
                    2,
                    CrossChainUser::new(
                        ChainUid::create("1".to_string()).unwrap(),
                        "useraddr".to_string(),
                    ),
                ),
                execute: None,
                admin: sender.clone(),
            },
            None,
            None,
        )
        .unwrap();
        vlp_hub = Some(vlp);
    }

    let migration_hub = MigrationContract::new(hub.clone());
    migration_hub.upload().unwrap();
    migration_hub
        .instantiate(
            &euclid::msgs::migrator::InstantiateMsg {
                router: router_hub.address().unwrap().to_string(),
                virtual_balance: vbalance_nibiru.address().unwrap().to_string(),
                vlp: vlp_hub
                    .as_ref()
                    .map_or(String::default(), |v| v.addr_str().unwrap()),
                admin: sender.clone(),
            },
            None,
            None,
        )
        .unwrap();

    // INTERCHAIN CHANNEL
    let channel_receipt = interchain
        .create_contract_channel(&migration, &migration_hub, "migrate-1", None)
        .unwrap();

    let channel_id = channel_receipt
        .interchain_channel
        .get_chain("nibiru")
        .unwrap()
        .channel
        .unwrap();

    match kind {
        MigrationKind::VBalance => {
            let before: VBalanceMigrateMsg = vbalance_hub
                .query(&euclid::msgs::virtual_balance::QueryMsg::GetMigrateData {})
                .unwrap();

            let msg = euclid::msgs::migrator::ExecuteMsg::MigrateVBalance {
                router_address: router_hub.address().unwrap().to_string() + "new",
                vbalance_address: vbalance_hub.address().unwrap().to_string(),
                channel_id: channel_id.to_string(),
                timeout: None,
            };
            let request = migration.execute(&msg, None).unwrap();
            let _ = interchain.await_packets("nibiru", request).unwrap();

            let after: VBalanceMigrateMsg = vbalance_hub
                .query(&euclid::msgs::virtual_balance::QueryMsg::GetMigrateData {})
                .unwrap();

            assert_ne!(before, after);
        }
        MigrationKind::VLP => {
            let vlp_hub = vlp_hub.expect("vlp_hub should be set");
            let before: VlpMigrateMsg = vlp_hub
                .query(&euclid::msgs::vlp::QueryMsg::GetMigrateData {})
                .unwrap();

            let msg = euclid::msgs::migrator::ExecuteMsg::MigrateVLP {
                vlp_address: vlp_hub.address().unwrap().to_string(),
                router_address: router_hub.address().unwrap().to_string() + "new",
                vbalance_address: vbalance_hub.address().unwrap().to_string(),
                channel_id: channel_id.to_string(),
                timeout: None,
            };
            let request = migration.execute(&msg, None).unwrap();
            let _ = interchain.await_packets("nibiru", request).unwrap();

            let after: VlpMigrateMsg = vlp_hub
                .query(&euclid::msgs::vlp::QueryMsg::GetMigrateData {})
                .unwrap();

            assert_ne!(before, after);
        }
    }
}
