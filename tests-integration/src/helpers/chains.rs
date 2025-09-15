#![cfg(not(target_arch = "wasm32"))]
use claimer::ClaimerContract;
use cw20::Cw20Contract;
use cw_orch::{mock::MockBase, prelude::*};
use cw_orch_interchain::core::{IbcQueryHandler, InterchainEnv};
use cw_orch_interchain::mock::MockInterchainEnv;
use escrow::EscrowContract;
use euclid::{
    chain::ChainUid,
    msgs::{
        factory::{ExecuteMsgFns as FactoryExecuteMsgFns, QueryMsgFns as FactoryQueryMsgFns},
        router::{
            ExecuteMsgFns as RouterExecuteMsgFns, QueryMsgFns as RouterQueryMsgFns,
            RegisterFactoryChainIbc, RegisterFactoryChainNative,
        },
    },
};
use euclid_relayer::RelayerContract;
use factory::FactoryContract;
use router::RouterContract;
use stable_vlp::StableVlpContract;
use virtual_balance::VirtualBalanceContract;
use vlp::VlpContract;

use crate::helpers::relayer::{relay_router_ack_packet, relay_router_send_packet};

use super::relayer::get_signer_key;

pub fn setup_factory(
    interchain: &MockInterchainEnv,
    factory_chain_id: &str,
    router_chain_id: &str,
    router: &RouterContract<MockBase>,
) -> Result<FactoryContract<MockBase>, CwOrchError> {
    let chain_uid = ChainUid::create(factory_chain_id.to_string()).unwrap();
    let chain = interchain.get_chain(factory_chain_id).unwrap();
    let _router_chain = interchain.get_chain(router_chain_id).unwrap();
    let factory = FactoryContract::new(chain.clone());
    let escrow = EscrowContract::new(chain.clone());
    let cw20 = Cw20Contract::new(chain.clone());
    let relayer = setup_relayer(&chain)?;

    factory.upload().unwrap();
    escrow.upload().unwrap();
    cw20.upload().unwrap();

    let is_native = router_chain_id == factory_chain_id;

    factory.instantiate(
        &euclid::msgs::factory::InstantiateMsg {
            router_contract: router.address().unwrap().to_string(),
            chain_uid: chain_uid.clone(),
            escrow_code_id: escrow.code_id().unwrap(),
            cw20_code_id: cw20.code_id().unwrap(),
            is_native,
            mock_relayer_address: Some(relayer.address().unwrap().to_string()),
        },
        None,
        &[],
    )?;

    if !is_native {
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

        factory.update_hub_channel(factory_channel.to_string())?;

        let chain_info =
            euclid::msgs::router::RegisterFactoryChainType::Ibc(RegisterFactoryChainIbc {
                channel: router_channel.to_string(),
                timeout: None,
                factory_address: factory.address().unwrap().to_string(),
                factory_chain_id: factory.environment().chain_id(),
            });
        let register_request = router
            .register_factory(chain_info, chain_uid.clone())
            .unwrap();
        let ack_events = relay_router_send_packet(register_request.events, &factory, &chain_uid)?;
        relay_router_ack_packet(router, &chain_uid, ack_events)?;
        // let _ = interchain
        //     .await_packets(router_chain_id, register_request)
        //     .unwrap();
    } else {
        let chain_info =
            euclid::msgs::router::RegisterFactoryChainType::Native(RegisterFactoryChainNative {
                factory_address: factory.address().unwrap().to_string(),
                factory_chain_id: factory.environment().chain_id(),
            });
        factory.update_hub_channel("channel-0".to_string())?;
        router.register_factory(chain_info, chain_uid.clone())?;
    }
    let all_chains = router.get_all_chains().unwrap();
    // Asert that this chain is registered
    assert!(all_chains
        .chains
        .iter()
        .any(|c| c.chain_uid == chain_uid.clone()));

    Ok(factory)
}

pub fn setup_router(chain: &MockBase) -> Result<RouterContract<MockBase>, CwOrchError> {
    let router = RouterContract::new(chain.clone());
    let virtual_balance = VirtualBalanceContract::new(chain.clone());
    let vlp = VlpContract::new(chain.clone());
    let stable_vlp = StableVlpContract::new(chain.clone());

    let relayer = setup_relayer(chain)?;

    router.upload().unwrap();
    virtual_balance.upload().unwrap();
    vlp.upload().unwrap();
    stable_vlp.upload().unwrap();

    router.instantiate(
        &euclid::msgs::router::InstantiateMsg {
            constant_product_vlp_code_id: vlp.code_id().unwrap(),
            stable_vlp_code_id: stable_vlp.code_id().unwrap(),
            virtual_balance_code_id: virtual_balance.code_id().unwrap(),
            mock_relayer_addresses: Some(vec![relayer.address().unwrap().to_string()]),
        },
        None,
        &[],
    )?;

    Ok(router)
}

