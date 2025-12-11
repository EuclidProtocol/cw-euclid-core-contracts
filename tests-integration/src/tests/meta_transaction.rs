#![cfg(not(target_arch = "wasm32"))]
use cosmwasm_std::{to_json_binary, to_json_string, Addr, Binary, Uint128};
use cw_orch::{
    mock::{cw_multi_test::App, MockBase},
    prelude::{
        ContractInstance, CwOrchError, CwOrchExecute, CwOrchInstantiate, CwOrchUpload, Environment,
    },
};
use cw_orch_interchain::mock::MockInterchainEnv;
use cw_orch_interchain::prelude::*;
use euclid::{
    chain::{ChainUid, CrossChainUser},
    fee::BPS_10_PERCENT,
    msgs::{
        factory::QueryMsgFns as FactoryQueryMsgFns,
        meta_transaction::{
            ExecuteMsgFns as MetaExecuteMsgFns, MetaTransaction, MetaTransactionCallData,
            MetaTransactionData, QueryMsgFns as MetaQueryMsgFns,
        },
        router::{ExecuteMsgFns as RouterExecuteMsgFns, QueryMsgFns as RouterQueryMsgFns},
        virtual_balance::QueryMsgFns as VirtualBalanceQueryMsgFns,
    },
    pool::PoolConfig,
    swap::NextSwapPair,
    token::{PairWithDenomAndAmount, Token, TokenWithDenom},
    virtual_balance::BalanceKey,
};
use euclid_ibc::msg::{
    ChainIbcExecuteMsg, ChainIbcSwapExecuteMsg, ChainIbcTransferExecuteMsg,
    ChainIbcWithdrawExecuteMsg,
};
use factory::FactoryContract;
use k256::ecdsa::SigningKey;
use meta_transaction::MetaTransactionContract;
use relayer::verify::{cosmos_address_from_pubkey, eth_address_from_pubkey, msg_to_sign_data};
use router::RouterContract;
use sha2::{digest::Update, Digest, Sha256};

use crate::helpers::{
    chains::{get_virtual_balance, setup_factory, setup_router},
    factory::{create_pool, deposit_token, register_token},
    relayer::{
        get_random_private_key, get_signer_key_from_pk, get_signer_key_from_pk_evm,
        relay_router_factory_router,
    },
};

fn setup_meta_transaction(
    chain: &MockBase,
    router_address: Addr,
) -> Result<MetaTransactionContract<MockBase>, CwOrchError> {
    let meta_tx_contract = MetaTransactionContract::new(chain.clone());
    meta_tx_contract.upload().unwrap();
    meta_tx_contract.instantiate(
        &euclid::msgs::meta_transaction::InstantiateMsg {
            router_contract: router_address,
        },
        None,
        &[],
    )?;
    Ok(meta_tx_contract)
}

fn setup_meta_transaction_e2e() -> Result<
    (
        RouterContract<MockBase>,
        FactoryContract<MockBase>,
        MetaTransactionContract<MockBase>,
    ),
    CwOrchError,
> {
    // Set up interchain environment with router and factory
    let factory_chain_id = "nibiru";
    let router_chain_id = "euclid";
    let interchain = MockInterchainEnv::new(vec![
        (factory_chain_id, "sender_for_all_chains"),
        (router_chain_id, "sender_for_router"),
    ]);
    let router_chain = interchain.get_chain(router_chain_id).unwrap();
    // Set up router and factory using the helper functions
    let router_contract = setup_router(&router_chain).unwrap();
    let factory_contract = setup_factory(
        &interchain,
        factory_chain_id,
        router_chain_id,
        &router_contract,
    )
    .unwrap();

    let meta_tx_contract =
        setup_meta_transaction(&router_chain, router_contract.address().unwrap()).unwrap();

    router_contract
        .update_router_state(euclid::msgs::router::UpdateRouterState {
            meta_transaction_contract: Some(meta_tx_contract.address().unwrap()),
            admin: None,
            vlp_code_id: None,
            stable_vlp_code_id: None,
            virtual_balance_address: None,
            locked: None,
            mock_relayer_addresses: None,
        })
        .unwrap();

    Ok((router_contract, factory_contract, meta_tx_contract))
}

