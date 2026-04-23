#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{to_json_binary, Addr, Uint128};
use euclid::{
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    limit::Limit,
    msgs::claimer::{
        msg::{ClaimVoucherData, ExecuteMsg as ClaimerExecuteMsg, QueryMsg as ClaimerQueryMsg},
        voucher_receive::CreateVoucherClaim,
    },
    msgs::virtual_balance::{GetBalanceResponse, QueryMsg as VirtualBalanceQueryMsg},
    recipient::Recipient,
    token::{Token, TokenType, TokenWithDenom},
    voucher::BalanceKey,
};

use crate::{
    helpers::{
        chains::{get_virtual_balance_addr, setup_claimer, setup_factory, setup_interchain, setup_router},
        claimer::{get_claimer_key, sign_claim_messsage},
        factory::{deposit_token, register_token, transfer_token_vcoin},
        relayer::relay_router_factory_router,
        multi_chain::MultiChainEnv,
    },
    tests_reusable::constants::{FACTORY_CHAIN_ID_IBC, ROUTER_CHAIN_ID},
};

const FACTORY_CHAIN_ID: &str = FACTORY_CHAIN_ID_IBC;

fn setup_env() -> (MultiChainEnv, Addr, Addr, Addr) {
    let sender = "sender_for_all_chains";
    let mut env = setup_interchain(sender, FACTORY_CHAIN_ID);
    let router_addr = setup_router(env.chain_mut(ROUTER_CHAIN_ID), vec![FACTORY_CHAIN_ID]).unwrap();
    let factory_addr =
        setup_factory(&mut env, FACTORY_CHAIN_ID, ROUTER_CHAIN_ID, &router_addr).unwrap();
    let vcoin_addr = get_virtual_balance_addr(env.chain(ROUTER_CHAIN_ID), &router_addr);
    let claimer_addr =
        setup_claimer(env.chain_mut(ROUTER_CHAIN_ID), &router_addr, &vcoin_addr).unwrap();
    (env, router_addr, factory_addr, claimer_addr)
}

fn factory_chain_uid(env: &MultiChainEnv, factory_addr: &Addr) -> ChainUid {
    let state: euclid::msgs::factory::StateResponse =
        env.chain(FACTORY_CHAIN_ID).query(factory_addr, &euclid::msgs::factory::QueryMsg::GetState {});
    state.chain_uid
}

fn get_vb_balance(env: &MultiChainEnv, router_addr: &Addr, user: &CrossChainUser, token: &Token) -> Uint128 {
    let vb_addr = get_virtual_balance_addr(env.chain(ROUTER_CHAIN_ID), router_addr);
    let resp: GetBalanceResponse = env.chain(ROUTER_CHAIN_ID).query(
        &vb_addr,
        &VirtualBalanceQueryMsg::GetBalance {
            balance_key: BalanceKey {
                cross_chain_user: user.clone(),
                token_id: token.to_string(),
            },
        },
    );
    resp.amount
}

fn get_user_claims(
    env: &MultiChainEnv,
    claimer_addr: &Addr,
    pub_key: cosmwasm_std::Binary,
    limit: u64,
    offset: u64,
) -> Vec<(u128, euclid::msgs::claimer::msg::Claim)> {
    env.chain(ROUTER_CHAIN_ID).query(
        claimer_addr,
        &ClaimerQueryMsg::GetUserClaims { pub_key, limit, offset },
    )
}

#[test]
fn test_proper_instantiation() {
    let (env, router_addr, _factory_addr, claimer_addr) = setup_env();
    let vcoin_addr = get_virtual_balance_addr(env.chain(ROUTER_CHAIN_ID), &router_addr);

    let state: euclid::msgs::claimer::msg::State =
        env.chain(ROUTER_CHAIN_ID).query(&claimer_addr, &ClaimerQueryMsg::GetState {});
    assert_eq!(state.router_contract, router_addr);
    assert_eq!(state.vcoin_address, vcoin_addr);
}

