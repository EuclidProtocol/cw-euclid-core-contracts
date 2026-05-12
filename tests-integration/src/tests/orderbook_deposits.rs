#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{to_json_binary, to_json_string, Addr, Binary, Uint256};
use euclid::msgs::orderbook_deposits::{
    AssetDepositResponse, AssetTotal, ExecuteMsg as OrderbookExecuteMsg,
    InstantiateMsg as OrderbookInstantiateMsg, MerkleProofStep, Permit, PermitData, ProofPosition,
    QueryMsg as OrderbookQueryMsg, StateResponse, UserDepositResponse, VoucherReceiveHookMsg,
    WhitelistListResponse, WithdrawalLeaf,
};
use euclid::{
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    msgs::virtual_balance::{
        ExecuteMint, ExecuteMsg as VirtualBalanceExecuteMsg, GetBalanceResponse,
        QueryMsg as VirtualBalanceQueryMsg,
    },
    token::TokenType,
    voucher::BalanceKey,
};
use k256::ecdsa::SigningKey;
use relayer::verify::{MsgSignData, MsgSignDataMsg, MsgSignDataValue};
use sha2::{digest::Update, Digest, Sha256};

use crate::helpers::relayer::get_signer_key;
use crate::helpers::{
    app::EuclidApp,
    chains::{orderbook_deposits_code, setup_router},
};
use crate::tests_reusable::constants::ROUTER_CHAIN_ID;

const PERMIT_EXPIRY: u64 = 4_102_444_800u64; // 2100-01-01T00:00:00Z

struct WithdrawTestContext {
    app: EuclidApp,
    router_address: Addr,
    depositor: Addr,
    destination: Addr,
    orderbook_addr: Addr,
    vb_addr: Addr,
    chain_uid: ChainUid,
    signer_key: SigningKey,
    signer_address: String,
}

#[test]
fn deposit_and_query_flow() {
    let token_id = "token1".to_string();
    let deposit_amount = Uint256::new(500);
    let chain_uid = ChainUid::vsl_chain_uid().unwrap();

    let mut app = EuclidApp::new(ROUTER_CHAIN_ID, "sender");
    let depositor = app.addr_make("depositor");
    let router_address = setup_router(&mut app, vec![ROUTER_CHAIN_ID]).unwrap();

    let router_state: euclid::msgs::router::StateResponse = app.query(
        &router_address,
        &euclid::msgs::router::QueryMsg::GetState {},
    );
    let vb_addr = router_state.virtual_balance_address;

    let ob_code_id = orderbook_deposits_code(&mut app);
    let sender = app.sender();
    let orderbook_addr = app.instantiate(
        ob_code_id,
        &sender,
        &OrderbookInstantiateMsg {
            virtual_balance: vb_addr.to_string(),
            admin: Some(router_address.to_string()),
            root_challenge_period: None,
            permit_signer_pubkey: None,
            permit_signer_address: None,
            authorized_posters: Some(vec![router_address.to_string()]),
        },
        &[],
        "orderbook_deposits",
    );

    // SetWhitelist (as router/admin)
    app.execute(
        &router_address,
        &orderbook_addr,
        &OrderbookExecuteMsg::SetWhitelist {
            token_id: token_id.clone(),
            whitelisted: true,
        },
        &[],
    );

    // Register TokenMetadata so Mint can find it
    app.execute(
        &router_address,
        &vb_addr,
        &VirtualBalanceExecuteMsg::RegisterTokenMetadata {
            token_metadata: euclid::token::TokenMetadata::new(
                euclid::token::Token::create(token_id.clone()).unwrap(),
                chain_uid.clone(),
                TokenType::Voucher {},
            ),
        },
        &[],
    );

    // Mint virtual balance tokens
    app.execute(
        &router_address,
        &vb_addr,
        &VirtualBalanceExecuteMsg::Mint(ExecuteMint {
            amount: deposit_amount,
            balance_key: BalanceKey {
                cross_chain_user: CrossChainUser::new(chain_uid.clone(), depositor.to_string()),
                token_id: token_id.clone(),
            },
            token_type: TokenType::Voucher {},
            token_source_chain_uid: chain_uid.clone(),
        }),
        &[],
    );

    // Transfer virtual balance tokens to orderbook (with hook to trigger deposit)
    let hook_msg = to_json_binary(&VoucherReceiveHookMsg::Deposit {}).unwrap();
    app.execute(
        &depositor,
        &vb_addr,
        &VirtualBalanceExecuteMsg::Transfer(euclid::msgs::virtual_balance::ExecuteTransfer {
            amount: deposit_amount,
            token_id: token_id.clone(),
            sender: None,
            to: CrossChainUser::new(chain_uid.clone(), orderbook_addr.to_string()),
            from: None,
            msg: Some(hook_msg),
        }),
        &[],
    );

    let state: StateResponse = app.query(&orderbook_addr, &OrderbookQueryMsg::State {});
    assert_eq!(state.admin, router_address.to_string());
    assert_eq!(state.virtual_balance, vb_addr.to_string());
    assert_eq!(state.status, "active".to_string());

    let asset_deposit: AssetDepositResponse = app.query(
        &orderbook_addr,
        &OrderbookQueryMsg::AssetDeposit {
            token_id: token_id.clone(),
        },
    );
    assert_eq!(asset_deposit.amount, deposit_amount);

    let user_deposit: UserDepositResponse = app.query(
        &orderbook_addr,
        &OrderbookQueryMsg::UserDeposit {
            user: depositor.to_string(),
            token_id: token_id.clone(),
        },
    );
    assert_eq!(user_deposit.amount, deposit_amount);

    let whitelisted: WhitelistListResponse = app.query(
        &orderbook_addr,
        &OrderbookQueryMsg::WhitelistedAssets {
            start_after: None,
            limit: None,
        },
    );
    assert!(whitelisted
        .assets
        .iter()
        .any(|w| w.token_id == token_id && w.whitelisted));
}

