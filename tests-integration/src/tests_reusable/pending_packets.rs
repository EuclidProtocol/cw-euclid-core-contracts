#![cfg(not(target_arch = "wasm32"))]

#[cfg(test)]
mod tests {
    use cosmwasm_std::{from_json, Addr};
    use cw_orch::mock::MockBase;
    use cw_orch::prelude::{ContractInstance, Environment};
    use cw_orch_interchain::prelude::InterchainEnv;
    use euclid::chain::ChainUid;
    use euclid::msgs::cross_chain_config::CrossChainConfig;
    use euclid::msgs::factory::msg::ExecuteMsgFns as FactoryExecuteMsgFns;
    use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
    use euclid::msgs::router::execute::ExecuteMsgFns as RouterExecuteMsgFns;
    use euclid::msgs::router::{RegisterFactoryChainEvm, RegisterFactoryChainType};
    use euclid::token::{Token, TokenType, TokenWithDenom};
    use euclid_ibc::factory_ibc::FactoryCrossChainExecuteMsg;
    use factory::FactoryContract;
    use router::RouterContract;

    use crate::helpers::chains::{setup_factory, setup_interchain, setup_router};
    use crate::helpers::relayer::{
        ack_register_factory_evm, extract_send_packet_events, relay_factory_ack_packet,
        relay_factory_send_packet,
    };
    use crate::tests_reusable::constants::{
        FACTORY_CHAIN_ID_EVM, FACTORY_CHAIN_ID_IBC, ROUTER_CHAIN_ID,
    };

    fn query_factory_user_pending_count(factory: &FactoryContract<MockBase>, sender: Addr) -> u128 {
        factory::rate_limit::USER_PENDING_PACKETS_COUNT
            .query(
                &factory.environment().app.borrow().wrap(),
                factory.address().unwrap(),
                sender,
            )
            .unwrap()
            .unwrap_or(0)
    }

    fn query_factory_pending_count(factory: &FactoryContract<MockBase>) -> u128 {
        let raw = factory
            .environment()
            .app
            .borrow()
            .wrap()
            .query_wasm_raw(
                factory.address().unwrap(),
                factory::relay_state::CROSS_CHAIN_PENDING_PACKETS_COUNT.as_slice(),
            )
            .unwrap();

        raw.map(|value| from_json::<u128>(value).unwrap())
            .unwrap_or(0)
    }

    fn query_router_pending_count(router: &RouterContract<MockBase>, chain_uid: ChainUid) -> u128 {
        router::relay_state::CROSS_CHAIN_PENDING_PACKETS_COUNT
            .query(
                &router.environment().app.borrow().wrap(),
                router.address().unwrap(),
                chain_uid,
            )
            .unwrap()
            .unwrap_or(0)
    }

    fn factory_pending_packet_exists(factory: &FactoryContract<MockBase>, sequence: u128) -> bool {
        factory::relay_state::CROSS_CHAIN_PENDING_SEND_PACKETS
            .query(
                &factory.environment().app.borrow().wrap(),
                factory.address().unwrap(),
                sequence,
            )
            .unwrap()
            .is_some()
    }

    fn router_pending_packet_exists(
        router: &RouterContract<MockBase>,
        chain_uid: ChainUid,
        sequence: u128,
    ) -> bool {
        router::relay_state::CROSS_CHAIN_PENDING_SEND_PACKETS
            .query(
                &router.environment().app.borrow().wrap(),
                router.address().unwrap(),
                (chain_uid, sequence),
            )
            .unwrap()
            .is_some()
    }

