#![cfg(not(target_arch = "wasm32"))]

use claimer::ClaimerContract;
use cosmwasm_std::{to_json_binary, Addr, Uint128};
use cw_orch::{mock::MockBase, prelude::*};
use cw_orch_interchain::{core::InterchainEnv, prelude::*};
use factory::FactoryContract;
use router::RouterContract;

use crate::{
    helpers::{
        chains::{get_virtual_balance, setup_claimer, setup_factory, setup_router},
        claimer::{get_claimer_key, sign_claim_messsage},
        factory::{deposit_token, register_token, transfer_token_vcoin},
        relayer::relay_router_factory_router,
    },
    tests::factory::run_test_swap_request_reusable,
};
use euclid::{
    chain::{CrossChainUser, CrossChainUserWithLimit, Limit},
    msgs::{
        claimer::{
            CreateVoucherClaim, ExecuteMsgFns as ClaimerExecuteMsgFns,
            QueryMsgFns as ClaimerQueryMsgFns,
        },
        factory::QueryMsgFns as FactoryQueryMsgFns,
        router::QueryMsgFns as RouterQueryMsgFns,
        virtual_balance::QueryMsgFns as VirtualBalanceQueryMsgFns,
    },
    token::{Token, TokenType, TokenWithDenom},
    virtual_balance::BalanceKey,
};

fn setup_claimer_and_factory() -> (
    RouterContract<MockBase>,
    ClaimerContract<MockBase>,
    FactoryContract<MockBase>,
    FactoryContract<MockBase>,
) {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = setup_router(&router_chain).unwrap();
    let osmosis_factory = setup_factory(&interchain, "osmosis", "nibiru", &router).unwrap();
    let nibiru_factory = setup_factory(&interchain, "nibiru", "nibiru", &router).unwrap();
    let vcoin_address = get_virtual_balance(
        &router_chain,
        &router.get_state().unwrap().virtual_balance_address.unwrap(),
    );
    let claimer = setup_claimer(&nibiru_factory, &vcoin_address).unwrap();
    (router, claimer, osmosis_factory, nibiru_factory)
}

#[test]
fn test_proper_instantiation() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let _factory_chain = interchain.get_chain("osmosis").unwrap();
    let router_chain = interchain.get_chain("nibiru").unwrap();

    let router = setup_router(&router_chain).unwrap();
    let vcoin_address = get_virtual_balance(
        &router_chain,
        &router.get_state().unwrap().virtual_balance_address.unwrap(),
    );
    let osmosis_factory = setup_factory(&interchain, "osmosis", "nibiru", &router).unwrap();
    let nibiru_factory = setup_factory(&interchain, "nibiru", "nibiru", &router).unwrap();

    assert!(
        setup_claimer(&osmosis_factory, &vcoin_address).is_err(),
        "Claimer should not be able to be instantiated on a different chain"
    );
    let claimer_contract = setup_claimer(&nibiru_factory, &vcoin_address).unwrap();

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
    let (router, claimer, osmosis_factory, nibiru_factory) = setup_claimer_and_factory();

    let token = TokenWithDenom {
        token: Token::create("eucl".to_string()).unwrap(),
        token_type: TokenType::Native {
            denom: "eucl".to_string(),
        },
    };
    register_token(&osmosis_factory, &router, token.clone()).unwrap();
    let amount_to_distribute = Uint128::from(10_000u128);
    let (_, pubkey_binary) = get_claimer_key();
    let claim_obj = euclid::msgs::claimer::VirtualBalanceReceiveHookMsg::CreateVoucherClaim(
        CreateVoucherClaim {
            claimer_pubkey: pubkey_binary.clone(),
        },
    );

    // Deposit with claim msg
    deposit_token(
        &osmosis_factory,
        &router,
        token.clone(),
        amount_to_distribute,
        Some(CrossChainUser::new(
            nibiru_factory.get_state().unwrap().chain_uid,
            claimer.address().unwrap().to_string(),
        )),
        // None,
        Some(to_json_binary(&claim_obj).unwrap()),
    )
    .unwrap();

    let claims = claimer
        .get_user_claims(1, 0, pubkey_binary.clone())
        .unwrap();
    assert_eq!(claims.len(), 1);
    let claim = claims.first().unwrap().1.clone();
    assert_eq!(claim.amount.u128(), amount_to_distribute.u128());
    assert_eq!(claim.token, token.token);
    assert_eq!(claim.claimer_pubkey, pubkey_binary);
    assert_eq!(
        claim.sender,
        CrossChainUser::new(
            osmosis_factory.get_state().unwrap().chain_uid,
            osmosis_factory.environment().sender.to_string(),
        )
    );
}

