#![cfg(not(target_arch = "wasm32"))]
use cw20::Cw20Contract;
use cw_orch::{mock::MockBase, prelude::*};
use cw_orch_interchain::{InterchainEnv, MockInterchainEnv};
use escrow::EscrowContract;
use euclid::{
    chain::ChainUid,
    msgs::{
        factory::ExecuteMsgFns as FactoryExecuteMsgFns,
        router::{
            ExecuteMsgFns as RouterExecuteMsgFns, QueryMsgFns, RegisterFactoryChainIbc,
            RegisterFactoryChainNative,
        },
    },
};
use factory::FactoryContract;
use router::RouterContract;
use virtual_balance::VirtualBalanceContract;
use vlp::VlpContract;

pub fn setup_factory(
    interchain: &MockInterchainEnv,
    factory_chain_id: &str,
    router_chain_id: &str,
    router: &RouterContract<MockBase>,
) -> FactoryContract<MockBase> {
    let chain_uid = ChainUid::create(factory_chain_id.to_string()).unwrap();
    let chain = interchain.get_chain(factory_chain_id).unwrap();
    let _router_chain = interchain.get_chain(router_chain_id).unwrap();
    let factory = FactoryContract::new(chain.clone());
    let escrow = EscrowContract::new(chain.clone());
    let cw20 = Cw20Contract::new(chain.clone());

    factory.upload().unwrap();
    escrow.upload().unwrap();
    cw20.upload().unwrap();

    factory
        .instantiate(
            &euclid::msgs::factory::InstantiateMsg {
                router_contract: router.address().unwrap().to_string(),
                chain_uid: chain_uid.clone(),
                escrow_code_id: escrow.code_id().unwrap(),
                cw20_code_id: cw20.code_id().unwrap(),
                is_native: false,
            },
            None,
            None,
        )
        .unwrap();

    if router_chain_id != factory_chain_id {
        // Set up channel from osmosis to nibiru
        let channel_receipt = interchain
            .create_contract_channel(&factory, router, "counter-1", None)
            .unwrap();
        let factory_channel = channel_receipt
            .interchain_channel
            .get_chain(factory_chain_id)
            .unwrap()
            .channel
            .unwrap();

        let router_channel = channel_receipt
            .interchain_channel
            .get_chain(router_chain_id)
            .unwrap()
            .channel
            .unwrap();

        factory
            .update_hub_channel(factory_channel.to_string())
            .unwrap();

        let chain_info =
            euclid::msgs::router::RegisterFactoryChainType::Ibc(RegisterFactoryChainIbc {
                channel: router_channel.to_string(),
                timeout: None,
            });
        let register_request = router
            .register_factory(chain_info, chain_uid.clone())
            .unwrap();
        let _ = interchain
            .await_packets(router_chain_id, register_request)
            .unwrap();
    } else {
        let chain_info =
            euclid::msgs::router::RegisterFactoryChainType::Native(RegisterFactoryChainNative {
                factory_address: factory.address().unwrap().to_string(),
            });
        router
            .register_factory(chain_info, chain_uid.clone())
            .unwrap();
    }
    let all_chains = router.get_all_chains().unwrap();
    // Asert that this chain is registered
    assert!(all_chains
        .chains
        .iter()
        .any(|c| c.chain_uid == chain_uid.clone()));

    factory
}

pub fn setup_router(chain: &MockBase) -> RouterContract<MockBase> {
    let router = RouterContract::new(chain.clone());
    let virtual_balance = VirtualBalanceContract::new(chain.clone());
    let vlp = VlpContract::new(chain.clone());

    router.upload().unwrap();
    virtual_balance.upload().unwrap();
    vlp.upload().unwrap();

    router
        .instantiate(
            &euclid::msgs::router::InstantiateMsg {
                vlp_code_id: vlp.code_id().unwrap(),
                virtual_balance_code_id: virtual_balance.code_id().unwrap(),
            },
            None,
            None,
        )
        .unwrap();

    router
}
