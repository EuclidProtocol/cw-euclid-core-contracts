#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{Addr, Uint128};
use cw_orch::prelude::*;
use cw_orch_interchain::{prelude::*, InterchainEnv};

use crate::helpers::{
    chains::{setup_claimer, setup_factory, setup_router},
    factory::{deposit_token, register_token},
    relayer::get_signer_key,
};
use euclid::{
    chain::CrossChainUser,
    msgs::{
        claimer::{ExecuteMsgFns as ClaimerExecuteMsgFns, QueryMsgFns as ClaimerQueryMsgFns},
        factory::QueryMsgFns as FactoryQueryMsgFns,
    },
    token::{Token, TokenType, TokenWithDenom},
};

#[test]
fn test_proper_instantiation() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let _factory_chain = interchain.get_chain("osmosis").unwrap();
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = setup_router(&router_chain).unwrap();
    let osmosis_factory = setup_factory(&interchain, "osmosis", "nibiru", &router).unwrap();
    let nibiru_factory = setup_factory(&interchain, "nibiru", "nibiru", &router).unwrap();

    assert!(
        setup_claimer(&osmosis_factory).is_err(),
        "Claimer should not be able to be instantiated on a different chain"
    );
    let claimer_contract = setup_claimer(&nibiru_factory).unwrap();

    let factory_address = nibiru_factory.address().unwrap();

    assert_eq!(
        claimer_contract.get_state().unwrap().factory_address,
        factory_address
    );

    assert_eq!(
        claimer_contract.get_state().unwrap().chain_uid,
        nibiru_factory.get_state().unwrap().chain_uid
    );
}

#[test]
fn test_create_claim() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = setup_router(&router_chain).unwrap();
    let osmosis_factory = setup_factory(&interchain, "osmosis", "nibiru", &router).unwrap();
    let nibiru_factory = setup_factory(&interchain, "nibiru", "nibiru", &router).unwrap();
    let claimer = setup_claimer(&nibiru_factory).unwrap();

    let token = TokenWithDenom {
        token: Token::create("eucl".to_string()).unwrap(),
        token_type: TokenType::Native {
            denom: "eucl".to_string(),
        },
    };
    register_token(&osmosis_factory, &router, token.clone()).unwrap();
    let amount_to_distribute = Uint128::from(10_000u128);
    deposit_token(
        &osmosis_factory,
        &router,
        token.clone(),
        amount_to_distribute,
        Some(CrossChainUser::new(
            nibiru_factory.get_state().unwrap().chain_uid,
            sender,
        )),
    )
    .unwrap();

    let (_, pubkey_binary) = get_signer_key();
    let claim_obj = euclid::msgs::claimer::CreateVoucherClaim {
        token: token.token.clone(),
        amount: amount_to_distribute,
        claimer_pubkey: pubkey_binary.clone(),
    };
    claimer.create_voucher_claim(claim_obj).unwrap();

    let claim_ids = claimer.get_user_claims(pubkey_binary.clone()).unwrap();
    assert_eq!(claim_ids.len(), 1);
    let claim_id = claim_ids[0];
    let claim = claimer.get_claim(claim_id).unwrap();
    assert_eq!(claim.amount.u128(), amount_to_distribute.u128());
    assert_eq!(claim.token, token.token);
    assert_eq!(claim.claimer_pubkey, pubkey_binary);
}