#[test]
fn test_create_claim_using_vcoin_transfer() {
    let (router, claimer, osmosis_factory, nibiru_factory) = setup_claimer_and_factory();

    let token = TokenWithDenom {
        token: Token::create("eucl".to_string()).unwrap(),
        token_type: TokenType::Native {
            denom: "eucl".to_string(),
        },
    };
    register_token(&osmosis_factory, &router, token.clone()).unwrap();
    let amount_to_distribute = Uint128::from(10_000u128);
    let (_, pubkey_binary) = get_claimer_key();
    let claim_obj = euclid::msgs::claimer::VirtualBalanceReceiveHookMsg::CreateVoucherClaim(
        CreateVoucherClaim {
            claimer_pubkey: pubkey_binary.clone(),
        },
    );

    // Deposit vouchers
    deposit_token(
        &osmosis_factory,
        &router,
        token.clone(),
        amount_to_distribute,
        None,
        None,
    )
    .unwrap();

    // Transfer vouchers
    transfer_token_vcoin(
        &osmosis_factory,
        &router,
        token.token.clone(),
        amount_to_distribute,
        CrossChainUser::new(
            nibiru_factory.get_state().unwrap().chain_uid,
            claimer.address().unwrap().to_string(),
        ),
        // None,
        Some(to_json_binary(&claim_obj).unwrap()),
    )
    .unwrap();

    let claims = claimer
        .get_user_claims(1, 0, pubkey_binary.clone())
        .unwrap();
    assert_eq!(claims.len(), 1);
    let claim = claims[0].1.clone();
    assert_eq!(claim.amount.u128(), amount_to_distribute.u128());
    assert_eq!(claim.token, token.token);
    assert_eq!(claim.claimer_pubkey, pubkey_binary);
    assert_eq!(
        claim.sender,
        CrossChainUser::new(
            osmosis_factory.get_state().unwrap().chain_uid,
            osmosis_factory.environment().sender.to_string(),
        )
    );
}

#[test]
fn test_create_claim_using_swap() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let (router, claimer, osmosis_factory, nibiru_factory) = setup_claimer_and_factory();
    let (_, pubkey_binary) = get_claimer_key();
    let claim_obj = euclid::msgs::claimer::VirtualBalanceReceiveHookMsg::CreateVoucherClaim(
        CreateVoucherClaim {
            claimer_pubkey: pubkey_binary.clone(),
        },
    );
    let amount_to_distribute = Uint128::from(100u128);

    let cross_chain_address = vec![CrossChainUserWithLimit {
        user: CrossChainUser::new(
            nibiru_factory.get_state().unwrap().chain_uid,
            claimer.address().unwrap().to_string(),
        ),
        limit: Some(Limit::Equal(amount_to_distribute)),
        preferred_denom: None,
        refund_address: None,
        forwarding_message: None,
        vcoin_msg: Some(to_json_binary(&claim_obj).unwrap()),
        unsafe_refund_voucher_to_recipient: None,
    }];
    let swap_test_output = run_test_swap_request_reusable(
        sender.as_str(),
        &osmosis_factory,
        &router,
        Some(cross_chain_address),
    )
    .unwrap();

    let claims = claimer
        .get_user_claims(1, 0, pubkey_binary.clone())
        .unwrap();
    assert_eq!(claims.len(), 1);
    let claim = claims[0].1.clone();
    assert_eq!(claim.amount.u128(), amount_to_distribute.u128());
    assert_eq!(claim.token, swap_test_output.token_out.token);
    assert_eq!(claim.claimer_pubkey, pubkey_binary);
    assert_eq!(
        claim.sender,
        CrossChainUser::new(
            osmosis_factory.get_state().unwrap().chain_uid,
            osmosis_factory.environment().sender.to_string(),
        )
    );
}

