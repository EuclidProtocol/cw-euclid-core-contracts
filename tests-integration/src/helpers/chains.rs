#![cfg(not(target_arch = "wasm32"))]
use claimer::ClaimerContract;
use cosmwasm_std::{from_json, Uint128};
use cp_vlp::VlpContract;
use cw_orch::{mock::MockBase, prelude::*};
use cw_orch_interchain::core::{IbcQueryHandler, InterchainEnv};
use cw_orch_interchain::mock::MockInterchainEnv;
use escrow::EscrowContract;
use euclid::chain::{CosmosChain, EvmChain};
use euclid::msgs::router::{
    ManageRouterState, RegisterFactoryChainCosmos, RegisterFactoryChainNative,
};
use euclid::{
    chain::{ChainType, ChainUid},
    msgs::router::RegisterFactoryChainEvm,
};
use euclid_ibc::factory_ibc::FactoryCrossChainExecuteMsg;
use euclid_relayer::RelayerContract;
use factory::FactoryContract;
use lp_token::LpTokenContract;
use meta_transaction::MetaTransactionContract;
use relayer::verify::cosmos_address_from_pubkey;
use relayer::Validator;
use router::RouterContract;
use stable_vlp::StableVlpContract;
use virtual_balance::VirtualBalanceContract;

use euclid::msgs::router::execute::ExecuteMsgFns as RouterExecuteMsgFns;
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;

use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;

use crate::helpers::relayer::{
    ack_register_factory_evm, extract_send_packet_events, relay_router_ack_packet,
    relay_router_send_packet,
};
use crate::tests_reusable::constants::ROUTER_CHAIN_ID;

use super::relayer::get_signer_key;

pub fn setup_interchain(sender: &str, factory_chain_id: &str) -> MockInterchainEnv {
    let mut chains = vec![(ROUTER_CHAIN_ID, sender)];
    if ROUTER_CHAIN_ID != factory_chain_id {
        chains.push((factory_chain_id, sender));
    }
    MockInterchainEnv::new(chains)
}

pub fn setup_factory(
    interchain: &MockInterchainEnv,
    factory_chain_id: &str,
    router_chain_id: &str,
    router: &RouterContract<MockBase>,
) -> Result<FactoryContract<MockBase>, CwOrchError> {
    setup_factory_inner(
        interchain,
        factory_chain_id,
        router_chain_id,
        router,
        ChainType::Cosmos(CosmosChain {
            chain_id: factory_chain_id.to_string(),
        }),
    )
}

pub fn setup_factory_evm(
    interchain: &MockInterchainEnv,
    factory_chain_id: &str,
    router_chain_id: &str,
    router: &RouterContract<MockBase>,
) -> Result<FactoryContract<MockBase>, CwOrchError> {
    setup_factory_inner(
        interchain,
        factory_chain_id,
        router_chain_id,
        router,
        ChainType::Evm(EvmChain {
            chain_id: factory_chain_id.to_string(),
        }),
    )
}

