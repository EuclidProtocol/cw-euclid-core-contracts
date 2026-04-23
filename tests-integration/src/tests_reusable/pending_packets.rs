#![cfg(not(target_arch = "wasm32"))]

#[cfg(test)]
mod tests {
    use cosmwasm_std::{from_json, Addr};
    use euclid::chain::ChainUid;
    use euclid::msgs::cross_chain_config::CrossChainConfig;
    use euclid::msgs::router::{RegisterFactoryChainEvm, RegisterFactoryChainType};
    use euclid::token::{Token, TokenType, TokenWithDenom};
    use euclid_ibc::factory_ibc::FactoryCrossChainExecuteMsg;

    use crate::helpers::app::EuclidApp;
    use crate::helpers::chains::{setup_factory, setup_interchain, setup_router};
    use crate::helpers::relayer::{
        ack_register_factory_evm, extract_send_packet_events, relay_factory_ack_packet,
        relay_factory_send_packet,
    };
    use crate::tests_reusable::constants::{
        FACTORY_CHAIN_ID_EVM, FACTORY_CHAIN_ID_IBC, ROUTER_CHAIN_ID,
    };

    fn query_factory_user_pending_count(factory_app: &EuclidApp, factory_addr: &Addr, sender: Addr) -> u128 {
        factory::rate_limit::USER_PENDING_PACKETS_COUNT
            .query(
                &factory_app.app().wrap(),
                factory_addr.clone(),
                sender,
            )
            .unwrap()
            .unwrap_or(0)
    }

    fn query_factory_pending_count(factory_app: &EuclidApp, factory_addr: &Addr) -> u128 {
        let raw = factory_app
            .app()
            .wrap()
            .query_wasm_raw(
                factory_addr.clone(),
                factory::relay_state::CROSS_CHAIN_PENDING_PACKETS_COUNT.as_slice(),
            )
            .unwrap();

        raw.map(|value| from_json::<u128>(value).unwrap())
            .unwrap_or(0)
    }

    fn query_router_pending_count(router_app: &EuclidApp, router_addr: &Addr, chain_uid: ChainUid) -> u128 {
        router::relay_state::CROSS_CHAIN_PENDING_PACKETS_COUNT
            .query(
                &router_app.app().wrap(),
                router_addr.clone(),
                chain_uid,
            )
            .unwrap()
            .unwrap_or(0)
    }

    fn factory_pending_packet_exists(factory_app: &EuclidApp, factory_addr: &Addr, sequence: u128) -> bool {
        factory::relay_state::CROSS_CHAIN_PENDING_SEND_PACKETS
            .query(
                &factory_app.app().wrap(),
                factory_addr.clone(),
                sequence,
            )
            .unwrap()
            .is_some()
    }

    fn router_pending_packet_exists(
        router_app: &EuclidApp,
        router_addr: &Addr,
        chain_uid: ChainUid,
        sequence: u128,
    ) -> bool {
        router::relay_state::CROSS_CHAIN_PENDING_SEND_PACKETS
            .query(
                &router_app.app().wrap(),
                router_addr.clone(),
                (chain_uid, sequence),
            )
            .unwrap()
            .is_some()
    }

    #[test]
    fn factory_pending_packet_counts_increment_and_decrement() {
        let sender = "sender_for_all_chains";
        let mut env = setup_interchain(sender, FACTORY_CHAIN_ID_IBC);
        let router_addr =
            setup_router(env.chain_mut(ROUTER_CHAIN_ID), vec![FACTORY_CHAIN_ID_IBC]).unwrap();
        let factory_addr =
            setup_factory(&mut env, FACTORY_CHAIN_ID_IBC, ROUTER_CHAIN_ID, &router_addr).unwrap();

        let sender_addr = env.chain(FACTORY_CHAIN_ID_IBC).sender();
        let factory_chain_uid = {
            let state: euclid::msgs::factory::StateResponse = env
                .chain(FACTORY_CHAIN_ID_IBC)
                .query(&factory_addr, &euclid::msgs::factory::QueryMsg::GetState {});
            state.chain_uid
        };
        let token = TokenWithDenom {
            token: Token::create("pendingcheck".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "pendingcheck".to_string(),
            },
        };

        let factory_app_sender = env.chain(FACTORY_CHAIN_ID_IBC).sender();
        let tx = env.chain_mut(FACTORY_CHAIN_ID_IBC).execute(
            &factory_app_sender,
            &factory_addr,
            &euclid::msgs::factory::ExecuteMsg::RegisterDenom {
                token_with_denom: token,
                cross_chain_config: CrossChainConfig::default(),
            },
            &[],
        );

        assert_eq!(
            query_factory_user_pending_count(
                env.chain(FACTORY_CHAIN_ID_IBC),
                &factory_addr,
                sender_addr.clone()
            ),
            1
        );
        assert_eq!(
            query_factory_pending_count(env.chain(FACTORY_CHAIN_ID_IBC), &factory_addr),
            1
        );
        assert!(factory_pending_packet_exists(
            env.chain(FACTORY_CHAIN_ID_IBC),
            &factory_addr,
            0
        ));

        let ack_events =
            relay_factory_send_packet(tx.events, &router_addr, env.chain_mut(ROUTER_CHAIN_ID))
                .unwrap();
        assert_eq!(
            query_factory_user_pending_count(
                env.chain(FACTORY_CHAIN_ID_IBC),
                &factory_addr,
                sender_addr.clone()
            ),
            1
        );
        assert_eq!(
            query_factory_pending_count(env.chain(FACTORY_CHAIN_ID_IBC), &factory_addr),
            1
        );

        relay_factory_ack_packet(
            &factory_addr,
            ack_events,
            &factory_chain_uid,
            env.chain_mut(FACTORY_CHAIN_ID_IBC),
        )
        .unwrap();
        assert_eq!(
            query_factory_user_pending_count(
                env.chain(FACTORY_CHAIN_ID_IBC),
                &factory_addr,
                sender_addr
            ),
            0
        );
        assert_eq!(
            query_factory_pending_count(env.chain(FACTORY_CHAIN_ID_IBC), &factory_addr),
            0
        );
    }

