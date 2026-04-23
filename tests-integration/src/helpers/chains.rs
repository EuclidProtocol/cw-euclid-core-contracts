#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{Addr, Uint128};
use cw_multi_test::ContractWrapper;
use euclid::chain::{ChainType, ChainUid, CosmosChain, EvmChain};
use euclid::msgs::router::{
    ManageRouterState, RegisterFactoryChainCosmos, RegisterFactoryChainNative,
    RegisterFactoryChainType,
};
use relayer::verify::cosmos_address_from_pubkey;
use relayer::Validator;

use crate::helpers::app::EuclidApp;
use crate::helpers::multi_chain::MultiChainEnv;
use crate::helpers::relayer::{
    ack_register_factory_evm, extract_send_packet_events, get_signer_key, relay_router_ack_packet,
    relay_router_send_packet,
};
use crate::tests_reusable::constants::ROUTER_CHAIN_ID;

// ---------------------------------------------------------------------------
// Contract code-store helpers
// ---------------------------------------------------------------------------

pub fn router_code(app: &mut EuclidApp) -> u64 {
    app.store_code(Box::new(
        ContractWrapper::new_with_empty(
            router::contract::execute,
            router::contract::instantiate,
            router::contract::query,
        )
        .with_reply(router::contract::reply),
    ))
}

pub fn virtual_balance_code(app: &mut EuclidApp) -> u64 {
    app.store_code(Box::new(ContractWrapper::new_with_empty(
        virtual_balance::contract::execute,
        virtual_balance::contract::instantiate,
        virtual_balance::contract::query,
    )))
}

pub fn cp_vlp_code(app: &mut EuclidApp) -> u64 {
    app.store_code(Box::new(
        ContractWrapper::new_with_empty(
            cp_vlp::contract::execute,
            cp_vlp::contract::instantiate,
            cp_vlp::contract::query,
        )
        .with_reply(cp_vlp::contract::reply),
    ))
}

pub fn stable_vlp_code(app: &mut EuclidApp) -> u64 {
    app.store_code(Box::new(
        ContractWrapper::new_with_empty(
            stable_vlp::contract::execute,
            stable_vlp::contract::instantiate,
            stable_vlp::contract::query,
        )
        .with_reply(stable_vlp::contract::reply),
    ))
}

pub fn relayer_code(app: &mut EuclidApp) -> u64 {
    app.store_code(Box::new(ContractWrapper::new_with_empty(
        euclid_relayer::contract::execute,
        euclid_relayer::contract::instantiate,
        euclid_relayer::contract::query,
    )))
}

pub fn meta_transaction_code(app: &mut EuclidApp) -> u64 {
    app.store_code(Box::new(ContractWrapper::new_with_empty(
        meta_transaction::contract::execute,
        meta_transaction::contract::instantiate,
        meta_transaction::contract::query,
    )))
}

pub fn factory_code(app: &mut EuclidApp) -> u64 {
    app.store_code(Box::new(
        ContractWrapper::new_with_empty(
            factory::contract::execute,
            factory::contract::instantiate,
            factory::contract::query,
        )
        .with_reply(factory::contract::reply),
    ))
}

pub fn escrow_code(app: &mut EuclidApp) -> u64 {
    app.store_code(Box::new(
        ContractWrapper::new_with_empty(
            escrow::contract::execute,
            escrow::contract::instantiate,
            escrow::contract::query,
        )
        .with_reply(escrow::contract::reply),
    ))
}

pub fn lp_token_code(app: &mut EuclidApp) -> u64 {
    app.store_code(Box::new(ContractWrapper::new_with_empty(
        lp_token::contract::execute,
        lp_token::contract::instantiate,
        lp_token::contract::query,
    )))
}

pub fn claimer_code(app: &mut EuclidApp) -> u64 {
    app.store_code(Box::new(ContractWrapper::new_with_empty(
        claimer::contract::execute,
        claimer::contract::instantiate,
        claimer::contract::query,
    )))
}