fn setup_meta_transaction_e2e_evm() -> Result<
    (
        RouterContract<MockBase>,
        FactoryContract<MockBase>,
        MetaTransactionContract<MockBase>,
    ),
    CwOrchError,
> {
    // Set up interchain environment with router and factory
    let factory_chain_id = "ethereum";
    let router_chain_id = "euclid";
    let interchain = MockInterchainEnv::new(vec![
        (factory_chain_id, "sender_for_all_chains"),
        (router_chain_id, "sender_for_router"),
    ]);
    let router_chain = interchain.get_chain(router_chain_id).unwrap();
    // Set up router and factory using the helper functions
    let router_contract = setup_router(&router_chain).unwrap();
    let factory_contract = setup_factory(
        &interchain,
        factory_chain_id,
        router_chain_id,
        &router_contract,
    )
    .unwrap();

    let meta_tx_contract =
        setup_meta_transaction(&router_chain, router_contract.address().unwrap()).unwrap();

    router_contract
        .update_router_state(euclid::msgs::router::UpdateRouterState {
            meta_transaction_contract: Some(meta_tx_contract.address().unwrap()),
            admin: None,
            vlp_code_id: None,
            stable_vlp_code_id: None,
            virtual_balance_address: None,
            locked: None,
            mock_relayer_addresses: None,
        })
        .unwrap();

    Ok((router_contract, factory_contract, meta_tx_contract))
}

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
    app: &App,
    secret_key: &SigningKey,
) -> MetaTransaction {
    // Create MetaTransactionData
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

    // Derive the public key from the secret key (compressed format for Cosmos)
    let pubkey = secret_key
        .verifying_key()
        .to_encoded_point(true) // true = compressed format (33 bytes, starts with 0x02 or 0x03)
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
    app: &App,
    secret_key: &SigningKey,
) -> MetaTransaction {
    // Create MetaTransactionData
    let data = MetaTransactionData {
        signer_address: signer_address.clone(),
        signer_prefix: "0x".to_string(),
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

    // Derive the public key from the secret key (uncompressed format for EVM)
    let pubkey = secret_key
        .verifying_key()
        .to_encoded_point(false) // false = uncompressed format (65 bytes)
        .as_bytes()
        .to_vec();

    MetaTransaction {
        data,
        signature: Binary::from(signature.to_vec()).to_base64(),
        signer_pubkey: Binary::from(pubkey).to_base64(),
    }
}

#[test]
fn test_meta_transaction_instantiation() {
    let chain = <MockBase>::new("nibiru");
    let router = setup_router(&chain).unwrap();
    let meta_tx_contract = setup_meta_transaction(&chain, router.address().unwrap()).unwrap();

    let state = meta_tx_contract.get_state().unwrap();
    assert_eq!(state.router_contract, router.address().unwrap());
    assert_eq!(state.admin, chain.sender);
}

#[test]
fn test_update_admin() {
    let chain = <MockBase>::new("nibiru");
    let router = setup_router(&chain).unwrap();
    let meta_tx_contract = setup_meta_transaction(&chain, router.address().unwrap()).unwrap();

    let new_admin = chain.addr_make("new_admin");

    // Update admin
    let _response = meta_tx_contract
        .execute(
            &euclid::msgs::meta_transaction::ExecuteMsg::UpdateAdmin(
                euclid::msgs::meta_transaction::UpdateAdminMsg {
                    new_admin: new_admin.clone(),
                },
            ),
            &[],
        )
        .unwrap();

    // Verify admin was updated
    let state = meta_tx_contract.get_state().unwrap();
    assert_eq!(state.admin, new_admin);
}

#[test]
fn test_execute_meta_transaction_withdraw_voucher() {
    let (router_contract, factory_contract, meta_tx_contract) =
        setup_meta_transaction_e2e().unwrap();

    let factory_chain_uid = factory_contract.get_state().unwrap().chain_uid.clone();
    let factory_chain = factory_contract.environment();

    // Get signer key and address (this will be different from the sender)
    let (user_secret_key, user_signer_address) = get_signer_key_and_address("user");

    let user = CrossChainUser::new(factory_chain_uid.clone(), user_signer_address.clone());
    println!("User: {}", user.to_sender_string());

    // Get signer key and address (this will be different from the sender)
    let (unauthorized_secret_key, unauthorized_signer_address) =
        get_signer_key_and_address("unauthorized_user");

    let unauthorized_user = CrossChainUser::new(
        factory_chain_uid.clone(),
        unauthorized_signer_address.clone(),
    );
    println!(
        "Unauthorized user: {}",
        unauthorized_user.to_sender_string()
    );

    let token_denom = "tokena";
    // Create tokens
    let token_a = TokenWithDenom {
        token: Token::create("token.a".to_string()).unwrap(),
        token_type: euclid::token::TokenType::Native {
            denom: token_denom.to_string(),
        },
    };

    register_token(&factory_contract, &router_contract, token_a.clone()).unwrap();
    deposit_token(
        &factory_contract,
        &router_contract,
        token_a.clone(),
        Uint128::from(1000u128),
        Some(user.clone()),
        None,
    )
    .unwrap();
    let virtual_balance_contract = get_virtual_balance(
        router_contract.environment(),
        &router_contract
            .get_state()
            .unwrap()
            .virtual_balance_address
            .unwrap(),
    );
    let user_virtual_balance = virtual_balance_contract
        .get_balance(BalanceKey {
            cross_chain_user: user.clone(),
            token_id: token_a.token.to_string(),
        })
        .unwrap();

    assert_eq!(
        factory_chain
            .query_balance(&Addr::unchecked(user.address.clone()), token_denom)
            .unwrap(),
        Uint128::zero(),
        "User native balance should be zero before meta withdraw"
    );

    assert_eq!(
        user_virtual_balance.amount,
        Uint128::from(1000u128),
        "User virtual balance should be amount of tokens deposited before meta withdraw"
    );

    let unauthorized_withdraw = ChainIbcExecuteMsg::Withdraw(ChainIbcWithdrawExecuteMsg {
        sender: user.clone(),
        tx_id: "".to_string(),
        token: token_a.token.clone(),
        amount: Uint128::from(1000u128),
        cross_chain_addresses: vec![unauthorized_user
            .clone()
            .with_limit(None, None, None, None, None, None)],
        timeout: None,
    });
    let unauthorized_withdraw_call_data = MetaTransactionCallData {
        target: router_contract.address().unwrap(),
        call_data: to_json_string(&unauthorized_withdraw).unwrap(),
    };

    // Create and sign the meta transaction
    let signed_meta_tx = sign_meta_transaction_message(
        vec![unauthorized_withdraw_call_data.clone()],
        unauthorized_user.address.clone(),
        "cosmwasm".to_string(),
        factory_chain_uid.clone(),
        "nonce_success_1".to_string(),
        &router_contract.environment().app.borrow(),
        &unauthorized_secret_key,
    );

    // Execute the meta transaction
    let response = meta_tx_contract.execute_meta_transaction(signed_meta_tx);
    assert!(
        response.is_err(),
        "Expected error for unauthorized meta transaction: {}",
        response.err().unwrap()
    );

    // Lets try to withdraw vouchers through a meta transaction
    let withdraw_voucher_msg = ChainIbcExecuteMsg::Withdraw(ChainIbcWithdrawExecuteMsg {
        sender: user.clone(),
        tx_id: "".to_string(),
        token: token_a.token.clone(),
        amount: Uint128::from(1000u128),
        cross_chain_addresses: vec![user.clone().with_limit(None, None, None, None, None, None)],
        timeout: None,
    });
    let withdraw_call_data = MetaTransactionCallData {
        target: router_contract.address().unwrap(),
        call_data: to_json_string(&withdraw_voucher_msg).unwrap(),
    };

    // Create and sign the meta transaction
    let signed_meta_tx = sign_meta_transaction_message(
        vec![withdraw_call_data],
        user.address.clone(),
        "cosmwasm".to_string(),
        factory_chain_uid.clone(),
        "nonce_success_1".to_string(),
        &router_contract.environment().app.borrow(),
        &user_secret_key,
    );

    // Execute the meta transaction
    let response = meta_tx_contract.execute_meta_transaction(signed_meta_tx);

    assert!(
        response.is_ok(),
        "Expected success for meta transaction: {}",
        response.err().unwrap()
    );

    relay_router_factory_router(
        response.unwrap().events,
        &factory_contract,
        &factory_chain_uid,
        &router_contract,
    )
    .unwrap();

    let user_virtual_balance = virtual_balance_contract
        .get_balance(BalanceKey {
            cross_chain_user: user.clone(),
            token_id: token_a.token.to_string(),
        })
        .unwrap();

    assert_eq!(
        factory_chain
            .query_balance(&Addr::unchecked(user.address.clone()), token_denom)
            .unwrap(),
        Uint128::from(1000u128),
        "User native balance should be amount of tokens withdrawn after meta withdraw"
    );

    assert_eq!(
        user_virtual_balance.amount,
        Uint128::zero(),
        "User virtual balance should be zero after meta withdraw"
    );
}

#[test]
fn test_execute_meta_transaction_transfer_voucher() {
    let (router_contract, factory_contract, meta_tx_contract) =
        setup_meta_transaction_e2e().unwrap();

    let factory_chain_uid = factory_contract.get_state().unwrap().chain_uid.clone();
    let factory_chain = factory_contract.environment();

    // Get signer key and address (this will be different from the sender)
    let (user_secret_key, user_signer_address) = get_signer_key_and_address("user");

    let user = CrossChainUser::new(factory_chain_uid.clone(), user_signer_address.clone());
    println!("User: {}", user.to_sender_string());

    // Get signer key and address (this will be different from the sender)
    let (unauthorized_secret_key, unauthorized_signer_address) =
        get_signer_key_and_address("unauthorized_user");

    let unauthorized_user = CrossChainUser::new(
        factory_chain_uid.clone(),
        unauthorized_signer_address.clone(),
    );
    println!(
        "Unauthorized user: {}",
        unauthorized_user.to_sender_string()
    );

    let token_denom = "tokena";
    // Create tokens
    let token_a = TokenWithDenom {
        token: Token::create("token.a".to_string()).unwrap(),
        token_type: euclid::token::TokenType::Native {
            denom: token_denom.to_string(),
        },
    };

    register_token(&factory_contract, &router_contract, token_a.clone()).unwrap();
    deposit_token(
        &factory_contract,
        &router_contract,
        token_a.clone(),
        Uint128::from(1000u128),
        Some(user.clone()),
        None,
    )
    .unwrap();
    let virtual_balance_contract = get_virtual_balance(
        router_contract.environment(),
        &router_contract
            .get_state()
            .unwrap()
            .virtual_balance_address
            .unwrap(),
    );
    let user_virtual_balance = virtual_balance_contract
        .get_balance(BalanceKey {
            cross_chain_user: user.clone(),
            token_id: token_a.token.to_string(),
        })
        .unwrap();

    assert_eq!(
        factory_chain
            .query_balance(&Addr::unchecked(user.address.clone()), token_denom)
            .unwrap(),
        Uint128::zero(),
        "User native balance should be zero before meta withdraw"
    );

    assert_eq!(
        user_virtual_balance.amount,
        Uint128::from(1000u128),
        "User virtual balance should be amount of tokens deposited before meta withdraw"
    );

    let unauthorized_transfer = ChainIbcExecuteMsg::Transfer(ChainIbcTransferExecuteMsg {
        sender: user.clone(),
        token: token_a.token.clone(),
        amount: Uint128::from(1000u128),
        recipient_address: unauthorized_user.clone(),
        from: None,
        msg: None,
        tx_id: "".to_string(),
        timeout: None,
    });
    let unauthorized_transfer_call_data = MetaTransactionCallData {
        target: router_contract.address().unwrap(),
        call_data: to_json_string(&unauthorized_transfer).unwrap(),
    };

    // Create and sign the meta transaction
    let signed_meta_tx = sign_meta_transaction_message(
        vec![unauthorized_transfer_call_data.clone()],
        unauthorized_user.address.clone(),
        "cosmwasm".to_string(),
        factory_chain_uid.clone(),
        "nonce_success_1".to_string(),
        &router_contract.environment().app.borrow(),
        &unauthorized_secret_key,
    );

    // Execute the meta transaction
    let response = meta_tx_contract.execute_meta_transaction(signed_meta_tx);
    assert!(
        response.is_err(),
        "Expected error for unauthorized meta transaction: {}",
        response.err().unwrap()
    );

    let recipient_user = CrossChainUser::new(
        factory_chain_uid.clone(),
        factory_chain.addr_make("recipient_user").to_string(),
    );
    // Lets try to withdraw vouchers through a meta transaction
    let transfer_voucher_msg = ChainIbcExecuteMsg::Transfer(ChainIbcTransferExecuteMsg {
        sender: user.clone(),
        tx_id: "".to_string(),
        token: token_a.token.clone(),
        amount: Uint128::from(1000u128),
        recipient_address: recipient_user.clone(),
        from: None,
        msg: None,
        timeout: None,
    });
    let transfer_call_data = MetaTransactionCallData {
        target: router_contract.address().unwrap(),
        call_data: to_json_string(&transfer_voucher_msg).unwrap(),
    };

    // Create and sign the meta transaction
    let signed_meta_tx = sign_meta_transaction_message(
        vec![transfer_call_data],
        user.address.clone(),
        "cosmwasm".to_string(),
        factory_chain_uid.clone(),
        "nonce_success_1".to_string(),
        &router_contract.environment().app.borrow(),
        &user_secret_key,
    );

    // Execute the meta transaction
    let response = meta_tx_contract.execute_meta_transaction(signed_meta_tx);

    assert!(
        response.is_ok(),
        "Expected success for meta transaction: {}",
        response.err().unwrap()
    );

    relay_router_factory_router(
        response.unwrap().events,
        &factory_contract,
        &factory_chain_uid,
        &router_contract,
    )
    .unwrap();

    let user_virtual_balance = virtual_balance_contract
        .get_balance(BalanceKey {
            cross_chain_user: user.clone(),
            token_id: token_a.token.to_string(),
        })
        .unwrap();

    assert_eq!(
        user_virtual_balance.amount,
        Uint128::zero(),
        "User virtual balance should be zero after meta withdraw"
    );

    let recipient_virtual_balance = virtual_balance_contract
        .get_balance(BalanceKey {
            cross_chain_user: recipient_user.clone(),
            token_id: token_a.token.to_string(),
        })
        .unwrap();

    assert_eq!(
        recipient_virtual_balance.amount,
        Uint128::from(1000u128),
        "Recipient virtual balance should be amount of tokens transferred after meta transfer"
    );
}

#[test]
fn test_execute_meta_transaction_transfer_voucher_evm() {
    let (router_contract, factory_contract, meta_tx_contract) =
        setup_meta_transaction_e2e_evm().unwrap();

    let factory_chain_uid = factory_contract.get_state().unwrap().chain_uid.clone();
    let factory_chain = factory_contract.environment();

    // Get signer key and address (this will be different from the sender)
    let (user_secret_key, user_signer_address) = get_signer_key_and_address_evm("user");

    let user = CrossChainUser::new(factory_chain_uid.clone(), user_signer_address.clone());
    println!("User: {}", user.to_sender_string());

    // Get signer key and address (this will be different from the sender)
    let (unauthorized_secret_key, unauthorized_signer_address) =
        get_signer_key_and_address_evm("unauthorized_user");

    let unauthorized_user = CrossChainUser::new(
        factory_chain_uid.clone(),
        unauthorized_signer_address.clone(),
    );
    println!(
        "Unauthorized user: {}",
        unauthorized_user.to_sender_string()
    );

    let token_denom = "tokena";
    // Create tokens
    let token_a = TokenWithDenom {
        token: Token::create("token.a".to_string()).unwrap(),
        token_type: euclid::token::TokenType::Native {
            denom: token_denom.to_string(),
        },
    };

    register_token(&factory_contract, &router_contract, token_a.clone()).unwrap();
    deposit_token(
        &factory_contract,
        &router_contract,
        token_a.clone(),
        Uint128::from(1000u128),
        Some(user.clone()),
        None,
    )
    .unwrap();
    let virtual_balance_contract = get_virtual_balance(
        router_contract.environment(),
        &router_contract
            .get_state()
            .unwrap()
            .virtual_balance_address
            .unwrap(),
    );
    let user_virtual_balance = virtual_balance_contract
        .get_balance(BalanceKey {
            cross_chain_user: user.clone(),
            token_id: token_a.token.to_string(),
        })
        .unwrap();

    assert_eq!(
        factory_chain
            .query_balance(&Addr::unchecked(user.address.clone()), token_denom)
            .unwrap(),
        Uint128::zero(),
        "User native balance should be zero before meta withdraw"
    );

    assert_eq!(
        user_virtual_balance.amount,
        Uint128::from(1000u128),
        "User virtual balance should be amount of tokens deposited before meta withdraw"
    );

    let unauthorized_transfer = ChainIbcExecuteMsg::Transfer(ChainIbcTransferExecuteMsg {
        sender: user.clone(),
        token: token_a.token.clone(),
        amount: Uint128::from(1000u128),
        recipient_address: unauthorized_user.clone(),
        from: None,
        msg: None,
        tx_id: "".to_string(),
        timeout: None,
    });
    let unauthorized_transfer_call_data = MetaTransactionCallData {
        target: router_contract.address().unwrap(),
        call_data: to_json_string(&unauthorized_transfer).unwrap(),
    };

    // Create and sign the meta transaction
    let signed_meta_tx = sign_meta_transaction_message_evm(
        vec![unauthorized_transfer_call_data.clone()],
        unauthorized_user.address.clone(),
        factory_chain_uid.clone(),
        "nonce_success_1".to_string(),
        &router_contract.environment().app.borrow(),
        &unauthorized_secret_key,
    );

    // Execute the meta transaction
    let response = meta_tx_contract.execute_meta_transaction(signed_meta_tx);
    assert!(
        response.is_err(),
        "Expected error for unauthorized meta transaction: {}",
        response.err().unwrap()
    );

    let recipient_user = CrossChainUser::new(
        factory_chain_uid.clone(),
        factory_chain.addr_make("recipient_user").to_string(),
    );
    // Lets try to withdraw vouchers through a meta transaction
    let transfer_voucher_msg = ChainIbcExecuteMsg::Transfer(ChainIbcTransferExecuteMsg {
        sender: user.clone(),
        tx_id: "".to_string(),
        token: token_a.token.clone(),
        amount: Uint128::from(1000u128),
        recipient_address: recipient_user.clone(),
        from: None,
        msg: None,
        timeout: None,
    });
    let transfer_call_data = MetaTransactionCallData {
        target: router_contract.address().unwrap(),
        call_data: to_json_string(&transfer_voucher_msg).unwrap(),
    };

    // Create and sign the meta transaction
    let signed_meta_tx = sign_meta_transaction_message_evm(
        vec![transfer_call_data],
        user.address.clone(),
        factory_chain_uid.clone(),
        "nonce_success_1".to_string(),
        &router_contract.environment().app.borrow(),
        &user_secret_key,
    );

    // Execute the meta transaction
    let response = meta_tx_contract.execute_meta_transaction(signed_meta_tx);

    assert!(
        response.is_ok(),
        "Expected success for meta transaction: {}",
        response.err().unwrap()
    );

    relay_router_factory_router(
        response.unwrap().events,
        &factory_contract,
        &factory_chain_uid,
        &router_contract,
    )
    .unwrap();

    let user_virtual_balance = virtual_balance_contract
        .get_balance(BalanceKey {
            cross_chain_user: user.clone(),
            token_id: token_a.token.to_string(),
        })
        .unwrap();

    assert_eq!(
        user_virtual_balance.amount,
        Uint128::zero(),
        "User virtual balance should be zero after meta withdraw"
    );

    let recipient_virtual_balance = virtual_balance_contract
        .get_balance(BalanceKey {
            cross_chain_user: recipient_user.clone(),
            token_id: token_a.token.to_string(),
        })
        .unwrap();

    assert_eq!(
        recipient_virtual_balance.amount,
        Uint128::from(1000u128),
        "Recipient virtual balance should be amount of tokens transferred after meta transfer"
    );
}

#[test]
fn test_execute_meta_transaction_swap() {
    let (router_contract, factory_contract, meta_tx_contract) =
        setup_meta_transaction_e2e().unwrap();

    let virtual_balance_contract = get_virtual_balance(
        router_contract.environment(),
        &router_contract
            .get_state()
            .unwrap()
            .virtual_balance_address
            .unwrap(),
    );

    let factory_chain_uid = factory_contract.get_state().unwrap().chain_uid.clone();
    let factory_chain = factory_contract.environment();

    // Get signer key and address (this will be different from the sender)
    let (user_secret_key, user_signer_address) = get_signer_key_and_address("user");

    let user = CrossChainUser::new(factory_chain_uid.clone(), user_signer_address.clone());
    println!("User: {}", user.to_sender_string());

    // Get signer key and address (this will be different from the sender)
    let (unauthorized_secret_key, unauthorized_signer_address) =
        get_signer_key_and_address("unauthorized_user");

    let unauthorized_user = CrossChainUser::new(
        factory_chain_uid.clone(),
        unauthorized_signer_address.clone(),
    );
    println!(
        "Unauthorized user: {}",
        unauthorized_user.to_sender_string()
    );

    let token_denom_a = "tokena";
    // Create tokens
    let token_a = TokenWithDenom {
        token: Token::create("token.a".to_string()).unwrap(),
        token_type: euclid::token::TokenType::Native {
            denom: token_denom_a.to_string(),
        },
    };

    let token_denom_b = "tokenb";
    let token_b = TokenWithDenom {
        token: Token::create("token.b".to_string()).unwrap(),
        token_type: euclid::token::TokenType::Native {
            denom: token_denom_b.to_string(),
        },
    };

    register_token(&factory_contract, &router_contract, token_a.clone()).unwrap();
    register_token(&factory_contract, &router_contract, token_b.clone()).unwrap();
    let pair_info = PairWithDenomAndAmount {
        token_1: token_a.clone().with_amount(Uint128::from(1000000u128)),
        token_2: token_b.clone().with_amount(Uint128::from(1000000u128)),
    };
    create_pool(
        &factory_contract,
        &router_contract,
        pair_info.clone(),
        BPS_10_PERCENT,
        PoolConfig::ConstantProduct {},
    )
    .unwrap();

    deposit_token(
        &factory_contract,
        &router_contract,
        token_a.clone(),
        Uint128::from(1000u128),
        Some(user.clone()),
        None,
    )
    .unwrap();

    for token in [token_a.clone(), token_b.clone()] {
        assert_eq!(
            factory_chain
                .query_balance(
                    &Addr::unchecked(user.address.clone()),
                    &token.token_type.get_denom().unwrap()
                )
                .unwrap(),
            Uint128::zero(),
            "User native balance should be zero before meta withdraw"
        );
    }

    let user_virtual_balance = virtual_balance_contract
        .get_balance(BalanceKey {
            cross_chain_user: user.clone(),
            token_id: token_a.token.to_string(),
        })
        .unwrap();

    assert_eq!(
        user_virtual_balance.amount,
        Uint128::from(1000u128),
        "User virtual balance should be amount of tokens deposited before meta withdraw"
    );

    let token_a_voucher = TokenWithDenom {
        token: token_a.token.clone(),
        token_type: euclid::token::TokenType::Voucher {},
    };

    let mut swap_msg = ChainIbcSwapExecuteMsg {
        sender: user.clone(),
        tx_id: "".to_string(),
        asset_in: token_a_voucher.clone(),
        amount_in: Uint128::from(1000u128),
        asset_out: token_b.token.clone(),
        min_amount_out: Uint128::from(10u128),
        swaps: vec![NextSwapPair {
            token_in: token_a.token.clone(),
            token_out: token_b.token.clone(),
            test_fail: None,
        }],
        partner_fee_amount: Uint128::zero(),
        partner_fee_recipient: unauthorized_user.clone(),
        cross_chain_addresses: vec![unauthorized_user
            .clone()
            .with_limit(None, None, None, None, None, None)],
    };

    let unauthorized_swap = ChainIbcExecuteMsg::Swap(swap_msg.clone());
    let unauthorized_swap_call_data = MetaTransactionCallData {
        target: router_contract.address().unwrap(),
        call_data: to_json_string(&unauthorized_swap).unwrap(),
    };

    // Create and sign the meta transaction
    let signed_meta_tx = sign_meta_transaction_message(
        vec![unauthorized_swap_call_data.clone()],
        unauthorized_user.address.clone(),
        "cosmwasm".to_string(),
        factory_chain_uid.clone(),
        "nonce_success_1".to_string(),
        &router_contract.environment().app.borrow(),
        &unauthorized_secret_key,
    );

    // Execute the meta transaction
    let response = meta_tx_contract.execute_meta_transaction(signed_meta_tx);
    assert!(
        response.is_err(),
        "Expected error for unauthorized meta transaction: {}",
        response.err().unwrap()
    );

    swap_msg.cross_chain_addresses =
        vec![user.clone().with_limit(None, None, None, None, None, None)];

    let mut swap_msg_without_voucher = swap_msg.clone();
    swap_msg_without_voucher.asset_in = token_a;
    let authorized_swap_without_voucher = ChainIbcExecuteMsg::Swap(swap_msg_without_voucher);
    let authorized_swap_without_voucher_call_data = MetaTransactionCallData {
        target: router_contract.address().unwrap(),
        call_data: to_json_string(&authorized_swap_without_voucher).unwrap(),
    };

    // Create and sign the meta transaction
    let signed_meta_tx = sign_meta_transaction_message(
        vec![authorized_swap_without_voucher_call_data],
        user.address.clone(),
        "cosmwasm".to_string(),
        factory_chain_uid.clone(),
        "nonce_success_1".to_string(),
        &router_contract.environment().app.borrow(),
        &user_secret_key,
    );

    // Execute the meta transaction
    let response = meta_tx_contract.execute_meta_transaction(signed_meta_tx);

    assert!(
        response.is_err(),
        "Expected error for meta transaction without voucher: {:?}",
        response.unwrap()
    );
    // Lets try to withdraw vouchers through a meta transaction
    let authorized_swap = ChainIbcExecuteMsg::Swap(swap_msg);
    let authorized_swap_call_data = MetaTransactionCallData {
        target: router_contract.address().unwrap(),
        call_data: to_json_string(&authorized_swap).unwrap(),
    };

    // Create and sign the meta transaction
    let signed_meta_tx = sign_meta_transaction_message(
        vec![authorized_swap_call_data],
        user.address.clone(),
        "cosmwasm".to_string(),
        factory_chain_uid.clone(),
        "nonce_success_1".to_string(),
        &router_contract.environment().app.borrow(),
        &user_secret_key,
    );

    // Execute the meta transaction
    let response = meta_tx_contract.execute_meta_transaction(signed_meta_tx);

    assert!(
        response.is_ok(),
        "Expected success for meta transaction: {}",
        response.err().unwrap()
    );

    relay_router_factory_router(
        response.unwrap().events,
        &factory_contract,
        &factory_chain_uid,
        &router_contract,
    )
    .unwrap();

    assert_eq!(
        factory_chain
            .query_balance(&Addr::unchecked(user.address.clone()), token_denom_b)
            .unwrap(),
        Uint128::from(998u128),
        "User native balance should be amount of tokens swapped after meta swap"
    );
}