#[test]
fn withdraw_with_merkle_and_permit() {
    let token_id = "token1";
    let deposit_amount = Uint256::new(500);
    let withdraw_amount = Uint256::new(300);
    let root_id = "root-1";
    let nonce = 1u64;

    let mut context = setup_withdraw_test_context(0);
    whitelist_token(&mut context, token_id);
    deposit_token(&mut context, token_id, deposit_amount);

    let leaf = WithdrawalLeaf {
        user: context.depositor.to_string(),
        token_id: token_id.to_string(),
        balance: deposit_amount,
    };
    let (root_hash, proof) = build_root_and_proof(&leaf);
    propose_root(
        &mut context,
        root_id,
        root_hash,
        vec![AssetTotal {
            token_id: token_id.to_string(),
            amount: deposit_amount,
        }],
    );

    let permit = sign_permit(
        &context.signer_key,
        context.signer_address.clone(),
        build_permit_data(
            &context,
            root_id,
            token_id,
            withdraw_amount,
            nonce,
            PERMIT_EXPIRY,
        ),
    );

    let depositor = context.depositor.clone();
    context.app.execute(
        &depositor,
        &context.orderbook_addr,
        &OrderbookExecuteMsg::Withdraw {
            root_id: root_id.to_string(),
            amount: withdraw_amount,
            nonce,
            leaf: leaf.clone(),
            proof: proof.clone(),
            permit: permit.clone(),
            destination_chain_uid: context.chain_uid.as_str().to_string(),
            destination: context.destination.to_string(),
        },
        &[],
    );

    let remaining = deposit_amount.checked_sub(withdraw_amount).unwrap();
    assert_eq!(query_asset_deposit(&context, token_id), remaining);
    assert_eq!(query_user_deposit(&context, token_id), remaining);
    assert_eq!(
        query_destination_balance(&context, token_id),
        withdraw_amount
    );

    // Replay should fail
    let replay = context.app.try_execute(
        &depositor,
        &context.orderbook_addr,
        &OrderbookExecuteMsg::Withdraw {
            root_id: root_id.to_string(),
            amount: withdraw_amount,
            nonce,
            leaf,
            proof,
            permit,
            destination_chain_uid: context.chain_uid.as_str().to_string(),
            destination: context.destination.to_string(),
        },
        &[],
    );
    assert!(
        replay.is_err(),
        "Expected replay to fail with PermitAlreadyUsed"
    );
    assert!(replay
        .unwrap_err()
        .to_string()
        .contains("permit already used"));
    assert_eq!(query_asset_deposit(&context, token_id), remaining);
    assert_eq!(query_user_deposit(&context, token_id), remaining);
    assert_eq!(
        query_destination_balance(&context, token_id),
        withdraw_amount
    );
}

