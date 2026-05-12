#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{to_json_binary, to_json_string, Addr, Binary, HexBinary, Uint128, Uint256};
use cw_multi_test::BasicApp;
use euclid::{
    admin::{AdminType, EuclidAdmin},
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    limit::Limit,
    msgs::{
        meta_transaction::{
            ExecuteMsg as MetaExecuteMsg, MetaTransaction, MetaTransactionCallData,
            MetaTransactionData, QueryMsg as MetaQueryMsg,
        },
        virtual_balance::{GetBalanceResponse, QueryMsg as VirtualBalanceQueryMsg},
        vlp::base::PoolConfig,
    },
    recipient::Recipient,
    swap::NextSwapPair,
    token::{PairWithDenomAndAmount, Token, TokenType, TokenWithDenom},
    voucher::BalanceKey,
};
use euclid_ibc::router_ibc::{
    RouterCrossChainExecuteMsg, RouterCrossChainSwapExecuteMsg,
    RouterCrossChainTransferVoucherExecuteMsg,
};
use k256::ecdsa::SigningKey;
use relayer::verify::{cosmos_address_from_pubkey, eth_address_from_pubkey, msg_to_sign_data};
use sha2::{digest::Update, Digest, Sha256};

use crate::helpers::{
    chains::{
        get_meta_tx_addr, get_virtual_balance_addr, setup_factory, setup_interchain, setup_router,
    },
    factory::{create_pool, deposit_token, register_token},
    multi_chain::MultiChainEnv,
    relayer::{
        get_random_private_key, get_signer_key_from_pk, get_signer_key_from_pk_evm,
        relay_router_factory_router,
    },
};
use crate::tests_reusable::constants::{FACTORY_CHAIN_ID_IBC, ROUTER_CHAIN_ID};

const META_FACTORY_CHAIN_ID: &str = FACTORY_CHAIN_ID_IBC;
const META_ROUTER_CHAIN_ID: &str = ROUTER_CHAIN_ID;

fn get_signer_key_and_address(seed: &str) -> (SigningKey, String) {
    let private_key = get_random_private_key(seed);
    let (secret_key, pubkey_binary) = get_signer_key_from_pk(&private_key);
    let signer_address = cosmos_address_from_pubkey(&pubkey_binary, "cosmwasm").unwrap();
    (secret_key, signer_address)
}

fn get_signer_key_and_address_evm(seed: &str) -> (SigningKey, String) {
    let private_key = get_random_private_key(seed);
    let (secret_key, pubkey_binary) = get_signer_key_from_pk_evm(&private_key);
    let signer_address = eth_address_from_pubkey(&pubkey_binary).unwrap();
    (secret_key, signer_address)
}

fn sign_meta_transaction_message(
    call_data: Vec<MetaTransactionCallData>,
    signer_address: String,
    signer_prefix: String,
    signer_chain_uid: ChainUid,
    nonce: String,
    app: &BasicApp,
    secret_key: &SigningKey,
) -> MetaTransaction {
    let data = MetaTransactionData {
        signer_address: signer_address.clone(),
        signer_prefix,
        signer_chain_uid,
        call_data,
        expiry: app.block_info().time.plus_seconds(60).seconds(),
        nonce,
    };

    let msg = msg_to_sign_data(to_json_binary(&data).unwrap(), signer_address.clone());
    let msg = to_json_string(&msg).unwrap();
    let message_digest = Sha256::new().chain(msg.as_bytes());

    let signature = secret_key
        .sign_digest_recoverable(message_digest)
        .unwrap()
        .0;

    let pubkey = secret_key
        .verifying_key()
        .to_encoded_point(true)
        .as_bytes()
        .to_vec();

    MetaTransaction {
        data,
        signature: Binary::from(signature.to_vec()).to_base64(),
        signer_pubkey: Binary::from(pubkey).to_base64(),
    }
}

fn sign_meta_transaction_message_evm(
    call_data: Vec<MetaTransactionCallData>,
    signer_address: String,
    signer_chain_uid: ChainUid,
    nonce: String,
    app: &BasicApp,
    secret_key: &SigningKey,
) -> MetaTransaction {
    let data = MetaTransactionData {
        signer_address: signer_address.clone(),
        signer_prefix: "0x".to_string(),
        signer_chain_uid,
        call_data,
        expiry: app.block_info().time.plus_seconds(60).seconds(),
        nonce,
    };

    let json_payload = to_json_string(&data).unwrap();
    let formatted_message = format!(
        "\x19Ethereum Signed Message:\n{}{}",
        json_payload.len(),
        json_payload
    );
    use sha3::{Digest as KeccakDigest, Keccak256};
    let digest = Keccak256::new().chain_update(formatted_message.as_bytes());

    let signature = secret_key
        .sign_digest_recoverable(digest)
        .expect("failed to sign")
        .0;

    let pubkey = secret_key
        .verifying_key()
        .to_encoded_point(false)
        .as_bytes()
        .to_vec();

    let pubkey_hex = HexBinary::from(pubkey).to_hex();
    let signature_hex = HexBinary::from(signature.to_vec()).to_hex();

    MetaTransaction {
        data,
        signature: signature_hex,
        signer_pubkey: pubkey_hex,
    }
}