#[test]
fn test_create_claim() {
    let (mut env, router_addr, factory_addr, claimer_addr) = setup_env();
    let factory_chain_uid = factory_chain_uid(&env, &factory_addr);

    let claimer_cross_chain_user = CrossChainUser::new(
        ChainUid::vsl_chain_uid().unwrap(),
        claimer_addr.to_string(),
    );

    let token = TokenWithDenom {
        token: Token::create("eucl".to_string()).unwrap(),
        token_type: TokenType::Native { denom: "eucl".to_string() },
    };
    register_token(&factory_addr, FACTORY_CHAIN_ID, &router_addr, ROUTER_CHAIN_ID, &mut env, token.clone()).unwrap();
    let amount_to_distribute = Uint128::from(10_000u128);
    let (_, pubkey_binary) = get_claimer_key();
    let claim_obj = euclid::msgs::claimer::voucher_receive::VoucherReceiveHookMsg::CreateVoucherClaim(
        CreateVoucherClaim {
            claimer_pubkey: pubkey_binary.clone(),
            pseudo_claim_id: Some("pseudo_claim_id".to_string()),
            claim_group_id: Some("group_id".to_string()),
        },
    );

    deposit_token(
        &factory_addr,
        FACTORY_CHAIN_ID,
        &router_addr,
        ROUTER_CHAIN_ID,
        &mut env,
        token.clone(),
        amount_to_distribute,
        vec![Recipient {
            recipient: claimer_cross_chain_user.clone(),
            amount: Limit::Dynamic(Uint128::zero()),
            denom: TokenType::Voucher {},
            forwarding_message: Some(to_json_binary(&claim_obj).unwrap().to_base64()),
            unsafe_refund_as_voucher: None,
        }],
    )
    .unwrap();

    let claims = get_user_claims(&env, &claimer_addr, pubkey_binary.clone(), 1, 0);
    assert_eq!(claims.len(), 1);
    let claim = claims.first().unwrap().1.clone();
    assert_eq!(claim.amount.u128(), amount_to_distribute.u128());
    assert_eq!(claim.token, token.token);
    assert_eq!(claim.claimer_pubkey, pubkey_binary);
    assert_eq!(
        claim.sender,
        CrossChainUser::new(
            factory_chain_uid,
            env.chain(FACTORY_CHAIN_ID).sender().to_string(),
        )
    );
}

#[test]
fn test_create_claim_using_vcoin_transfer() {
    let (mut env, router_addr, factory_addr, claimer_addr) = setup_env();
    let factory_chain_uid = factory_chain_uid(&env, &factory_addr);

    let claimer_cross_chain_user = CrossChainUser::new(
        ChainUid::vsl_chain_uid().unwrap(),
        claimer_addr.to_string(),
    );

    let token = TokenWithDenom {
        token: Token::create("eucl".to_string()).unwrap(),
        token_type: TokenType::Native { denom: "eucl".to_string() },
    };
    register_token(&factory_addr, FACTORY_CHAIN_ID, &router_addr, ROUTER_CHAIN_ID, &mut env, token.clone()).unwrap();
    let amount_to_distribute = Uint128::from(10_000u128);
    let (_, pubkey_binary) = get_claimer_key();
    let claim_obj = euclid::msgs::claimer::voucher_receive::VoucherReceiveHookMsg::CreateVoucherClaim(
        CreateVoucherClaim {
            claimer_pubkey: pubkey_binary.clone(),
            pseudo_claim_id: Some("pseudo_claim_id".to_string()),
            claim_group_id: Some("group_id".to_string()),
        },
    );

    deposit_token(
        &factory_addr,
        FACTORY_CHAIN_ID,
        &router_addr,
        ROUTER_CHAIN_ID,
        &mut env,
        token.clone(),
        amount_to_distribute,
        vec![],
    )
    .unwrap();

    transfer_token_vcoin(
        &factory_addr,
        FACTORY_CHAIN_ID,
        &router_addr,
        ROUTER_CHAIN_ID,
        &mut env,
        token.token.clone(),
        amount_to_distribute,
        vec![Recipient {
            recipient: claimer_cross_chain_user.clone(),
            amount: Limit::Dynamic(Uint128::zero()),
            denom: TokenType::Voucher {},
            forwarding_message: Some(to_json_binary(&claim_obj).unwrap().to_base64()),
            unsafe_refund_as_voucher: None,
        }],
    )
    .unwrap();

    let claims = get_user_claims(&env, &claimer_addr, pubkey_binary.clone(), 1, 0);
    assert_eq!(claims.len(), 1);
    let claim = claims[0].1.clone();
    assert_eq!(claim.amount.u128(), amount_to_distribute.u128());
    assert_eq!(claim.token, token.token);
    assert_eq!(claim.claimer_pubkey, pubkey_binary);
    assert_eq!(
        claim.sender,
        CrossChainUser::new(
            factory_chain_uid,
            env.chain(FACTORY_CHAIN_ID).sender().to_string(),
        )
    );
}