#[test]
fn same_nonce_new_permit_bytes_fails() {
    let token_id = "token1";
    let deposit_amount = Uint256::new(500);
    let withdraw_amount = Uint256::new(200);
    let root_id = "root-1";
    let nonce = 3u64;

    let mut context = setup_withdraw_test_context(0);
    whitelist_token(&mut context, token_id);
    deposit_token(&mut context, token_id, deposit_amount);

    let leaf = WithdrawalLeaf {
        user: context.depositor.to_string(),
        token_id: token_id.to_string(),
        balance: deposit_amount,
    };
    let (root_hash, proof) = build_root_and_proof(&leaf);
    propose_root(
        &mut context,
        root_id,
        root_hash,
        vec![AssetTotal {
            token_id: token_id.to_string(),
            amount: deposit_amount,
        }],
    );

    let first_permit = sign_permit(
        &context.signer_key,
        context.signer_address.clone(),
        build_permit_data(
            &context,
            root_id,
            token_id,
            withdraw_amount,
            nonce,
            PERMIT_EXPIRY,
        ),
    );

    let depositor = context.depositor.clone();
    context.app.execute(
        &depositor,
        &context.orderbook_addr,
        &OrderbookExecuteMsg::Withdraw {
            root_id: root_id.to_string(),
            amount: withdraw_amount,
            nonce,
            leaf: leaf.clone(),
            proof: proof.clone(),
            permit: first_permit,
            destination_chain_uid: context.chain_uid.as_str().to_string(),
            destination: context.destination.to_string(),
        },
        &[],
    );

    let second_permit = sign_permit(
        &context.signer_key,
        context.signer_address.clone(),
        build_permit_data(
            &context,
            root_id,
            token_id,
            withdraw_amount,
            nonce,
            PERMIT_EXPIRY - 60,
        ),
    );

    let replay = context.app.try_execute(
        &depositor,
        &context.orderbook_addr,
        &OrderbookExecuteMsg::Withdraw {
            root_id: root_id.to_string(),
            amount: withdraw_amount,
            nonce,
            leaf,
            proof,
            permit: second_permit,
            destination_chain_uid: context.chain_uid.as_str().to_string(),
            destination: context.destination.to_string(),
        },
        &[],
    );
    assert!(
        replay.is_err(),
        "Expected replay to fail with WithdrawalAlreadyConsumed"
    );
    assert!(replay
        .unwrap_err()
        .to_string()
        .contains("withdrawal already consumed"));

    let remaining = deposit_amount.checked_sub(withdraw_amount).unwrap();
    assert_eq!(query_asset_deposit(&context, token_id), remaining);
    assert_eq!(query_user_deposit(&context, token_id), remaining);
    assert_eq!(
        query_destination_balance(&context, token_id),
        withdraw_amount
    );
}