fn setup_e2e_single_factory() -> (MultiChainEnv, Addr, Addr, Addr, ChainUid) {
    let sender = "sender_for_all_chains";
    let mut env = setup_interchain(sender, META_FACTORY_CHAIN_ID);
    let router_addr = setup_router(
        env.chain_mut(META_ROUTER_CHAIN_ID),
        vec![META_FACTORY_CHAIN_ID],
    )
    .unwrap();
    let factory_addr = setup_factory(
        &mut env,
        META_FACTORY_CHAIN_ID,
        META_ROUTER_CHAIN_ID,
        &router_addr,
    )
    .unwrap();
    let meta_tx_addr = get_meta_tx_addr(env.chain(META_ROUTER_CHAIN_ID), &router_addr);
    let factory_chain_uid = ChainUid::create(META_FACTORY_CHAIN_ID.to_string()).unwrap();
    (
        env,
        router_addr,
        factory_addr,
        meta_tx_addr,
        factory_chain_uid,
    )
}

fn get_vb_balance(
    env: &MultiChainEnv,
    router_addr: &Addr,
    user: &CrossChainUser,
    token: &Token,
) -> Uint256 {
    let vb_addr = get_virtual_balance_addr(env.chain(META_ROUTER_CHAIN_ID), router_addr);
    let resp: GetBalanceResponse = env.chain(META_ROUTER_CHAIN_ID).query(
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

#[test]
fn test_meta_transaction_instantiation() {
    let sender = "sender_for_all_chains";
    let mut env = setup_interchain(sender, ROUTER_CHAIN_ID);
    let router_addr = setup_router(env.chain_mut(ROUTER_CHAIN_ID), vec![ROUTER_CHAIN_ID]).unwrap();
    let meta_tx_addr = get_meta_tx_addr(env.chain(ROUTER_CHAIN_ID), &router_addr);

    let state: euclid::msgs::meta_transaction::StateResponse = env
        .chain(ROUTER_CHAIN_ID)
        .query(&meta_tx_addr, &MetaQueryMsg::GetState {});
    assert_eq!(state.router_contract, router_addr);

    let router_sender = env.chain(ROUTER_CHAIN_ID).sender();
    let expected_admin = EuclidAdmin::default(router_sender);
    assert_eq!(state.admin, expected_admin);
}

#[test]
fn test_update_admin() {
    let sender = "sender_for_all_chains";
    let mut env = setup_interchain(sender, ROUTER_CHAIN_ID);
    let router_addr = setup_router(env.chain_mut(ROUTER_CHAIN_ID), vec![ROUTER_CHAIN_ID]).unwrap();
    let meta_tx_addr = get_meta_tx_addr(env.chain(ROUTER_CHAIN_ID), &router_addr);

    let new_admin = env.chain(ROUTER_CHAIN_ID).addr_make("new_admin");
    let router_sender = env.chain(ROUTER_CHAIN_ID).sender();

    env.chain_mut(ROUTER_CHAIN_ID).execute(
        &router_sender,
        &meta_tx_addr,
        &MetaExecuteMsg::UpdateAdmin(euclid::msgs::meta_transaction::UpdateAdminMsg {
            new_admin: new_admin.to_string(),
            admin_type: AdminType::GeneralAdmin,
        }),
        &[],
    );

    let state: euclid::msgs::meta_transaction::StateResponse = env
        .chain(ROUTER_CHAIN_ID)
        .query(&meta_tx_addr, &MetaQueryMsg::GetState {});
    let expected_admin = EuclidAdmin::new(new_admin, router_sender.clone(), router_sender);
    assert_eq!(state.admin, expected_admin);
}

#[test]
fn test_execute_meta_transaction_withdraw_voucher() {
    let (mut env, router_addr, factory_addr, meta_tx_addr, factory_chain_uid) =
        setup_e2e_single_factory();

    let (user_secret_key, user_signer_address) = get_signer_key_and_address("user");
    let (unauthorized_secret_key, unauthorized_signer_address) =
        get_signer_key_and_address("unauthorized_user");

    let user = CrossChainUser::new(factory_chain_uid.clone(), user_signer_address.clone());
    let unauthorized_user = CrossChainUser::new(
        factory_chain_uid.clone(),
        unauthorized_signer_address.clone(),
    );

    let token_denom = "tokena";
    let token_a = TokenWithDenom {
        token: Token::create("token.a".to_string()).unwrap(),
        token_type: TokenType::Native {
            denom: token_denom.to_string(),
            decimals: Some(6),
        },
    };

    register_token(
        &factory_addr,
        META_FACTORY_CHAIN_ID,
        &router_addr,
        META_ROUTER_CHAIN_ID,
        &mut env,
        token_a.clone(),
    )
    .unwrap();
    deposit_token(
        &factory_addr,
        META_FACTORY_CHAIN_ID,
        &router_addr,
        META_ROUTER_CHAIN_ID,
        &mut env,
        token_a.clone(),
        Uint256::from(1000u128),
        vec![Recipient::default_voucher_recipient(
            user.clone(),
            Limit::Dynamic(Uint256::zero()),
        )],
    )
    .unwrap();

    let user_addr = Addr::unchecked(user.address.clone());
    let normalized_1000 =
        euclid::normalize::normalize_token_to_voucher(Uint256::from(1000u128), 6).unwrap();
    assert_eq!(
        env.chain(META_FACTORY_CHAIN_ID)
            .query_balance(&user_addr, token_denom),
        Uint256::zero(),
    );
    assert_eq!(
        get_vb_balance(&env, &router_addr, &user, &token_a.token),
        normalized_1000
    );

    // Attempt unauthorized withdraw
    let unauthorized_withdraw =
        RouterCrossChainExecuteMsg::TransferVoucher(RouterCrossChainTransferVoucherExecuteMsg {
            sender: user.clone(),
            tx_id: "".to_string(),
            token: token_a.token.clone(),
            amount: normalized_1000,
            from: None,
            recipients: vec![Recipient::default_voucher_recipient(
                unauthorized_user.clone(),
                Limit::Dynamic(Uint256::zero()),
            )],
        });
    let unauthorized_call_data = MetaTransactionCallData {
        target: router_addr.clone(),
        call_data: to_json_string(&unauthorized_withdraw).unwrap(),
    };
    let unauthorized_meta_tx = sign_meta_transaction_message(
        vec![unauthorized_call_data],
        unauthorized_user.address.clone(),
        "cosmwasm".to_string(),
        factory_chain_uid.clone(),
        "nonce_unauth".to_string(),
        env.chain(META_ROUTER_CHAIN_ID).app(),
        &unauthorized_secret_key,
    );
    let router_sender = env.chain(META_ROUTER_CHAIN_ID).sender();
    let unauth_result = env.chain_mut(META_ROUTER_CHAIN_ID).try_execute(
        &router_sender,
        &meta_tx_addr,
        &MetaExecuteMsg::ExecuteMetaTransaction(unauthorized_meta_tx),
        &[],
    );
    assert!(
        unauth_result.is_err(),
        "Expected unauthorized meta-tx to fail"
    );

    // Authorized withdraw
    let withdraw_msg =
        RouterCrossChainExecuteMsg::TransferVoucher(RouterCrossChainTransferVoucherExecuteMsg {
            sender: user.clone(),
            tx_id: "".to_string(),
            token: token_a.token.clone(),
            amount: normalized_1000,
            from: None,
            recipients: vec![Recipient {
                recipient: user.clone(),
                amount: Limit::Dynamic(Uint256::zero()),
                denom: token_a.token_type.clone(),
                forwarding_message: None,
                unsafe_refund_as_voucher: None,
            }],
        });
    let withdraw_call_data = MetaTransactionCallData {
        target: router_addr.clone(),
        call_data: to_json_string(&withdraw_msg).unwrap(),
    };
    let signed_meta_tx = sign_meta_transaction_message(
        vec![withdraw_call_data],
        user.address.clone(),
        "cosmwasm".to_string(),
        factory_chain_uid.clone(),
        "nonce_success_1".to_string(),
        env.chain(META_ROUTER_CHAIN_ID).app(),
        &user_secret_key,
    );

    let response = env.chain_mut(META_ROUTER_CHAIN_ID).execute(
        &router_sender,
        &meta_tx_addr,
        &MetaExecuteMsg::ExecuteMetaTransaction(signed_meta_tx),
        &[],
    );

    relay_router_factory_router(
        response.events,
        META_FACTORY_CHAIN_ID,
        &factory_addr,
        &factory_chain_uid,
        META_ROUTER_CHAIN_ID,
        &router_addr,
        &mut env,
    )
    .unwrap();

    assert_eq!(
        env.chain(META_FACTORY_CHAIN_ID)
            .query_balance(&user_addr, token_denom),
        Uint256::from(1000u128),
    );
    assert_eq!(
        get_vb_balance(&env, &router_addr, &user, &token_a.token),
        Uint256::zero()
    );
}

#[test]
fn test_execute_meta_transaction_transfer_voucher() {
    let (mut env, router_addr, factory_addr, meta_tx_addr, factory_chain_uid) =
        setup_e2e_single_factory();

    let (user_secret_key, user_signer_address) = get_signer_key_and_address("user2");
    let user = CrossChainUser::new(factory_chain_uid.clone(), user_signer_address.clone());
    let recipient_user = CrossChainUser::new(
        factory_chain_uid.clone(),
        env.chain(META_FACTORY_CHAIN_ID)
            .addr_make("recipient_user")
            .to_string(),
    );

    let token_a = TokenWithDenom {
        token: Token::create("token.a".to_string()).unwrap(),
        token_type: TokenType::Native {
            denom: "tokena".to_string(),
            decimals: Some(6),
        },
    };

    register_token(
        &factory_addr,
        META_FACTORY_CHAIN_ID,
        &router_addr,
        META_ROUTER_CHAIN_ID,
        &mut env,
        token_a.clone(),
    )
    .unwrap();
    deposit_token(
        &factory_addr,
        META_FACTORY_CHAIN_ID,
        &router_addr,
        META_ROUTER_CHAIN_ID,
        &mut env,
        token_a.clone(),
        Uint256::from(1000u128),
        vec![Recipient {
            recipient: user.clone(),
            amount: Limit::Dynamic(Uint256::zero()),
            denom: TokenType::Voucher {},
            forwarding_message: None,
            unsafe_refund_as_voucher: None,
        }],
    )
    .unwrap();

    let normalized_1000 =
        euclid::normalize::normalize_token_to_voucher(Uint256::from(1000u128), 6).unwrap();
    assert_eq!(
        get_vb_balance(&env, &router_addr, &user, &token_a.token),
        normalized_1000
    );

    let transfer_msg =
        RouterCrossChainExecuteMsg::TransferVoucher(RouterCrossChainTransferVoucherExecuteMsg {
            sender: user.clone(),
            tx_id: "".to_string(),
            token: token_a.token.clone(),
            amount: normalized_1000,
            from: None,
            recipients: vec![Recipient {
                recipient: recipient_user.clone(),
                amount: Limit::Dynamic(Uint256::zero()),
                denom: TokenType::Voucher {},
                forwarding_message: None,
                unsafe_refund_as_voucher: None,
            }],
        });
    let transfer_call_data = MetaTransactionCallData {
        target: router_addr.clone(),
        call_data: to_json_string(&transfer_msg).unwrap(),
    };
    let signed_meta_tx = sign_meta_transaction_message(
        vec![transfer_call_data],
        user.address.clone(),
        "cosmwasm".to_string(),
        factory_chain_uid.clone(),
        "nonce_transfer_1".to_string(),
        env.chain(META_ROUTER_CHAIN_ID).app(),
        &user_secret_key,
    );

    let router_sender = env.chain(META_ROUTER_CHAIN_ID).sender();
    let response = env.chain_mut(META_ROUTER_CHAIN_ID).execute(
        &router_sender,
        &meta_tx_addr,
        &MetaExecuteMsg::ExecuteMetaTransaction(signed_meta_tx),
        &[],
    );

    relay_router_factory_router(
        response.events,
        META_FACTORY_CHAIN_ID,
        &factory_addr,
        &factory_chain_uid,
        META_ROUTER_CHAIN_ID,
        &router_addr,
        &mut env,
    )
    .unwrap();

    assert_eq!(
        get_vb_balance(&env, &router_addr, &user, &token_a.token),
        Uint256::zero()
    );
    assert_eq!(
        get_vb_balance(&env, &router_addr, &recipient_user, &token_a.token),
        normalized_1000
    );
}

#[test]
fn test_execute_meta_transaction_swap() {
    let (mut env, router_addr, factory_addr, meta_tx_addr, factory_chain_uid) =
        setup_e2e_single_factory();

    let (user_secret_key, user_signer_address) = get_signer_key_and_address("swap_user");
    let user = CrossChainUser::new(factory_chain_uid.clone(), user_signer_address.clone());

    let token_denom_b = "tokenb";
    let token_a = TokenWithDenom {
        token: Token::create("token.a".to_string()).unwrap(),
        token_type: TokenType::Native {
            denom: "tokena".to_string(),
            decimals: Some(6),
        },
    };
    let token_b = TokenWithDenom {
        token: Token::create("token.b".to_string()).unwrap(),
        token_type: TokenType::Native {
            denom: token_denom_b.to_string(),
            decimals: Some(6),
        },
    };

    register_token(
        &factory_addr,
        META_FACTORY_CHAIN_ID,
        &router_addr,
        META_ROUTER_CHAIN_ID,
        &mut env,
        token_a.clone(),
    )
    .unwrap();
    register_token(
        &factory_addr,
        META_FACTORY_CHAIN_ID,
        &router_addr,
        META_ROUTER_CHAIN_ID,
        &mut env,
        token_b.clone(),
    )
    .unwrap();

    let pair_info = PairWithDenomAndAmount {
        token_1: token_a.clone().with_amount(Uint256::from(1_000_000u128)),
        token_2: token_b.clone().with_amount(Uint256::from(1_000_000u128)),
    };
    create_pool(
        &factory_addr,
        META_FACTORY_CHAIN_ID,
        &router_addr,
        META_ROUTER_CHAIN_ID,
        &mut env,
        pair_info,
        100,
        PoolConfig::ConstantProduct {},
    )
    .unwrap();

    deposit_token(
        &factory_addr,
        META_FACTORY_CHAIN_ID,
        &router_addr,
        META_ROUTER_CHAIN_ID,
        &mut env,
        token_a.clone(),
        Uint256::from(1000u128),
        vec![Recipient {
            recipient: user.clone(),
            amount: Limit::Dynamic(Uint256::zero()),
            denom: TokenType::Voucher {},
            forwarding_message: None,
            unsafe_refund_as_voucher: None,
        }],
    )
    .unwrap();

    let normalized_1000 =
        euclid::normalize::normalize_token_to_voucher(Uint256::from(1000u128), 6).unwrap();
    assert_eq!(
        get_vb_balance(&env, &router_addr, &user, &token_a.token),
        normalized_1000
    );

    let token_a_voucher = TokenWithDenom {
        token: token_a.token.clone(),
        token_type: TokenType::Voucher {},
    };
    let swap_msg = RouterCrossChainExecuteMsg::Swap(RouterCrossChainSwapExecuteMsg {
        sender: user.clone(),
        tx_id: "".to_string(),
        asset_in: token_a_voucher.clone(),
        amount_in: normalized_1000,
        asset_out: token_b.token.clone(),
        min_amount_out: Uint256::from(10u128),
        swaps: vec![NextSwapPair {
            token_in: token_a.token.clone(),
            token_out: token_b.token.clone(),
            test_fail: None,
        }],
        partner_fee_amount: Uint256::zero(),
        partner_fee_recipient: user.clone(),
        recipients: vec![Recipient {
            recipient: user.clone(),
            amount: Limit::Dynamic(Uint256::zero()),
            denom: token_b.token_type.clone(),
            forwarding_message: None,
            unsafe_refund_as_voucher: None,
        }],
    });
    let swap_call_data = MetaTransactionCallData {
        target: router_addr.clone(),
        call_data: to_json_string(&swap_msg).unwrap(),
    };
    let signed_meta_tx = sign_meta_transaction_message(
        vec![swap_call_data],
        user.address.clone(),
        "cosmwasm".to_string(),
        factory_chain_uid.clone(),
        "nonce_swap_1".to_string(),
        env.chain(META_ROUTER_CHAIN_ID).app(),
        &user_secret_key,
    );

    let router_sender = env.chain(META_ROUTER_CHAIN_ID).sender();
    let response = env.chain_mut(META_ROUTER_CHAIN_ID).execute(
        &router_sender,
        &meta_tx_addr,
        &MetaExecuteMsg::ExecuteMetaTransaction(signed_meta_tx),
        &[],
    );

    relay_router_factory_router(
        response.events,
        META_FACTORY_CHAIN_ID,
        &factory_addr,
        &factory_chain_uid,
        META_ROUTER_CHAIN_ID,
        &router_addr,
        &mut env,
    )
    .unwrap();

    assert_eq!(
        get_vb_balance(&env, &router_addr, &user, &token_a.token),
        Uint256::zero()
    );
    let user_native_b = env
        .chain(META_FACTORY_CHAIN_ID)
        .query_balance(&Addr::unchecked(user.address.clone()), token_denom_b);
    assert!(
        user_native_b > Uint256::zero(),
        "User should have received token_b after swap"
    );
}