#[test]
fn test_claim_voucher_as_voucher() {
    let (router, claimer, osmosis_factory, nibiru_factory) = setup_claimer_and_factory();
    let token = TokenWithDenom {
        token: Token::create("eucl".to_string()).unwrap(),
        token_type: TokenType::Native {
            denom: "eucl".to_string(),
        },
    };
    register_token(&osmosis_factory, &router, token.clone()).unwrap();
    let amount_to_distribute = Uint128::from(10_000u128);
    let (signer_key, pubkey_binary) = get_claimer_key();
    let claim_obj = euclid::msgs::claimer::VirtualBalanceReceiveHookMsg::CreateVoucherClaim(
        CreateVoucherClaim {
            claimer_pubkey: pubkey_binary.clone(),
        },
    );

    // Deposit with claim msg
    deposit_token(
        &osmosis_factory,
        &router,
        token.clone(),
        amount_to_distribute,
        Some(CrossChainUser::new(
            nibiru_factory.get_state().unwrap().chain_uid,
            claimer.address().unwrap().to_string(),
        )),
        // None,
        Some(to_json_binary(&claim_obj).unwrap()),
    )
    .unwrap();

    let claims = claimer
        .get_user_claims(1, 0, pubkey_binary.clone())
        .unwrap();
    assert_eq!(claims.len(), 1);
    let (claim_id, claim) = claims.first().unwrap();
    assert_eq!(claim.amount.u128(), amount_to_distribute.u128());
    assert_eq!(claim.token, token.token);
    assert_eq!(claim.claimer_pubkey, pubkey_binary);
    assert_eq!(
        claim.sender,
        CrossChainUser::new(
            osmosis_factory.get_state().unwrap().chain_uid,
            osmosis_factory.environment().sender.to_string(),
        )
    );

    let new_recipient = CrossChainUser::new(
        osmosis_factory.get_state().unwrap().chain_uid,
        osmosis_factory
            .environment()
            .addr_make("new_recipient")
            .to_string(),
    );

    let claim_msg = euclid::msgs::claimer::ClaimVoucherData {
        claim_id: *claim_id,
        recipient: new_recipient.clone(),
        release_funds: false,
        release_msg: None,
    };

    let signed_data =
        sign_claim_messsage(signer_key, claim_msg, &claimer.environment().app.borrow());

    let response = claimer.claim_voucher(signed_data).unwrap();
    relay_router_factory_router(
        response.events,
        &osmosis_factory,
        &osmosis_factory.get_state().unwrap().chain_uid,
        &router,
    )
    .unwrap();

    let claims = claimer
        .get_user_claims(1, 0, pubkey_binary.clone())
        .unwrap();
    assert_eq!(claims.len(), 0);

    let vcoin_contract = get_virtual_balance(
        router.environment(),
        &router.get_state().unwrap().virtual_balance_address.unwrap(),
    );

    let balance_key = BalanceKey {
        cross_chain_user: new_recipient.clone(),
        token_id: token.token.to_string(),
    };
    let vcoin_balance = vcoin_contract.get_balance(balance_key).unwrap();
    assert_eq!(vcoin_balance.amount, amount_to_distribute);
}