pub fn orderbook_deposits_code(app: &mut EuclidApp) -> u64 {
    app.store_code(Box::new(ContractWrapper::new_with_empty(
        orderbook_deposits::contract::execute,
        orderbook_deposits::contract::instantiate,
        orderbook_deposits::contract::query,
    )))
}

// ---------------------------------------------------------------------------
// Setup helpers
// ---------------------------------------------------------------------------

pub fn setup_interchain(sender: &str, factory_chain_id: &str) -> MultiChainEnv {
    let mut chains = vec![(ROUTER_CHAIN_ID, sender)];
    if ROUTER_CHAIN_ID != factory_chain_id {
        chains.push((factory_chain_id, sender));
    }
    MultiChainEnv::new(chains)
}

pub fn setup_router(app: &mut EuclidApp, factory_chains: Vec<&str>) -> Result<Addr, anyhow::Error> {
    let vlp_code_id = cp_vlp_code(app);
    let stable_vlp_code_id = stable_vlp_code(app);
    let vb_code_id = virtual_balance_code(app);
    let router_code_id = router_code(app);
    let relayer_addr = setup_relayer(app, factory_chains)?;

    let sender = app.sender();
    let router_addr = app.instantiate(
        router_code_id,
        &sender,
        &euclid::msgs::router::InstantiateMsg {
            constant_product_vlp_code_id: vlp_code_id,
            stable_vlp_code_id,
            virtual_balance_code_id: vb_code_id,
            relayer_contract: relayer_addr,
            release_fee_recipient: app.addr_make("release_fee_recipient"),
            default_fee_recipient: app.addr_make("default_fee_recipient"),
        },
        &[],
        "router",
    );

    let meta_tx_addr = setup_meta_transaction_contract(app, &router_addr)?;
    app.execute(
        &sender,
        &router_addr,
        &euclid::msgs::router::ExecuteMsg::ManageRouterState(
            ManageRouterState::MetaTransactionContract {
                meta_transaction_contract: meta_tx_addr,
            },
        ),
        &[],
    );

    Ok(router_addr)
}

pub fn setup_relayer(app: &mut EuclidApp, chain_uids: Vec<&str>) -> Result<Addr, anyhow::Error> {
    let code_id = relayer_code(app);
    let (_, pubkey_binary) = get_signer_key();

    let validator_address = cosmos_address_from_pubkey(&pubkey_binary, "cosmos").unwrap();

    let validator = Validator {
        pubkey: pubkey_binary,
        address: validator_address,
    };

    let sender = app.sender();
    let relayer_addr = app.instantiate(
        code_id,
        &sender,
        &relayer::msgs::InstantiateMsg {
            message_signer: validator.clone(),
            signature_threshold: 1,
        },
        &[],
        "relayer",
    );

    for chain_uid in chain_uids {
        app.execute(
            &sender,
            &relayer_addr,
            &relayer::ExecuteMsg::AddValidator {
                chain_uid: ChainUid::create(chain_uid.to_string()).unwrap(),
                validator: validator.clone(),
            },
            &[],
        );
    }
    Ok(relayer_addr)
}

pub fn setup_meta_transaction_contract(
    app: &mut EuclidApp,
    router_addr: &Addr,
) -> Result<Addr, anyhow::Error> {
    let code_id = meta_transaction_code(app);
    let sender = app.sender();
    let addr = app.instantiate(
        code_id,
        &sender,
        &euclid::msgs::meta_transaction::msg::InstantiateMsg {
            router_contract: router_addr.clone(),
        },
        &[],
        "meta_transaction",
    );
    Ok(addr)
}

pub fn setup_claimer(
    app: &mut EuclidApp,
    router_addr: &Addr,
    vcoin_address: &Addr,
) -> Result<Addr, anyhow::Error> {
    let code_id = claimer_code(app);
    let sender = app.sender();
    let addr = app.instantiate(
        code_id,
        &sender,
        &euclid::msgs::claimer::msg::InstantiateMsg {
            router_contract: router_addr.clone(),
            vcoin_address: vcoin_address.clone(),
        },
        &[],
        "claimer",
    );
    Ok(addr)
}