#[test]
fn same_nonce_after_root_rotation_fails() {
    let token_id = "token1";
    let deposit_amount = Uint256::new(1_000);
    let withdraw_amount = Uint256::new(300);
    let nonce = 7u64;
    let root_one = "root-1";
    let root_two = "root-2";

    let mut context = setup_withdraw_test_context(1);
    whitelist_token(&mut context, token_id);
    deposit_token(&mut context, token_id, deposit_amount);

    let initial_leaf = WithdrawalLeaf {
        user: context.depositor.to_string(),
        token_id: token_id.to_string(),
        balance: deposit_amount,
    };
    let (initial_root_hash, initial_proof) = build_root_and_proof(&initial_leaf);
    propose_root(
        &mut context,
        root_one,
        initial_root_hash,
        vec![AssetTotal {
            token_id: token_id.to_string(),
            amount: deposit_amount,
        }],
    );
    activate_root(&mut context, root_one, 1);

    let first_permit = sign_permit(
        &context.signer_key,
        context.signer_address.clone(),
        build_permit_data(
            &context,
            root_one,
            token_id,
            withdraw_amount,
            nonce,
            PERMIT_EXPIRY,
        ),
    );

    let depositor = context.depositor.clone();
    context.app.execute(
        &depositor,
        &context.orderbook_addr,
        &OrderbookExecuteMsg::Withdraw {
            root_id: root_one.to_string(),
            amount: withdraw_amount,
            nonce,
            leaf: initial_leaf,
            proof: initial_proof,
            permit: first_permit,
            destination_chain_uid: context.chain_uid.as_str().to_string(),
            destination: context.destination.to_string(),
        },
        &[],
    );

    let remaining = deposit_amount.checked_sub(withdraw_amount).unwrap();
    let rotated_leaf = WithdrawalLeaf {
        user: context.depositor.to_string(),
        token_id: token_id.to_string(),
        balance: remaining,
    };
    let (rotated_root_hash, rotated_proof) = build_root_and_proof(&rotated_leaf);
    propose_root(
        &mut context,
        root_two,
        rotated_root_hash,
        vec![AssetTotal {
            token_id: token_id.to_string(),
            amount: remaining,
        }],
    );
    activate_root(&mut context, root_two, 1);

    let second_permit = sign_permit(
        &context.signer_key,
        context.signer_address.clone(),
        build_permit_data(
            &context,
            root_two,
            token_id,
            withdraw_amount,
            nonce,
            PERMIT_EXPIRY - 60,
        ),
    );

    let replay = context.app.try_execute(
        &depositor,
        &context.orderbook_addr,
        &OrderbookExecuteMsg::Withdraw {
            root_id: root_two.to_string(),
            amount: withdraw_amount,
            nonce,
            leaf: rotated_leaf,
            proof: rotated_proof,
            permit: second_permit,
            destination_chain_uid: context.chain_uid.as_str().to_string(),
            destination: context.destination.to_string(),
        },
        &[],
    );
    assert!(
        replay.is_err(),
        "Expected replay to fail with WithdrawalAlreadyConsumed"
    );
    assert!(replay
        .unwrap_err()
        .to_string()
        .contains("withdrawal already consumed"));
    assert_eq!(query_asset_deposit(&context, token_id), remaining);
    assert_eq!(query_user_deposit(&context, token_id), remaining);
    assert_eq!(
        query_destination_balance(&context, token_id),
        withdraw_amount
    );
}

#[test]
fn different_nonce_still_succeeds() {
    let token_id = "token1";
    let deposit_amount = Uint256::new(1_000);
    let first_amount = Uint256::new(300);
    let second_amount = Uint256::new(200);
    let root_id = "root-1";

    let mut context = setup_withdraw_test_context(0);
    whitelist_token(&mut context, token_id);
    deposit_token(&mut context, token_id, deposit_amount);

    let leaf = WithdrawalLeaf {
        user: context.depositor.to_string(),
        token_id: token_id.to_string(),
        balance: deposit_amount,
    };
    let (root_hash, proof) = build_root_and_proof(&leaf);
    propose_root(
        &mut context,
        root_id,
        root_hash,
        vec![AssetTotal {
            token_id: token_id.to_string(),
            amount: deposit_amount,
        }],
    );

    let first_permit = sign_permit(
        &context.signer_key,
        context.signer_address.clone(),
        build_permit_data(&context, root_id, token_id, first_amount, 10, PERMIT_EXPIRY),
    );
    let second_permit = sign_permit(
        &context.signer_key,
        context.signer_address.clone(),
        build_permit_data(
            &context,
            root_id,
            token_id,
            second_amount,
            11,
            PERMIT_EXPIRY - 60,
        ),
    );

    let depositor = context.depositor.clone();
    context.app.execute(
        &depositor,
        &context.orderbook_addr,
        &OrderbookExecuteMsg::Withdraw {
            root_id: root_id.to_string(),
            amount: first_amount,
            nonce: 10,
            leaf: leaf.clone(),
            proof: proof.clone(),
            permit: first_permit,
            destination_chain_uid: context.chain_uid.as_str().to_string(),
            destination: context.destination.to_string(),
        },
        &[],
    );
    context.app.execute(
        &depositor,
        &context.orderbook_addr,
        &OrderbookExecuteMsg::Withdraw {
            root_id: root_id.to_string(),
            amount: second_amount,
            nonce: 11,
            leaf,
            proof,
            permit: second_permit,
            destination_chain_uid: context.chain_uid.as_str().to_string(),
            destination: context.destination.to_string(),
        },
        &[],
    );

    let remaining = deposit_amount
        .checked_sub(first_amount)
        .unwrap()
        .checked_sub(second_amount)
        .unwrap();
    let total_withdrawn = first_amount.checked_add(second_amount).unwrap();
    assert_eq!(query_asset_deposit(&context, token_id), remaining);
    assert_eq!(query_user_deposit(&context, token_id), remaining);
    assert_eq!(
        query_destination_balance(&context, token_id),
        total_withdrawn
    );
}