#[test]
fn test_claim_voucher_and_release() {
    let (router, claimer, osmosis_factory, nibiru_factory) = setup_claimer_and_factory();
    let token = TokenWithDenom {
        token: Token::create("eucl".to_string()).unwrap(),
        token_type: TokenType::Native {
            denom: "eucl".to_string(),
        },
    };
    register_token(&osmosis_factory, &router, token.clone()).unwrap();
    let amount_to_distribute = Uint128::from(10_000u128);
    let (signer_key, pubkey_binary) = get_claimer_key();
    let claim_obj = euclid::msgs::claimer::VirtualBalanceReceiveHookMsg::CreateVoucherClaim(
        CreateVoucherClaim {
            claimer_pubkey: pubkey_binary.clone(),
        },
    );

    // Deposit with claim msg
    deposit_token(
        &osmosis_factory,
        &router,
        token.clone(),
        amount_to_distribute,
        Some(CrossChainUser::new(
            nibiru_factory.get_state().unwrap().chain_uid,
            claimer.address().unwrap().to_string(),
        )),
        // None,
        Some(to_json_binary(&claim_obj).unwrap()),
    )
    .unwrap();

    let claims = claimer
        .get_user_claims(1, 0, pubkey_binary.clone())
        .unwrap();
    assert_eq!(claims.len(), 1);
    let (claim_id, claim) = claims.first().unwrap();
    assert_eq!(claim.amount.u128(), amount_to_distribute.u128());
    assert_eq!(claim.token, token.token);
    assert_eq!(claim.claimer_pubkey, pubkey_binary);
    assert_eq!(
        claim.sender,
        CrossChainUser::new(
            osmosis_factory.get_state().unwrap().chain_uid,
            osmosis_factory.environment().sender.to_string(),
        )
    );

    let new_recipient = CrossChainUser::new(
        osmosis_factory.get_state().unwrap().chain_uid,
        osmosis_factory
            .environment()
            .addr_make("new_recipient")
            .to_string(),
    );

    let claim_msg = euclid::msgs::claimer::ClaimVoucherData {
        claim_id: *claim_id,
        recipient: new_recipient.clone(),
        release_funds: true,
        release_msg: None,
    };

    let signed_data =
        sign_claim_messsage(signer_key, claim_msg, &claimer.environment().app.borrow());

    let response = claimer.claim_voucher(signed_data).unwrap();
    relay_router_factory_router(
        response.events,
        &osmosis_factory,
        &osmosis_factory.get_state().unwrap().chain_uid,
        &router,
    )
    .unwrap();

    let claims = claimer
        .get_user_claims(1, 0, pubkey_binary.clone())
        .unwrap();
    assert_eq!(claims.len(), 0);

    let vcoin_contract = get_virtual_balance(
        router.environment(),
        &router.get_state().unwrap().virtual_balance_address.unwrap(),
    );

    let balance_key = BalanceKey {
        cross_chain_user: new_recipient.clone(),
        token_id: token.token.to_string(),
    };
    let vcoin_balance = vcoin_contract.get_balance(balance_key).unwrap();
    assert_eq!(vcoin_balance.amount, Uint128::zero());

    let new_user_native_balance = osmosis_factory
        .environment()
        .balance(
            &Addr::unchecked(new_recipient.address.clone()),
            Some(token.token_type.get_denom().unwrap()),
        )
        .unwrap();
    assert_eq!(new_user_native_balance.len(), 1);
    assert_eq!(new_user_native_balance[0].amount, amount_to_distribute);
}

#[test]
fn test_unauthorized_claim_voucher() {
    let (router, claimer, osmosis_factory, nibiru_factory) = setup_claimer_and_factory();
    let token = TokenWithDenom {
        token: Token::create("eucl".to_string()).unwrap(),
        token_type: TokenType::Native {
            denom: "eucl".to_string(),
        },
    };
    register_token(&osmosis_factory, &router, token.clone()).unwrap();
    let amount_to_distribute = Uint128::from(10_000u128);
    let (_, pubkey_binary) = get_claimer_key();
    let claim_obj = euclid::msgs::claimer::VirtualBalanceReceiveHookMsg::CreateVoucherClaim(
        CreateVoucherClaim {
            claimer_pubkey: pubkey_binary.clone(),
        },
    );

    // Deposit with claim msg
    deposit_token(
        &osmosis_factory,
        &router,
        token.clone(),
        amount_to_distribute,
        Some(CrossChainUser::new(
            nibiru_factory.get_state().unwrap().chain_uid,
            claimer.address().unwrap().to_string(),
        )),
        // None,
        Some(to_json_binary(&claim_obj).unwrap()),
    )
    .unwrap();

    let claims = claimer
        .get_user_claims(1, 0, pubkey_binary.clone())
        .unwrap();
    assert_eq!(claims.len(), 1);
    let (claim_id, claim) = claims.first().unwrap();
    assert_eq!(claim.amount.u128(), amount_to_distribute.u128());
    assert_eq!(claim.token, token.token);
    assert_eq!(claim.claimer_pubkey, pubkey_binary);
    assert_eq!(
        claim.sender,
        CrossChainUser::new(
            osmosis_factory.get_state().unwrap().chain_uid,
            osmosis_factory.environment().sender.to_string(),
        )
    );

    let new_recipient = CrossChainUser::new(
        osmosis_factory.get_state().unwrap().chain_uid,
        osmosis_factory
            .environment()
            .addr_make("new_recipient")
            .to_string(),
    );

    let claim_msg = euclid::msgs::claimer::ClaimVoucherData {
        claim_id: *claim_id,
        recipient: new_recipient.clone(),
        release_funds: false,
        release_msg: None,
    };

    let (signer_key, _) = get_claimer_key();

    let signed_data =
        sign_claim_messsage(signer_key, claim_msg, &claimer.environment().app.borrow());

    let response = claimer.claim_voucher(signed_data);
    assert!(
        response.is_err(),
        "Claimer should not be able to claim voucher for another user"
    );
}