pub fn setup_factory(
    env: &mut MultiChainEnv,
    factory_chain_id: &str,
    router_chain_id: &str,
    router_addr: &Addr,
) -> Result<Addr, anyhow::Error> {
    setup_factory_inner(
        env,
        factory_chain_id,
        router_chain_id,
        router_addr,
        ChainType::Cosmos(CosmosChain {
            chain_id: factory_chain_id.to_string(),
        }),
    )
}

pub fn setup_factory_evm(
    env: &mut MultiChainEnv,
    factory_chain_id: &str,
    router_chain_id: &str,
    router_addr: &Addr,
) -> Result<Addr, anyhow::Error> {
    setup_factory_inner(
        env,
        factory_chain_id,
        router_chain_id,
        router_addr,
        ChainType::Evm(EvmChain {
            chain_id: factory_chain_id.to_string(),
        }),
    )
}

fn setup_factory_inner(
    env: &mut MultiChainEnv,
    factory_chain_id: &str,
    router_chain_id: &str,
    router_addr: &Addr,
    chain_type: ChainType,
) -> Result<Addr, anyhow::Error> {
    let chain_uid = ChainUid::create(factory_chain_id.to_string()).unwrap();
    let vsl_chain_uid = ChainUid::vsl_chain_uid().unwrap();

    // Store codes on the factory chain
    let factory_chain = env.chain_mut(factory_chain_id);
    let factory_code_id = factory_code(factory_chain);
    let escrow_code_id = escrow_code(factory_chain);
    let lp_code_id = lp_token_code(factory_chain);
    let relayer_addr_factory = setup_relayer(
        factory_chain,
        vec![vsl_chain_uid.as_str(), chain_uid.as_str()],
    )?;

    let is_native = router_chain_id == factory_chain_id;
    let rate_limit_fee_recipient = factory_chain.addr_make("rate_limit_fee_recipient");

    // Instantiate factory multiple times to randomize its address (mirrors old cw-orch behaviour).
    let string_length = factory_chain_id.len();
    let factory_sender = factory_chain.sender();
    let router_addr_str = router_addr.to_string();

    let mut factory_addr = Addr::unchecked("");
    for _ in 0..string_length {
        factory_addr = factory_chain.instantiate(
            factory_code_id,
            &factory_sender,
            &euclid::msgs::factory::InstantiateMsg {
                router_contract: router_addr_str.clone(),
                chain_uid: chain_uid.clone(),
                escrow_code_id,
                lp_code_id,
                relayer_contract: relayer_addr_factory.clone(),
                rate_limit_fee_recipient: rate_limit_fee_recipient.clone(),
                rate_limit_fee_denom: "ufee".to_string(),
                rate_limit_free_limit: Uint128::from(10u128),
                is_native,
            },
            &[],
            "factory",
        );
    }

    if !is_native {
        match chain_type {
            ChainType::Cosmos(_) => {
                let chain_info = RegisterFactoryChainType::Cosmos(RegisterFactoryChainCosmos {
                    factory_address: factory_addr.to_string(),
                    factory_chain_id: factory_chain_id.to_string(),
                });
                // Execute register_factory on the router chain
                let router_sender = env.chain(router_chain_id).sender();
                let register_response = env.chain_mut(router_chain_id).execute(
                    &router_sender,
                    router_addr,
                    &euclid::msgs::router::ExecuteMsg::RegisterFactory {
                        chain_info,
                        chain_uid: chain_uid.clone(),
                    },
                    &[],
                );
                let ack_events = relay_router_send_packet(
                    register_response.events,
                    &factory_addr,
                    &chain_uid,
                    env.chain_mut(factory_chain_id),
                )?;
                relay_router_ack_packet(
                    router_addr,
                    &chain_uid,
                    ack_events,
                    env.chain_mut(router_chain_id),
                )?;
            }
            ChainType::Evm(_) => {
                let chain_info =
                    RegisterFactoryChainType::Evm(euclid::msgs::router::RegisterFactoryChainEvm {
                        factory_address: factory_addr.to_string(),
                        factory_chain_id: factory_chain_id.to_string(),
                    });
                let router_sender = env.chain(router_chain_id).sender();
                let register_response = env.chain_mut(router_chain_id).execute(
                    &router_sender,
                    router_addr,
                    &euclid::msgs::router::ExecuteMsg::RegisterFactory {
                        chain_info,
                        chain_uid: chain_uid.clone(),
                    },
                    &[],
                );
                let send_packet_events = extract_send_packet_events(&register_response.events);
                let packet = send_packet_events.first().unwrap();
                let msg: euclid_ibc::factory_ibc::FactoryCrossChainExecuteMsg =
                    cosmwasm_std::from_json(&packet.msg).unwrap();
                let tx_id = msg.get_tx_id();

                ack_register_factory_evm(
                    router_addr,
                    env.chain_mut(router_chain_id),
                    &chain_uid,
                    &factory_addr.to_string(),
                    factory_chain_id,
                    &tx_id,
                    packet.sequence,
                )?;
            }
            ChainType::Native {} => {
                unreachable!("native chains are handled by is_native branch")
            }
        }
    } else {
        let chain_info = RegisterFactoryChainType::Native(RegisterFactoryChainNative {
            factory_address: factory_addr.to_string(),
            factory_chain_id: factory_chain_id.to_string(),
        });
        let router_sender = env.chain(router_chain_id).sender();
        env.chain_mut(router_chain_id).execute(
            &router_sender,
            router_addr,
            &euclid::msgs::router::ExecuteMsg::RegisterFactory {
                chain_info,
                chain_uid: chain_uid.clone(),
            },
            &[],
        );
    }

    // Assert the chain was registered
    let all_chains: euclid::msgs::router::AllChainResponse = env.chain(router_chain_id).query(
        router_addr,
        &euclid::msgs::router::QueryMsg::GetAllChains {},
    );
    assert!(
        all_chains.chains.iter().any(|c| c.chain_uid == chain_uid),
        "Factory chain not registered"
    );

    Ok(factory_addr)
}

