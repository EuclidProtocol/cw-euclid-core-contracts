#![cfg(not(target_arch = "wasm32"))]
use cosmwasm_std::{from_json, Uint128};
use cw_orch::{mock::MockBase, prelude::*};
use cw_orch_interchain::mock::MockInterchainEnv;
use cw_orch_interchain::prelude::IbcQueryHandler;
use cw_orch_interchain::prelude::InterchainEnv;
use escrow::EscrowContract;
use euclid::chain::ChainType;
use euclid::msgs::factory::msg::ExecuteMsgFns as FactoryExecuteMsgFns;
use euclid::msgs::router::execute::ExecuteMsgFns as RouterExecuteMsgFns;
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
use euclid::msgs::router::{RegisterFactoryChainCosmos, RegisterFactoryChainNative};
use euclid::{chain::ChainUid, msgs::router::RegisterFactoryChainEvm};
use euclid_ibc::factory_ibc::FactoryCrossChainExecuteMsg;
use factory::FactoryContract;
use lp_token::LpTokenContract;
use position_token::PositionTokenContract;
use router::RouterContract;

use crate::helpers::chains::setup_relayer;
use crate::helpers::relayer::{
    ack_register_factory_evm, extract_send_packet_events, relay_router_ack_packet,
    relay_router_send_packet,
};
use crate::tests_reusable::constants::ROUTER_CHAIN_ID;

#[derive(Clone, Copy, Debug)]
pub enum FactorySetupMode {
    Native,
    Ibc,
    Evm,
}

pub fn setup_factory(
    interchain: &MockInterchainEnv,
    factory_chain_id: &str,
    router: &RouterContract<MockBase>,
) -> Result<FactoryContract<MockBase>, CwOrchError> {
    let mode = if ROUTER_CHAIN_ID == factory_chain_id {
        FactorySetupMode::Native
    } else {
        FactorySetupMode::Ibc
    };
    setup_factory_with_mode(interchain, factory_chain_id, router, mode)
}

pub fn setup_factory_evm(
    interchain: &MockInterchainEnv,
    factory_chain_id: &str,
    router: &RouterContract<MockBase>,
) -> Result<FactoryContract<MockBase>, CwOrchError> {
    setup_factory_with_mode(interchain, factory_chain_id, router, FactorySetupMode::Evm)
}

pub fn setup_factory_with_mode(
    interchain: &MockInterchainEnv,
    factory_chain_id: &str,
    router: &RouterContract<MockBase>,
    mode: FactorySetupMode,
) -> Result<FactoryContract<MockBase>, CwOrchError> {
    let chain_uid = ChainUid::create(factory_chain_id.to_string()).unwrap();
    let vsl_chain_uid = ChainUid::vsl_chain_uid().unwrap();
    let chain = interchain.get_chain(factory_chain_id).unwrap();
    let factory = FactoryContract::new(chain.clone());
    let escrow = EscrowContract::new(chain.clone());
    let lp_token = LpTokenContract::new(chain.clone());
    let position_token = PositionTokenContract::new(chain.clone());
    let relayer = setup_relayer(&chain, vec![vsl_chain_uid.as_str(), chain_uid.as_str()])?;

    let string_length = factory_chain_id.len();

    factory.upload().unwrap();
    escrow.upload().unwrap();
    lp_token.upload().unwrap();
    position_token.upload().unwrap();

    let is_native = matches!(mode, FactorySetupMode::Native);

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

    position_token.instantiate(
        &position_token::msg::InstantiateMsg {
            name: "Euclid Concentrated Positions".to_string(),
            symbol: "EUPOS".to_string(),
            minter: factory.address().unwrap(),
            admin: chain.addr_make("position_token_admin"),
        },
        None,
        &[],
    )?;
    factory.manage_factory_state(
        euclid::msgs::factory::ManageFactoryState::UpdatePositionTokenContract {
            position_token_contract: position_token.address().unwrap().to_string(),
        },
    )?;

    if !is_native {
        match mode {
            FactorySetupMode::Ibc => {
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
            FactorySetupMode::Evm => {
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
            FactorySetupMode::Native => unreachable!("native setup handled by is_native branch"),
        }
    } else {
        let chain_info =
            euclid::msgs::router::RegisterFactoryChainType::Native(RegisterFactoryChainNative {
                factory_address: factory.address().unwrap().to_string(),
                factory_chain_id: factory.environment().chain_id(),
            });
        router.register_factory(chain_info, chain_uid.clone())?;
    }

    Ok(factory)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helpers::chains::setup_router;
    use crate::tests_reusable::constants::{
        FACTORY_CHAIN_ID_EVM, FACTORY_CHAIN_ID_IBC, FACTORY_CHAIN_ID_LOCAL,
    };
    use rstest::rstest;

    #[rstest]
    #[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
    #[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
    #[case(FactorySetupMode::Evm, FACTORY_CHAIN_ID_EVM)]
    fn setup_factory_registers_chain(
        #[case] mode: FactorySetupMode,
        #[case] factory_chain_id: &str,
    ) {
        let sender = "sender_for_all_chains";
        let mut chains = vec![(ROUTER_CHAIN_ID, sender)];
        if ROUTER_CHAIN_ID != factory_chain_id {
            chains.push((factory_chain_id, sender));
        }

        let interchain = MockInterchainEnv::new(chains);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![factory_chain_id]).unwrap();

        let _factory =
            setup_factory_with_mode(&interchain, factory_chain_id, &router, mode).unwrap();

        let chain_uid = ChainUid::create(factory_chain_id.to_string()).unwrap();
        let all_chains = router.get_all_chains().unwrap();
        let registered_chain = all_chains
            .chains
            .iter()
            .find(|c| c.chain_uid == chain_uid)
            .expect("factory chain not registered");

        match mode {
            FactorySetupMode::Native => {
                assert!(matches!(
                    registered_chain.chain.chain_type,
                    ChainType::Native {}
                ));
            }
            FactorySetupMode::Ibc => {
                let chain_id = match &registered_chain.chain.chain_type {
                    ChainType::Cosmos(chain) => chain.chain_id.as_str(),
                    _ => panic!("expected cosmos chain type"),
                };
                assert_eq!(chain_id, factory_chain_id);
            }
            FactorySetupMode::Evm => {
                let chain_id = match &registered_chain.chain.chain_type {
                    ChainType::Evm(chain) => chain.chain_id.as_str(),
                    _ => panic!("expected evm chain type"),
                };
                assert_eq!(chain_id, factory_chain_id);
            }
        }
    }
}