    #[test]
    fn factory_pending_send_packets_keep_existing_sequences() {
        let sender = "sender_for_all_chains";
        let mut env = setup_interchain(sender, FACTORY_CHAIN_ID_IBC);
        let router_addr =
            setup_router(env.chain_mut(ROUTER_CHAIN_ID), vec![FACTORY_CHAIN_ID_IBC]).unwrap();
        let factory_addr =
            setup_factory(&mut env, FACTORY_CHAIN_ID_IBC, ROUTER_CHAIN_ID, &router_addr).unwrap();
        let sender_addr = env.chain(FACTORY_CHAIN_ID_IBC).sender();

        let token_one = TokenWithDenom {
            token: Token::create("pendingone".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "pendingone".to_string(),
            },
        };
        let token_two = TokenWithDenom {
            token: Token::create("pendingtwo".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "pendingtwo".to_string(),
            },
        };

        env.chain_mut(FACTORY_CHAIN_ID_IBC).execute(
            &sender_addr,
            &factory_addr,
            &euclid::msgs::factory::ExecuteMsg::RegisterDenom {
                token_with_denom: token_one,
                cross_chain_config: CrossChainConfig::default(),
            },
            &[],
        );
        env.chain_mut(FACTORY_CHAIN_ID_IBC).execute(
            &sender_addr,
            &factory_addr,
            &euclid::msgs::factory::ExecuteMsg::RegisterDenom {
                token_with_denom: token_two,
                cross_chain_config: CrossChainConfig::default(),
            },
            &[],
        );

        assert!(factory_pending_packet_exists(
            env.chain(FACTORY_CHAIN_ID_IBC),
            &factory_addr,
            0
        ));
        assert!(factory_pending_packet_exists(
            env.chain(FACTORY_CHAIN_ID_IBC),
            &factory_addr,
            1
        ));
        assert_eq!(
            query_factory_user_pending_count(
                env.chain(FACTORY_CHAIN_ID_IBC),
                &factory_addr,
                sender_addr
            ),
            2
        );
        assert_eq!(
            query_factory_pending_count(env.chain(FACTORY_CHAIN_ID_IBC), &factory_addr),
            2
        );
    }

    #[test]
    fn factory_rate_limit_does_not_accumulate_after_successful_acks() {
        let sender = "sender_for_all_chains";
        let mut env = setup_interchain(sender, FACTORY_CHAIN_ID_IBC);
        let router_addr =
            setup_router(env.chain_mut(ROUTER_CHAIN_ID), vec![FACTORY_CHAIN_ID_IBC]).unwrap();
        let factory_addr =
            setup_factory(&mut env, FACTORY_CHAIN_ID_IBC, ROUTER_CHAIN_ID, &router_addr).unwrap();

        let sender_addr = env.chain(FACTORY_CHAIN_ID_IBC).sender();
        for i in 0..12u8 {
            let token_id = format!("ratelimit{i}");
            let token = TokenWithDenom {
                token: Token::create(token_id.clone()).unwrap(),
                token_type: TokenType::Native { denom: token_id },
            };
            let tx = env.chain_mut(FACTORY_CHAIN_ID_IBC).execute(
                &sender_addr,
                &factory_addr,
                &euclid::msgs::factory::ExecuteMsg::RegisterDenom {
                    token_with_denom: token,
                    cross_chain_config: CrossChainConfig::default(),
                },
                &[],
            );
            let chain_uid = {
                let state: euclid::msgs::factory::StateResponse = env
                    .chain(FACTORY_CHAIN_ID_IBC)
                    .query(&factory_addr, &euclid::msgs::factory::QueryMsg::GetState {});
                state.chain_uid
            };
            let ack_events =
                relay_factory_send_packet(tx.events, &router_addr, env.chain_mut(ROUTER_CHAIN_ID))
                    .unwrap();
            relay_factory_ack_packet(
                &factory_addr,
                ack_events,
                &chain_uid,
                env.chain_mut(FACTORY_CHAIN_ID_IBC),
            )
            .unwrap();
        }

        assert_eq!(
            query_factory_user_pending_count(
                env.chain(FACTORY_CHAIN_ID_IBC),
                &factory_addr,
                sender_addr
            ),
            0
        );
        assert_eq!(
            query_factory_pending_count(env.chain(FACTORY_CHAIN_ID_IBC), &factory_addr),
            0
        );
    }