pub fn setup_relayer(chain: &MockBase) -> Result<RelayerContract<MockBase>, CwOrchError> {
    let relayer = RelayerContract::new(chain.clone());
    let (_, pubkey_binary) = get_signer_key();

    relayer.upload().unwrap();

    relayer.instantiate(
        &relayer::msgs::InstantiateMsg {
            relayer_pubkey: pubkey_binary,
            relayer_address: format!("relayer_{}", chain.chain_id()),
            authorized_addresses: vec![],
        },
        Some(&chain.sender),
        &[],
    )?;

    Ok(relayer)
}

pub fn setup_claimer(
    factory: &FactoryContract<MockBase>,
    voucher_address: &VirtualBalanceContract<MockBase>,
) -> Result<ClaimerContract<MockBase>, CwOrchError> {
    let chain = factory.environment().clone();
    let claimer = ClaimerContract::new(chain.clone());
    claimer.upload().unwrap();
    claimer.instantiate(
        &euclid::msgs::claimer::InstantiateMsg {
            factory_address: factory.address().unwrap(),
            voucher_address: voucher_address.address().unwrap(),
        },
        None,
        &[],
    )?;
    Ok(claimer)
}

pub fn get_vlp(chain: &MockBase, address: &Addr) -> VlpContract<MockBase> {
    let mut vlp = VlpContract::new(chain.clone());
    vlp.as_instance_mut().id = format!("vlp_{}", address);
    vlp.set_address(address);
    vlp
}

#[allow(dead_code)]
pub fn get_stable_vlp(chain: &MockBase, address: &Addr) -> StableVlpContract<MockBase> {
    let mut stable_vlp = StableVlpContract::new(chain.clone());
    stable_vlp.as_instance_mut().id = format!("stable_vlp_{}", address);
    stable_vlp.set_address(address);
    stable_vlp
}

pub fn get_virtual_balance(chain: &MockBase, address: &Addr) -> VirtualBalanceContract<MockBase> {
    let mut virtual_balance = VirtualBalanceContract::new(chain.clone());
    virtual_balance.as_instance_mut().id = format!("virtual_balance_{}", address);
    virtual_balance.set_address(address);
    virtual_balance
}

pub fn get_cw20(chain: &MockBase, address: &Addr) -> Cw20Contract<MockBase> {
    let mut cw20 = Cw20Contract::new(chain.clone());
    cw20.as_instance_mut().id = format!("cw20_{}", address);
    cw20.set_address(address);
    cw20
}

pub fn get_escrow(factory: &FactoryContract<MockBase>, token: &str) -> EscrowContract<MockBase> {
    let escrow_address = factory.get_escrow(token).unwrap();
    let escrow_address = escrow_address.escrow_address.unwrap();
    println!("Token: {:?} Escrow address: {:?}", token, escrow_address);
    // Create a new escrow contract instance
    let mut escrow = EscrowContract::new(factory.environment().clone());
    escrow.as_instance_mut().id = format!("escrow_{}", escrow_address);
    escrow.set_address(&escrow_address);
    escrow
}

#[allow(dead_code)]
pub fn get_factory(chain: &MockBase, address: &Addr) -> FactoryContract<MockBase> {
    let mut factory = FactoryContract::new(chain.clone());
    factory.as_instance_mut().id = format!("factory_{}", address);
    factory.set_address(address);
    factory
}

#[allow(dead_code)]
pub fn get_router(chain: &MockBase, address: &Addr) -> RouterContract<MockBase> {
    let mut router = RouterContract::new(chain.clone());
    router.as_instance_mut().id = format!("router_{}", address);
    router.set_address(address);
    router
}

pub fn get_relayer(chain: &MockBase, address: &Addr) -> RelayerContract<MockBase> {
    let mut relayer = RelayerContract::new(chain.clone());
    relayer.as_instance_mut().id = format!("relayer_{}", address);
    relayer.set_address(address);
    relayer
}

pub fn _get_claimer(chain: &MockBase, address: &Addr) -> ClaimerContract<MockBase> {
    let mut claimer = ClaimerContract::new(chain.clone());
    claimer.as_instance_mut().id = format!("claimer_{}", address);
    claimer.set_address(address);
    claimer
}