fn setup_factory_inner(
    interchain: &MockInterchainEnv,
    factory_chain_id: &str,
    router_chain_id: &str,
    router: &RouterContract<MockBase>,
    chain_type: ChainType,
) -> Result<FactoryContract<MockBase>, CwOrchError> {
    let chain_uid = ChainUid::create(factory_chain_id.to_string()).unwrap();
    let chain = interchain.get_chain(factory_chain_id).unwrap();
    let _router_chain = interchain.get_chain(router_chain_id).unwrap();
    let factory = FactoryContract::new(chain.clone());
    let escrow = EscrowContract::new(chain.clone());
    let lp_token = LpTokenContract::new(chain.clone());
    let relayer = setup_relayer(&chain)?;

    let string_length = factory_chain_id.len();

    factory.upload().unwrap();
    escrow.upload().unwrap();
    lp_token.upload().unwrap();

    let is_native = router_chain_id == factory_chain_id;

    for _ in 0..string_length {
        // Do this in order to randomly generate the factory address
        factory.instantiate(
            &euclid::msgs::factory::InstantiateMsg {
                router_contract: router.address().unwrap().to_string(),
                chain_uid: chain_uid.clone(),
                escrow_code_id: escrow.code_id().unwrap(),
                lp_code_id: lp_token.code_id().unwrap(),
                relayer_contract: relayer.address().unwrap(),
                rate_limit_fee_recipient: chain.addr_make("rate_limit_fee_recipient"),
                rate_limit_fee_denom: "ufee".to_string(),
                rate_limit_free_limit: Uint128::from(10u128),
                is_native,
            },
            None,
            &[],
        )?;
    }

    if !is_native {
        match chain_type {
            ChainType::Cosmos(_) => {
                let chain_info = euclid::msgs::router::RegisterFactoryChainType::Cosmos(
                    RegisterFactoryChainCosmos {
                        factory_address: factory.address().unwrap().to_string(),
                        factory_chain_id: factory.environment().chain_id(),
                    },
                );
                let register_request = router
                    .register_factory(chain_info, chain_uid.clone())
                    .unwrap();
                let ack_events =
                    relay_router_send_packet(register_request.events, &factory, &chain_uid)?;
                relay_router_ack_packet(router, &chain_uid, ack_events)?;
            }
            ChainType::Evm(_) => {
                let chain_info =
                    euclid::msgs::router::RegisterFactoryChainType::Evm(RegisterFactoryChainEvm {
                        factory_address: factory.address().unwrap().to_string(),
                        factory_chain_id: factory.environment().chain_id(),
                    });
                let register_request = router
                    .register_factory(chain_info, chain_uid.clone())
                    .unwrap();
                let send_packet_events = extract_send_packet_events(&register_request.events);
                let packet = send_packet_events.first().unwrap();
                let msg: FactoryCrossChainExecuteMsg = from_json(&packet.msg).unwrap();
                let tx_id = msg.get_tx_id();

                let _relay_ack_events = ack_register_factory_evm(
                    router,
                    &chain_uid,
                    factory.address().unwrap().as_str(),
                    &factory.environment().chain_id(),
                    &tx_id,
                    packet.sequence,
                )?;
            }
            ChainType::Native {} => {
                unreachable!("native chains are handled by is_native branch")
            }
        }
    } else {
        let chain_info =
            euclid::msgs::router::RegisterFactoryChainType::Native(RegisterFactoryChainNative {
                factory_address: factory.address().unwrap().to_string(),
                factory_chain_id: factory.environment().chain_id(),
            });
        router.register_factory(chain_info, chain_uid.clone())?;
    }
    let all_chains = router.get_all_chains().unwrap();
    // Assert that this chain is registered
    assert!(
        all_chains
            .chains
            .iter()
            .any(|c| c.chain_uid == chain_uid.clone()),
        "Factory chain not registered",
    );

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
            relayer_contract: relayer.address().unwrap(),
            release_fee_recipient: chain.addr_make("release_fee_recipient"),
            default_fee_recipient: chain.addr_make("default_fee_recipient"),
        },
        None,
        &[],
    )?;

    let meta_transaction_contract = setup_meta_transaction_contract(&router)?;
    router.manage_router_state(ManageRouterState::MetaTransactionContract {
        meta_transaction_contract: meta_transaction_contract.address().unwrap(),
    })?;

    Ok(router)
}

pub fn setup_relayer(chain: &MockBase) -> Result<RelayerContract<MockBase>, CwOrchError> {
    let relayer = RelayerContract::new(chain.clone());
    let (_, pubkey_binary) = get_signer_key();

    let validator_address = cosmos_address_from_pubkey(&pubkey_binary, "cosmos").unwrap();

    relayer.upload().unwrap();

    let validator = Validator {
        pubkey: pubkey_binary,
        address: validator_address,
    };

    relayer.instantiate(
        &relayer::msgs::InstantiateMsg {
            message_signer: validator.clone(),
            signature_threshold: 1,
            validators: vec![validator],
        },
        Some(&chain.sender),
        &[],
    )?;

    Ok(relayer)
}

pub fn setup_claimer(
    router: &RouterContract<MockBase>,
    vcoin_address: &VirtualBalanceContract<MockBase>,
) -> Result<ClaimerContract<MockBase>, CwOrchError> {
    let chain = router.environment().clone();
    let claimer = ClaimerContract::new(chain.clone());
    claimer.upload().unwrap();
    claimer.instantiate(
        &euclid::msgs::claimer::msg::InstantiateMsg {
            router_contract: router.address().unwrap(),
            vcoin_address: vcoin_address.address().unwrap(),
        },
        None,
        &[],
    )?;
    Ok(claimer)
}

pub fn setup_meta_transaction_contract(
    router: &RouterContract<MockBase>,
) -> Result<MetaTransactionContract<MockBase>, CwOrchError> {
    let chain = router.environment().clone();
    let meta_transaction_contract = MetaTransactionContract::new(chain.clone());
    meta_transaction_contract.upload().unwrap();
    meta_transaction_contract.instantiate(
        &euclid::msgs::meta_transaction::msg::InstantiateMsg {
            router_contract: router.address().unwrap(),
        },
        None,
        &[],
    )?;
    Ok(meta_transaction_contract)
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

pub fn get_lp_token(chain: &MockBase, address: &Addr) -> LpTokenContract<MockBase> {
    let mut lp_token = LpTokenContract::new(chain.clone());
    lp_token.as_instance_mut().id = format!("lp_token_{}", address);
    lp_token.set_address(address);
    lp_token
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