#[test]
fn test_claim_voucher_as_voucher() {
    let (mut env, router_addr, factory_addr, claimer_addr) = setup_env();
    let factory_chain_uid = factory_chain_uid(&env, &factory_addr);

    let claimer_cross_chain_user = CrossChainUser::new(
        ChainUid::vsl_chain_uid().unwrap(),
        claimer_addr.to_string(),
    );
    let token = TokenWithDenom {
        token: Token::create("eucl".to_string()).unwrap(),
        token_type: TokenType::Native { denom: "eucl".to_string() },
    };
    register_token(&factory_addr, FACTORY_CHAIN_ID, &router_addr, ROUTER_CHAIN_ID, &mut env, token.clone()).unwrap();
    let amount_to_distribute = Uint128::from(10_000u128);
    let (signer_key, pubkey_binary) = get_claimer_key();
    let claim_obj = euclid::msgs::claimer::voucher_receive::VoucherReceiveHookMsg::CreateVoucherClaim(
        CreateVoucherClaim {
            claimer_pubkey: pubkey_binary.clone(),
            pseudo_claim_id: Some("pseudo_claim_id".to_string()),
            claim_group_id: Some("group_id".to_string()),
        },
    );

    deposit_token(
        &factory_addr,
        FACTORY_CHAIN_ID,
        &router_addr,
        ROUTER_CHAIN_ID,
        &mut env,
        token.clone(),
        amount_to_distribute,
        vec![Recipient {
            recipient: claimer_cross_chain_user.clone(),
            amount: Limit::Dynamic(Uint128::zero()),
            denom: TokenType::Voucher {},
            forwarding_message: Some(to_json_binary(&claim_obj).unwrap().to_base64()),
            unsafe_refund_as_voucher: None,
        }],
    )
    .unwrap();

    let claims = get_user_claims(&env, &claimer_addr, pubkey_binary.clone(), 1, 0);
    assert_eq!(claims.len(), 1);
    let (claim_id, claim) = claims.first().unwrap();
    assert_eq!(claim.amount.u128(), amount_to_distribute.u128());
    assert_eq!(claim.token, token.token);
    assert_eq!(claim.claimer_pubkey, pubkey_binary);
    assert_eq!(
        claim.sender,
        CrossChainUser::new(
            factory_chain_uid.clone(),
            env.chain(FACTORY_CHAIN_ID).sender().to_string(),
        )
    );

    let new_recipient = CrossChainUser::new(
        factory_chain_uid.clone(),
        env.chain(FACTORY_CHAIN_ID).addr_make("new_recipient").to_string(),
    );

    let claim_msg = ClaimVoucherData {
        claim_id: *claim_id,
        recipients: vec![Recipient {
            recipient: new_recipient.clone(),
            amount: Limit::Dynamic(Uint128::zero()),
            denom: TokenType::Voucher {},
            forwarding_message: None,
            unsafe_refund_as_voucher: Some(true),
        }],
    };

    let signed_data = sign_claim_messsage(signer_key, claim_msg, env.chain(ROUTER_CHAIN_ID).app());

    let router_sender = env.chain(ROUTER_CHAIN_ID).sender();
    let response = env.chain_mut(ROUTER_CHAIN_ID).execute(
        &router_sender,
        &claimer_addr,
        &ClaimerExecuteMsg::ClaimVoucher(signed_data),
        &[],
    );

    relay_router_factory_router(
        response.events,
        FACTORY_CHAIN_ID,
        &factory_addr,
        &factory_chain_uid,
        ROUTER_CHAIN_ID,
        &router_addr,
        &mut env,
    )
    .unwrap();

    let claims = get_user_claims(&env, &claimer_addr, pubkey_binary.clone(), 1, 0);
    assert_eq!(claims.len(), 0);

    let vb_balance = get_vb_balance(&env, &router_addr, &new_recipient, &token.token);
    assert_eq!(vb_balance, amount_to_distribute);
}