#[test]
fn same_nonce_different_token_still_succeeds() {
    let first_token = "token1";
    let second_token = "token2";
    let first_deposit = Uint256::new(500);
    let second_deposit = Uint256::new(400);
    let first_withdrawal = Uint256::new(200);
    let second_withdrawal = Uint256::new(150);
    let nonce = 21u64;

    let mut context = setup_withdraw_test_context(0);
    whitelist_token(&mut context, first_token);
    whitelist_token(&mut context, second_token);
    deposit_token(&mut context, first_token, first_deposit);
    deposit_token(&mut context, second_token, second_deposit);

    let first_leaf = WithdrawalLeaf {
        user: context.depositor.to_string(),
        token_id: first_token.to_string(),
        balance: first_deposit,
    };
    let (first_root_hash, first_proof) = build_root_and_proof(&first_leaf);
    propose_root(
        &mut context,
        "root-token-1",
        first_root_hash,
        vec![AssetTotal {
            token_id: first_token.to_string(),
            amount: first_deposit,
        }],
    );

    let first_permit = sign_permit(
        &context.signer_key,
        context.signer_address.clone(),
        build_permit_data(
            &context,
            "root-token-1",
            first_token,
            first_withdrawal,
            nonce,
            PERMIT_EXPIRY,
        ),
    );

    let depositor = context.depositor.clone();
    context.app.execute(
        &depositor,
        &context.orderbook_addr,
        &OrderbookExecuteMsg::Withdraw {
            root_id: "root-token-1".to_string(),
            amount: first_withdrawal,
            nonce,
            leaf: first_leaf,
            proof: first_proof,
            permit: first_permit,
            destination_chain_uid: context.chain_uid.as_str().to_string(),
            destination: context.destination.to_string(),
        },
        &[],
    );

    let second_leaf = WithdrawalLeaf {
        user: context.depositor.to_string(),
        token_id: second_token.to_string(),
        balance: second_deposit,
    };
    let (second_root_hash, second_proof) = build_root_and_proof(&second_leaf);
    propose_root(
        &mut context,
        "root-token-2",
        second_root_hash,
        vec![AssetTotal {
            token_id: second_token.to_string(),
            amount: second_deposit,
        }],
    );

    let second_permit = sign_permit(
        &context.signer_key,
        context.signer_address.clone(),
        build_permit_data(
            &context,
            "root-token-2",
            second_token,
            second_withdrawal,
            nonce,
            PERMIT_EXPIRY - 60,
        ),
    );

    context.app.execute(
        &depositor,
        &context.orderbook_addr,
        &OrderbookExecuteMsg::Withdraw {
            root_id: "root-token-2".to_string(),
            amount: second_withdrawal,
            nonce,
            leaf: second_leaf,
            proof: second_proof,
            permit: second_permit,
            destination_chain_uid: context.chain_uid.as_str().to_string(),
            destination: context.destination.to_string(),
        },
        &[],
    );

    assert_eq!(
        query_asset_deposit(&context, first_token),
        first_deposit.checked_sub(first_withdrawal).unwrap()
    );
    assert_eq!(
        query_user_deposit(&context, first_token),
        first_deposit.checked_sub(first_withdrawal).unwrap()
    );
    assert_eq!(
        query_destination_balance(&context, first_token),
        first_withdrawal
    );
    assert_eq!(
        query_asset_deposit(&context, second_token),
        second_deposit.checked_sub(second_withdrawal).unwrap()
    );
    assert_eq!(
        query_user_deposit(&context, second_token),
        second_deposit.checked_sub(second_withdrawal).unwrap()
    );
    assert_eq!(
        query_destination_balance(&context, second_token),
        second_withdrawal
    );
}