    #[test]
    fn router_pending_packet_counts_increment_and_decrement_for_evm_registration() {
        let sender = "sender_for_all_chains";
        let mut env = setup_interchain(sender, FACTORY_CHAIN_ID_EVM);
        let router_addr =
            setup_router(env.chain_mut(ROUTER_CHAIN_ID), vec![FACTORY_CHAIN_ID_EVM]).unwrap();

        let chain_uid = ChainUid::create(FACTORY_CHAIN_ID_EVM.to_string()).unwrap();
        let factory_address = env
            .chain(FACTORY_CHAIN_ID_EVM)
            .addr_make("pending-factory")
            .to_string();

        let router_sender = env.chain(ROUTER_CHAIN_ID).sender();
        let register = env.chain_mut(ROUTER_CHAIN_ID).execute(
            &router_sender,
            &router_addr,
            &euclid::msgs::router::ExecuteMsg::RegisterFactory {
                chain_info: RegisterFactoryChainType::Evm(RegisterFactoryChainEvm {
                    factory_address: factory_address.clone(),
                    factory_chain_id: FACTORY_CHAIN_ID_EVM.to_string(),
                }),
                chain_uid: chain_uid.clone(),
            },
            &[],
        );

        assert_eq!(
            query_router_pending_count(env.chain(ROUTER_CHAIN_ID), &router_addr, chain_uid.clone()),
            1
        );

        let packet = extract_send_packet_events(&register.events)
            .into_iter()
            .next()
            .unwrap();
        let packet_msg: FactoryCrossChainExecuteMsg = from_json(&packet.msg).unwrap();
        let tx_id = packet_msg.get_tx_id();
        ack_register_factory_evm(
            &router_addr,
            env.chain_mut(ROUTER_CHAIN_ID),
            &chain_uid,
            &factory_address,
            FACTORY_CHAIN_ID_EVM,
            &tx_id,
            packet.sequence,
        )
        .unwrap();

        assert_eq!(
            query_router_pending_count(env.chain(ROUTER_CHAIN_ID), &router_addr, chain_uid),
            0
        );
    }

    #[test]
    fn router_pending_send_packets_keep_existing_sequences() {
        let sender = "sender_for_all_chains";
        let mut env = setup_interchain(sender, FACTORY_CHAIN_ID_EVM);
        let router_addr =
            setup_router(env.chain_mut(ROUTER_CHAIN_ID), vec![FACTORY_CHAIN_ID_EVM]).unwrap();
        let chain_uid = ChainUid::create(FACTORY_CHAIN_ID_EVM.to_string()).unwrap();
        let factory_address = env
            .chain(FACTORY_CHAIN_ID_EVM)
            .addr_make("pending-factory-keep")
            .to_string();

        let router_sender = env.chain(ROUTER_CHAIN_ID).sender();
        env.chain_mut(ROUTER_CHAIN_ID).execute(
            &router_sender,
            &router_addr,
            &euclid::msgs::router::ExecuteMsg::RegisterFactory {
                chain_info: RegisterFactoryChainType::Evm(RegisterFactoryChainEvm {
                    factory_address: factory_address.clone(),
                    factory_chain_id: FACTORY_CHAIN_ID_EVM.to_string(),
                }),
                chain_uid: chain_uid.clone(),
            },
            &[],
        );
        env.chain_mut(ROUTER_CHAIN_ID).execute(
            &router_sender,
            &router_addr,
            &euclid::msgs::router::ExecuteMsg::RegisterFactory {
                chain_info: RegisterFactoryChainType::Evm(RegisterFactoryChainEvm {
                    factory_address,
                    factory_chain_id: FACTORY_CHAIN_ID_EVM.to_string(),
                }),
                chain_uid: chain_uid.clone(),
            },
            &[],
        );

        assert!(router_pending_packet_exists(
            env.chain(ROUTER_CHAIN_ID),
            &router_addr,
            chain_uid.clone(),
            0
        ));
        assert!(router_pending_packet_exists(
            env.chain(ROUTER_CHAIN_ID),
            &router_addr,
            chain_uid.clone(),
            1
        ));
        assert_eq!(
            query_router_pending_count(env.chain(ROUTER_CHAIN_ID), &router_addr, chain_uid),
            2
        );
    }
}
