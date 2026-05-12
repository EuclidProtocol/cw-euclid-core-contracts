#![cfg(not(target_arch = "wasm32"))]
use cosmwasm_std::Addr;

use crate::helpers::chains::{setup_factory, setup_factory_evm};
use crate::helpers::multi_chain::MultiChainEnv;
use crate::tests_reusable::constants::ROUTER_CHAIN_ID;

#[derive(Clone, Copy, Debug)]
pub enum FactorySetupMode {
    Native,
    Ibc,
    Evm,
}

pub fn setup_factory_with_mode(
    env: &mut MultiChainEnv,
    factory_chain_id: &str,
    router_chain_id: &str,
    router_addr: &Addr,
    mode: FactorySetupMode,
) -> Result<Addr, anyhow::Error> {
    match mode {
        FactorySetupMode::Native | FactorySetupMode::Ibc => {
            setup_factory(env, factory_chain_id, router_chain_id, router_addr)
        }
        FactorySetupMode::Evm => {
            setup_factory_evm(env, factory_chain_id, router_chain_id, router_addr)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helpers::chains::{setup_interchain, setup_router};
    use crate::tests_reusable::constants::{
        FACTORY_CHAIN_ID_EVM, FACTORY_CHAIN_ID_IBC, FACTORY_CHAIN_ID_LOCAL,
    };
    use euclid::chain::{ChainType, ChainUid};
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
        let mut env = setup_interchain(sender, factory_chain_id);
        let router_addr =
            setup_router(env.chain_mut(ROUTER_CHAIN_ID), vec![factory_chain_id]).unwrap();

        let _factory_addr = setup_factory_with_mode(
            &mut env,
            factory_chain_id,
            ROUTER_CHAIN_ID,
            &router_addr,
            mode,
        )
        .unwrap();

        let chain_uid = ChainUid::create(factory_chain_id.to_string()).unwrap();
        let all_chains: euclid::msgs::router::AllChainResponse = env.chain(ROUTER_CHAIN_ID).query(
            &router_addr,
            &euclid::msgs::router::QueryMsg::GetAllChains {},
        );
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