#[test]
fn amount_above_leaf_balance_fails_without_nullifiers() {
    let token_id = "token1";
    let deposit_amount = Uint256::new(500);
    let withdraw_amount = Uint256::new(600);
    let root_id = "root-1";
    let nonce = 42u64;

    let mut context = setup_withdraw_test_context(0);
    whitelist_token(&mut context, token_id);
    deposit_token(&mut context, token_id, deposit_amount);

    let leaf = WithdrawalLeaf {
        user: context.depositor.to_string(),
        token_id: token_id.to_string(),
        balance: deposit_amount,
    };
    let (root_hash, proof) = build_root_and_proof(&leaf);
    propose_root(
        &mut context,
        root_id,
        root_hash,
        vec![AssetTotal {
            token_id: token_id.to_string(),
            amount: deposit_amount,
        }],
    );

    let permit = sign_permit(
        &context.signer_key,
        context.signer_address.clone(),
        build_permit_data(
            &context,
            root_id,
            token_id,
            withdraw_amount,
            nonce,
            PERMIT_EXPIRY,
        ),
    );

    let depositor = context.depositor.clone();
    let withdraw = context.app.try_execute(
        &depositor,
        &context.orderbook_addr,
        &OrderbookExecuteMsg::Withdraw {
            root_id: root_id.to_string(),
            amount: withdraw_amount,
            nonce,
            leaf,
            proof,
            permit,
            destination_chain_uid: context.chain_uid.as_str().to_string(),
            destination: context.destination.to_string(),
        },
        &[],
    );
    assert!(
        withdraw.is_err(),
        "Expected error for amount above leaf balance"
    );
    assert!(withdraw
        .unwrap_err()
        .to_string()
        .contains("insufficient withdrawable balance"));
    assert_eq!(query_asset_deposit(&context, token_id), deposit_amount);
    assert_eq!(query_user_deposit(&context, token_id), deposit_amount);
    assert_eq!(
        query_destination_balance(&context, token_id),
        Uint256::zero()
    );
}

#[test]
fn withdraw_rejects_invalid_merkle_proof() {
    let token_id = "token1";
    let deposit_amount = Uint256::new(500);
    let withdraw_amount = Uint256::new(200);
    let root_id = "root-invalid";
    let nonce = 7u64;

    let mut context = setup_withdraw_test_context(0);
    whitelist_token(&mut context, token_id);
    deposit_token(&mut context, token_id, deposit_amount);

    let leaf = WithdrawalLeaf {
        user: context.depositor.to_string(),
        token_id: token_id.to_string(),
        balance: deposit_amount,
    };
    let sibling_hash = {
        let sibling = WithdrawalLeaf {
            user: "other".to_string(),
            token_id: token_id.to_string(),
            balance: Uint256::zero(),
        };
        hash_leaf(&sibling)
    };
    let (root_hash, _correct_proof) = build_root_and_proof(&leaf);
    propose_root(
        &mut context,
        root_id,
        root_hash,
        vec![AssetTotal {
            token_id: token_id.to_string(),
            amount: deposit_amount,
        }],
    );

    let permit = sign_permit(
        &context.signer_key,
        context.signer_address.clone(),
        build_permit_data(
            &context,
            root_id,
            token_id,
            withdraw_amount,
            nonce,
            PERMIT_EXPIRY,
        ),
    );

    let bad_position_proof = vec![MerkleProofStep {
        hash: Binary::from(sibling_hash.to_vec()),
        position: ProofPosition::Left,
    }];
    let depositor = context.depositor.clone();
    let bad_position = context.app.try_execute(
        &depositor,
        &context.orderbook_addr,
        &OrderbookExecuteMsg::Withdraw {
            root_id: root_id.to_string(),
            amount: withdraw_amount,
            nonce,
            leaf: leaf.clone(),
            proof: bad_position_proof,
            permit: permit.clone(),
            destination_chain_uid: context.chain_uid.as_str().to_string(),
            destination: context.destination.to_string(),
        },
        &[],
    );
    assert!(bad_position.is_err());

    let mut bad_hash_bytes = sibling_hash;
    bad_hash_bytes[0] ^= 0x01;
    let permit2 = sign_permit(
        &context.signer_key,
        context.signer_address.clone(),
        build_permit_data(
            &context,
            root_id,
            token_id,
            withdraw_amount,
            nonce,
            PERMIT_EXPIRY - 60,
        ),
    );
    let bad_hash_proof = vec![MerkleProofStep {
        hash: Binary::from(bad_hash_bytes.to_vec()),
        position: ProofPosition::Right,
    }];
    let bad_hash_result = context.app.try_execute(
        &depositor,
        &context.orderbook_addr,
        &OrderbookExecuteMsg::Withdraw {
            root_id: root_id.to_string(),
            amount: withdraw_amount,
            nonce,
            leaf,
            proof: bad_hash_proof,
            permit: permit2,
            destination_chain_uid: context.chain_uid.as_str().to_string(),
            destination: context.destination.to_string(),
        },
        &[],
    );
    assert!(bad_hash_result.is_err());
}