#[test]
fn test_claim_voucher_and_release() {
    let (mut env, router_addr, factory_addr, claimer_addr) = setup_env();
    let factory_chain_uid = factory_chain_uid(&env, &factory_addr);

    let claimer_cross_chain_user = CrossChainUser::new(
        ChainUid::vsl_chain_uid().unwrap(),
        claimer_addr.to_string(),
    );
    let token = TokenWithDenom {
        token: Token::create("eucl".to_string()).unwrap(),
        token_type: TokenType::Native { denom: "eucl".to_string() },
    };
    register_token(&factory_addr, FACTORY_CHAIN_ID, &router_addr, ROUTER_CHAIN_ID, &mut env, token.clone()).unwrap();
    let amount_to_distribute = Uint128::from(10_000u128);
    let (signer_key, pubkey_binary) = get_claimer_key();
    let claim_obj = euclid::msgs::claimer::voucher_receive::VoucherReceiveHookMsg::CreateVoucherClaim(
        CreateVoucherClaim {
            claimer_pubkey: pubkey_binary.clone(),
            pseudo_claim_id: Some("pseudo_claim_id".to_string()),
            claim_group_id: Some("group_id".to_string()),
        },
    );

    deposit_token(
        &factory_addr,
        FACTORY_CHAIN_ID,
        &router_addr,
        ROUTER_CHAIN_ID,
        &mut env,
        token.clone(),
        amount_to_distribute,
        vec![Recipient {
            recipient: claimer_cross_chain_user.clone(),
            amount: Limit::Dynamic(Uint128::zero()),
            denom: TokenType::Voucher {},
            forwarding_message: Some(to_json_binary(&claim_obj).unwrap().to_base64()),
            unsafe_refund_as_voucher: None,
        }],
    )
    .unwrap();

    let claims = get_user_claims(&env, &claimer_addr, pubkey_binary.clone(), 1, 0);
    assert_eq!(claims.len(), 1);
    let (claim_id, _claim) = claims.first().unwrap();

    let new_recipient = CrossChainUser::new(
        factory_chain_uid.clone(),
        env.chain(FACTORY_CHAIN_ID).addr_make("new_recipient").to_string(),
    );

    let claim_msg = ClaimVoucherData {
        claim_id: *claim_id,
        recipients: vec![Recipient {
            recipient: new_recipient.clone(),
            amount: Limit::Dynamic(Uint128::zero()),
            denom: token.token_type.clone(),
            forwarding_message: None,
            unsafe_refund_as_voucher: Some(true),
        }],
    };

    let signed_data = sign_claim_messsage(signer_key, claim_msg, env.chain(ROUTER_CHAIN_ID).app());

    let router_sender = env.chain(ROUTER_CHAIN_ID).sender();
    let response = env.chain_mut(ROUTER_CHAIN_ID).execute(
        &router_sender,
        &claimer_addr,
        &ClaimerExecuteMsg::ClaimVoucher(signed_data),
        &[],
    );

    relay_router_factory_router(
        response.events,
        FACTORY_CHAIN_ID,
        &factory_addr,
        &factory_chain_uid,
        ROUTER_CHAIN_ID,
        &router_addr,
        &mut env,
    )
    .unwrap();

    let claims = get_user_claims(&env, &claimer_addr, pubkey_binary.clone(), 1, 0);
    assert_eq!(claims.len(), 0);

    let vb_balance = get_vb_balance(&env, &router_addr, &new_recipient, &token.token);
    assert_eq!(vb_balance, Uint128::zero());

    let native_balance = env.chain(FACTORY_CHAIN_ID).query_balance(
        &Addr::unchecked(new_recipient.address.clone()),
        token.token_type.get_denom().unwrap().as_str(),
    );
    assert_eq!(native_balance, amount_to_distribute);
}