    #[test]
    fn factory_pending_packet_counts_increment_and_decrement() {
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, FACTORY_CHAIN_ID_IBC);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![FACTORY_CHAIN_ID_IBC]).unwrap();
        let factory =
            setup_factory(&interchain, FACTORY_CHAIN_ID_IBC, ROUTER_CHAIN_ID, &router).unwrap();

        let sender_addr = factory.environment().sender.clone();
        let factory_chain_uid = factory.get_state().unwrap().chain_uid;
        let token = TokenWithDenom {
            token: Token::create("pendingcheck".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "pendingcheck".to_string(),
                decimals: Some(18),
            },
        };

        let tx = factory
            .register_denom(CrossChainConfig::default(), token)
            .unwrap();

        assert_eq!(
            query_factory_user_pending_count(&factory, sender_addr.clone()),
            1
        );
        assert_eq!(query_factory_pending_count(&factory), 1);
        assert!(factory_pending_packet_exists(&factory, 0));

        let ack_events = relay_factory_send_packet(tx.events, &router).unwrap();
        assert_eq!(
            query_factory_user_pending_count(&factory, sender_addr.clone()),
            1
        );
        assert_eq!(query_factory_pending_count(&factory), 1);

        relay_factory_ack_packet(&factory, ack_events, &factory_chain_uid).unwrap();
        assert_eq!(query_factory_user_pending_count(&factory, sender_addr), 0);
        assert_eq!(query_factory_pending_count(&factory), 0);
    }

    #[test]
    fn factory_pending_send_packets_keep_existing_sequences() {
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, FACTORY_CHAIN_ID_IBC);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![FACTORY_CHAIN_ID_IBC]).unwrap();
        let factory =
            setup_factory(&interchain, FACTORY_CHAIN_ID_IBC, ROUTER_CHAIN_ID, &router).unwrap();
        let sender_addr = factory.environment().sender.clone();

        let token_one = TokenWithDenom {
            token: Token::create("pendingone".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "pendingone".to_string(),
                decimals: Some(18),
            },
        };
        let token_two = TokenWithDenom {
            token: Token::create("pendingtwo".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "pendingtwo".to_string(),
                decimals: Some(18),
            },
        };

        factory
            .register_denom(CrossChainConfig::default(), token_one)
            .unwrap();
        factory
            .register_denom(CrossChainConfig::default(), token_two)
            .unwrap();

        assert!(factory_pending_packet_exists(&factory, 0));
        assert!(factory_pending_packet_exists(&factory, 1));
        assert_eq!(query_factory_user_pending_count(&factory, sender_addr), 2);
        assert_eq!(query_factory_pending_count(&factory), 2);
    }

    #[test]
    fn factory_rate_limit_does_not_accumulate_after_successful_acks() {
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, FACTORY_CHAIN_ID_IBC);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![FACTORY_CHAIN_ID_IBC]).unwrap();
        let factory =
            setup_factory(&interchain, FACTORY_CHAIN_ID_IBC, ROUTER_CHAIN_ID, &router).unwrap();

        let sender_addr = factory.environment().sender.clone();
        for i in 0..12u8 {
            let token_id = format!("ratelimit{i}");
            let token = TokenWithDenom {
                token: Token::create(token_id.clone()).unwrap(),
                token_type: TokenType::Native {
                    denom: token_id,
                    decimals: Some(18),
                },
            };
            let tx = factory
                .register_denom(CrossChainConfig::default(), token)
                .unwrap();
            let chain_uid = factory.get_state().unwrap().chain_uid;
            let ack_events = relay_factory_send_packet(tx.events, &router).unwrap();
            relay_factory_ack_packet(&factory, ack_events, &chain_uid).unwrap();
        }

        assert_eq!(query_factory_user_pending_count(&factory, sender_addr), 0);
        assert_eq!(query_factory_pending_count(&factory), 0);
    }

    #[test]
    fn router_pending_packet_counts_increment_and_decrement_for_evm_registration() {
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, FACTORY_CHAIN_ID_EVM);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let factory_chain = interchain.get_chain(FACTORY_CHAIN_ID_EVM).unwrap();
        let router = setup_router(&router_chain, vec![FACTORY_CHAIN_ID_EVM]).unwrap();

        let chain_uid = ChainUid::create(FACTORY_CHAIN_ID_EVM.to_string()).unwrap();
        let factory_address = factory_chain.addr_make("pending-factory").to_string();
        let register = router
            .register_factory(
                RegisterFactoryChainType::Evm(RegisterFactoryChainEvm {
                    factory_address: factory_address.clone(),
                    factory_chain_id: FACTORY_CHAIN_ID_EVM.to_string(),
                }),
                chain_uid.clone(),
            )
            .unwrap();

        assert_eq!(query_router_pending_count(&router, chain_uid.clone()), 1);

        let packet = extract_send_packet_events(&register.events)
            .into_iter()
            .next()
            .unwrap();
        let packet_msg: FactoryCrossChainExecuteMsg = from_json(&packet.msg).unwrap();
        let tx_id = packet_msg.get_tx_id();
        ack_register_factory_evm(
            &router,
            &chain_uid,
            &factory_address,
            FACTORY_CHAIN_ID_EVM,
            &tx_id,
            packet.sequence,
        )
        .unwrap();

        assert_eq!(query_router_pending_count(&router, chain_uid), 0);
    }

    #[test]
    fn router_pending_send_packets_keep_existing_sequences() {
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, FACTORY_CHAIN_ID_EVM);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let factory_chain = interchain.get_chain(FACTORY_CHAIN_ID_EVM).unwrap();
        let router = setup_router(&router_chain, vec![FACTORY_CHAIN_ID_EVM]).unwrap();
        let chain_uid = ChainUid::create(FACTORY_CHAIN_ID_EVM.to_string()).unwrap();
        let factory_address = factory_chain.addr_make("pending-factory-keep").to_string();

        router
            .register_factory(
                RegisterFactoryChainType::Evm(RegisterFactoryChainEvm {
                    factory_address: factory_address.clone(),
                    factory_chain_id: FACTORY_CHAIN_ID_EVM.to_string(),
                }),
                chain_uid.clone(),
            )
            .unwrap();
        router
            .register_factory(
                RegisterFactoryChainType::Evm(RegisterFactoryChainEvm {
                    factory_address,
                    factory_chain_id: FACTORY_CHAIN_ID_EVM.to_string(),
                }),
                chain_uid.clone(),
            )
            .unwrap();

        assert!(router_pending_packet_exists(&router, chain_uid.clone(), 0));
        assert!(router_pending_packet_exists(&router, chain_uid.clone(), 1));
        assert_eq!(query_router_pending_count(&router, chain_uid), 2);
    }
}