// ---------------------------------------------------------------------------
// Address-only handle helpers (replaces get_* with MockBase)
// ---------------------------------------------------------------------------

pub fn get_escrow_addr(app: &EuclidApp, factory_addr: &Addr, token: &str) -> Addr {
    let response: euclid::msgs::factory::GetEscrowResponse = app.query(
        factory_addr,
        &euclid::msgs::factory::QueryMsg::GetEscrow {
            token_id: token.to_string(),
        },
    );
    let escrow_address = response.escrow_address.expect("escrow not found");
    println!("Token: {:?} Escrow address: {:?}", token, escrow_address);
    escrow_address
}

pub fn get_relayer_addr(app: &EuclidApp, router_or_factory_addr: &Addr) -> Addr {
    // Try router query first, fall back to factory query
    // This is used from relayer.rs — caller passes the correct contract type
    // We expose separate helpers per contract type for clarity.
    router_or_factory_addr.clone() // placeholder — use typed helpers below
}

pub fn get_router_relayer_addr(app: &EuclidApp, router_addr: &Addr) -> Addr {
    let resp: euclid::msgs::router::QueryRelayerAddressesResponse = app.query(
        router_addr,
        &euclid::msgs::router::QueryMsg::QueryRelayerAddresses {},
    );
    resp.relayer_contract
}

pub fn get_factory_relayer_addr(app: &EuclidApp, factory_addr: &Addr) -> Addr {
    let resp: euclid::msgs::factory::StateResponse =
        app.query(factory_addr, &euclid::msgs::factory::QueryMsg::GetState {});
    resp.relayer_contract
}

pub fn get_virtual_balance_addr(app: &EuclidApp, router_addr: &Addr) -> Addr {
    let resp: euclid::msgs::router::StateResponse =
        app.query(router_addr, &euclid::msgs::router::QueryMsg::GetState {});
    resp.virtual_balance_address
}

pub fn get_meta_tx_addr(app: &EuclidApp, router_addr: &Addr) -> Addr {
    router::state::META_TRANSACTION_CONTRACT
        .query(&app.app().wrap(), router_addr.clone())
        .expect("meta_transaction_contract not set")
}