#[test]
fn test_unauthorized_claim_voucher() {
    let (mut env, router_addr, factory_addr, claimer_addr) = setup_env();

    let claimer_cross_chain_user = CrossChainUser::new(
        ChainUid::vsl_chain_uid().unwrap(),
        claimer_addr.to_string(),
    );
    let token = TokenWithDenom {
        token: Token::create("eucl".to_string()).unwrap(),
        token_type: TokenType::Native { denom: "eucl".to_string() },
    };
    register_token(&factory_addr, FACTORY_CHAIN_ID, &router_addr, ROUTER_CHAIN_ID, &mut env, token.clone()).unwrap();
    let amount_to_distribute = Uint128::from(10_000u128);
    let (_, pubkey_binary) = get_claimer_key();
    let claim_obj = euclid::msgs::claimer::voucher_receive::VoucherReceiveHookMsg::CreateVoucherClaim(
        CreateVoucherClaim {
            claimer_pubkey: pubkey_binary.clone(),
            pseudo_claim_id: Some("pseudo_claim_id".to_string()),
            claim_group_id: Some("group_id".to_string()),
        },
    );

    deposit_token(
        &factory_addr,
        FACTORY_CHAIN_ID,
        &router_addr,
        ROUTER_CHAIN_ID,
        &mut env,
        token.clone(),
        amount_to_distribute,
        vec![Recipient {
            recipient: claimer_cross_chain_user.clone(),
            amount: Limit::Dynamic(Uint128::zero()),
            denom: TokenType::Voucher {},
            forwarding_message: Some(to_json_binary(&claim_obj).unwrap().to_base64()),
            unsafe_refund_as_voucher: None,
        }],
    )
    .unwrap();

    let claims = get_user_claims(&env, &claimer_addr, pubkey_binary.clone(), 1, 0);
    assert_eq!(claims.len(), 1);
    let (claim_id, _claim) = claims.first().unwrap();

    let factory_chain_uid = factory_chain_uid(&env, &factory_addr);
    let new_recipient = CrossChainUser::new(
        factory_chain_uid,
        env.chain(FACTORY_CHAIN_ID).addr_make("new_recipient").to_string(),
    );

    // Sign with a DIFFERENT key that doesn't match the pubkey in the claim
    let (wrong_signer_key, _) = get_claimer_key();
    let claim_msg = ClaimVoucherData {
        claim_id: *claim_id,
        recipients: vec![Recipient {
            recipient: new_recipient.clone(),
            amount: Limit::Dynamic(Uint128::zero()),
            denom: token.token_type.clone(),
            forwarding_message: None,
            unsafe_refund_as_voucher: Some(false),
        }],
    };

    let signed_data =
        sign_claim_messsage(wrong_signer_key, claim_msg, env.chain(ROUTER_CHAIN_ID).app());

    let router_sender = env.chain(ROUTER_CHAIN_ID).sender();
    let result = env.chain_mut(ROUTER_CHAIN_ID).try_execute(
        &router_sender,
        &claimer_addr,
        &ClaimerExecuteMsg::ClaimVoucher(signed_data),
        &[],
    );
    assert!(result.is_err(), "Claimer should not be able to claim voucher for another user");
}