fn setup_withdraw_test_context(root_challenge_period: u64) -> WithdrawTestContext {
    let mut app = EuclidApp::new(ROUTER_CHAIN_ID, "sender");
    let depositor = app.addr_make("depositor");
    let destination = app.addr_make("destination");
    let router_address = setup_router(&mut app, vec![ROUTER_CHAIN_ID]).unwrap();

    let router_state: euclid::msgs::router::StateResponse = app.query(
        &router_address,
        &euclid::msgs::router::QueryMsg::GetState {},
    );
    let vb_addr = router_state.virtual_balance_address;

    let (signer_key, signer_pubkey) = get_signer_key();
    let chain_id = app.chain_id().to_string();
    let signer_address = format!("permit_{chain_id}");

    let ob_code_id = orderbook_deposits_code(&mut app);
    let sender = app.sender();
    let orderbook_addr = app.instantiate(
        ob_code_id,
        &sender,
        &OrderbookInstantiateMsg {
            virtual_balance: vb_addr.to_string(),
            admin: Some(router_address.to_string()),
            root_challenge_period: Some(root_challenge_period),
            permit_signer_pubkey: Some(signer_pubkey.clone()),
            permit_signer_address: Some(signer_address.clone()),
            authorized_posters: Some(vec![router_address.to_string()]),
        },
        &[],
        "orderbook_deposits",
    );

    WithdrawTestContext {
        app,
        router_address,
        depositor,
        destination,
        orderbook_addr,
        vb_addr,
        chain_uid: ChainUid::vsl_chain_uid().unwrap(),
        signer_key,
        signer_address,
    }
}

fn whitelist_token(context: &mut WithdrawTestContext, token_id: &str) {
    let router_address = context.router_address.clone();
    context.app.execute(
        &router_address,
        &context.orderbook_addr,
        &OrderbookExecuteMsg::SetWhitelist {
            token_id: token_id.to_string(),
            whitelisted: true,
        },
        &[],
    );
}

fn deposit_token(context: &mut WithdrawTestContext, token_id: &str, deposit_amount: Uint256) {
    let router_address = context.router_address.clone();
    let depositor = context.depositor.clone();
    let chain_uid = context.chain_uid.clone();
    let orderbook_addr = context.orderbook_addr.clone();

    // Register TokenMetadata so Mint can find it
    let vb_addr_copy = context.vb_addr.clone();
    context.app.execute(
        &router_address,
        &vb_addr_copy,
        &VirtualBalanceExecuteMsg::RegisterTokenMetadata {
            token_metadata: euclid::token::TokenMetadata::new(
                euclid::token::Token::create(token_id.to_string()).unwrap(),
                chain_uid.clone(),
                TokenType::Voucher {},
            ),
        },
        &[],
    );

    // Mint virtual balance tokens for depositor
    context.app.execute(
        &router_address,
        &context.vb_addr,
        &VirtualBalanceExecuteMsg::Mint(ExecuteMint {
            amount: deposit_amount,
            balance_key: BalanceKey {
                cross_chain_user: CrossChainUser::new(chain_uid.clone(), depositor.to_string()),
                token_id: token_id.to_string(),
            },
            token_type: TokenType::Voucher {},
            token_source_chain_uid: chain_uid.clone(),
        }),
        &[],
    );

    // Depositor transfers to orderbook with hook
    let hook_msg = to_json_binary(&VoucherReceiveHookMsg::Deposit {}).unwrap();
    let vb_addr = context.vb_addr.clone();
    context.app.execute(
        &depositor,
        &vb_addr,
        &VirtualBalanceExecuteMsg::Transfer(euclid::msgs::virtual_balance::ExecuteTransfer {
            amount: deposit_amount,
            token_id: token_id.to_string(),
            sender: None,
            to: CrossChainUser::new(chain_uid.clone(), orderbook_addr.to_string()),
            from: None,
            msg: Some(hook_msg),
        }),
        &[],
    );
}

fn build_permit_data(
    context: &WithdrawTestContext,
    root_id: &str,
    token_id: &str,
    amount: Uint256,
    nonce: u64,
    expiry: u64,
) -> PermitData {
    PermitData {
        root_id: root_id.to_string(),
        user: context.depositor.to_string(),
        token_id: token_id.to_string(),
        amount,
        nonce,
        destination_chain_uid: context.chain_uid.as_str().to_string(),
        destination: context.destination.to_string(),
        expiry,
    }
}

fn build_root_and_proof(leaf: &WithdrawalLeaf) -> ([u8; 32], Vec<MerkleProofStep>) {
    let sibling = WithdrawalLeaf {
        user: "other".to_string(),
        token_id: leaf.token_id.clone(),
        balance: Uint256::zero(),
    };
    let leaf_hash = hash_leaf(leaf);
    let sibling_hash = hash_leaf(&sibling);
    (
        hash_pair(&leaf_hash, &sibling_hash),
        vec![MerkleProofStep {
            hash: Binary::from(sibling_hash.to_vec()),
            position: ProofPosition::Right,
        }],
    )
}

fn propose_root(
    context: &mut WithdrawTestContext,
    root_id: &str,
    root_hash: [u8; 32],
    per_asset_totals: Vec<AssetTotal>,
) {
    let router_address = context.router_address.clone();
    context.app.execute(
        &router_address,
        &context.orderbook_addr,
        &OrderbookExecuteMsg::ProposeRoot {
            root_id: root_id.to_string(),
            root_hash: Binary::from(root_hash.to_vec()),
            per_asset_totals,
            da_hash: None,
            da_url: None,
        },
        &[],
    );
}

fn activate_root(context: &mut WithdrawTestContext, root_id: &str, seconds: u64) {
    context.app.app_mut().update_block(|block| {
        block.height += 1;
        block.time = block.time.plus_seconds(seconds);
    });
    let router_address = context.router_address.clone();
    context.app.execute(
        &router_address,
        &context.orderbook_addr,
        &OrderbookExecuteMsg::ActivateRoot {
            root_id: root_id.to_string(),
        },
        &[],
    );
}

fn query_asset_deposit(context: &WithdrawTestContext, token_id: &str) -> Uint256 {
    let asset_deposit: AssetDepositResponse = context.app.query(
        &context.orderbook_addr,
        &OrderbookQueryMsg::AssetDeposit {
            token_id: token_id.to_string(),
        },
    );
    asset_deposit.amount
}

fn query_user_deposit(context: &WithdrawTestContext, token_id: &str) -> Uint256 {
    let user_deposit: UserDepositResponse = context.app.query(
        &context.orderbook_addr,
        &OrderbookQueryMsg::UserDeposit {
            user: context.depositor.to_string(),
            token_id: token_id.to_string(),
        },
    );
    user_deposit.amount
}

fn query_destination_balance(context: &WithdrawTestContext, token_id: &str) -> Uint256 {
    let balance: GetBalanceResponse = context.app.query(
        &context.vb_addr,
        &VirtualBalanceQueryMsg::GetBalance {
            balance_key: BalanceKey {
                cross_chain_user: CrossChainUser::new(
                    context.chain_uid.clone(),
                    context.destination.to_string(),
                ),
                token_id: token_id.to_string(),
            },
        },
    );
    balance.amount
}

fn sign_permit(signer_key: &SigningKey, signer_address: String, permit_data: PermitData) -> Permit {
    let msg = MsgSignDataMsg::new(MsgSignDataValue::new(
        to_json_binary(&permit_data).unwrap(),
        signer_address,
    ));
    let msg = MsgSignData::new(vec![msg]);
    let msg = to_json_string(&msg).unwrap();
    let message_digest = Sha256::new().chain(msg.as_bytes());
    let signature = signer_key
        .sign_digest_recoverable(message_digest)
        .unwrap()
        .0;
    Permit {
        data: msg,
        signature: Binary::from(signature.to_vec()),
    }
}

fn hash_leaf(leaf: &WithdrawalLeaf) -> [u8; 32] {
    let bytes = to_json_binary(leaf).unwrap();
    Sha256::digest(bytes.as_slice()).into()
}

fn hash_pair(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut data = Vec::with_capacity(64);
    data.extend_from_slice(left);
    data.extend_from_slice(right);
    Sha256::digest(data.as_slice()).into()
}
