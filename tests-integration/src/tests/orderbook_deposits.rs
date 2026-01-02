#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{to_json_binary, to_json_string, Binary, Empty, Uint128};
use cw_multi_test::{App, Contract, ContractWrapper, Executor};
use euclid::{
    chain::{ChainUid, CrossChainUser},
    msgs::virtual_balance::{
        ExecuteMint, ExecuteMsg as VirtualBalanceExecuteMsg, GetBalanceResponse,
        InstantiateMsg as VirtualBalanceInstantiateMsg, QueryMsg as VirtualBalanceQueryMsg,
    },
    virtual_balance::BalanceKey,
};
use k256::ecdsa::SigningKey;
use orderbook_deposits::msg::{
    AssetDepositResponse, MerkleProofStep, Permit, PermitData, ProofPosition,
    QueryMsg as OrderbookQueryMsg, StateResponse, UserDepositResponse,
    VirtualBalanceReceiveHookMsg, WhitelistListResponse, WithdrawalLeaf,
};
use orderbook_deposits::msg::{
    ExecuteMsg as OrderbookExecuteMsg, InstantiateMsg as OrderbookInstantiateMsg,
};
use orderbook_deposits::state::AssetTotal;
use relayer::verify::{MsgSignData, MsgSignDataMsg, MsgSignDataValue};
use sha2::{digest::Update, Digest, Sha256};

use crate::helpers::relayer::get_signer_key;

fn orderbook_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new(
        orderbook_deposits::contract::execute,
        orderbook_deposits::contract::instantiate,
        orderbook_deposits::contract::query,
    ))
}

fn virtual_balance_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new(
        virtual_balance::contract::execute,
        virtual_balance::contract::instantiate,
        virtual_balance::contract::query,
    ))
}

#[test]
fn deposit_and_query_flow() {
    let mut app = App::default();

    let router = app.api().addr_make("router");
    let depositor = app.api().addr_make("depositor");
    let token_id = "token1".to_string();
    let deposit_amount = Uint128::new(500);
    let chain_uid = ChainUid::vsl_chain_uid().unwrap();

    let vb_code_id = app.store_code(virtual_balance_contract());
    let ob_code_id = app.store_code(orderbook_contract());

    // Instantiate virtual balance with router as instantiator (also router field)
    let virtual_balance_addr = app
        .instantiate_contract(
            vb_code_id,
            router.clone(),
            &VirtualBalanceInstantiateMsg {
                router: router.clone(),
                admin: Some(router.clone()),
            },
            &[],
            "virtual_balance",
            None,
        )
        .unwrap();

    // Instantiate orderbook deposits pointing to virtual balance
    let orderbook_addr = app
        .instantiate_contract(
            ob_code_id,
            router.clone(),
            &OrderbookInstantiateMsg {
                virtual_balance: virtual_balance_addr.to_string(),
                admin: Some(router.to_string()),
                root_challenge_period: None,
                permit_signer_pubkey: None,
                permit_signer_address: None,
                authorized_posters: None,
            },
            &[],
            "orderbook_deposits",
            None,
        )
        .unwrap();

    // Whitelist token as admin
    app.execute_contract(
        router.clone(),
        orderbook_addr.clone(),
        &OrderbookExecuteMsg::SetWhitelist {
            token_id: token_id.clone(),
            whitelisted: true,
        },
        &[],
    )
    .unwrap();

    // Mint virtual balance to depositor (router authority)
    app.execute_contract(
        router.clone(),
        virtual_balance_addr.clone(),
        &VirtualBalanceExecuteMsg::Mint(ExecuteMint {
            amount: deposit_amount,
            balance_key: BalanceKey {
                cross_chain_user: CrossChainUser::new(chain_uid.clone(), depositor.to_string()),
                token_id: token_id.clone(),
            },
        }),
        &[],
    )
    .unwrap();

    // Deposit via virtual balance transfer with hook
    let hook_msg = cosmwasm_std::to_json_binary(&VirtualBalanceReceiveHookMsg::Deposit {}).unwrap();
    app.execute_contract(
        depositor.clone(),
        virtual_balance_addr.clone(),
        &VirtualBalanceExecuteMsg::Transfer(euclid::msgs::virtual_balance::ExecuteTransfer {
            amount: deposit_amount,
            token_id: token_id.clone(),
            sender: None,
            to: CrossChainUser::new(chain_uid.clone(), orderbook_addr.to_string()),
            from: None,
            msg: Some(hook_msg),
        }),
        &[],
    )
    .unwrap();

    // Verify state query
    let state: StateResponse = app
        .wrap()
        .query_wasm_smart(orderbook_addr.clone(), &OrderbookQueryMsg::State {})
        .unwrap();
    assert_eq!(state.admin, router.to_string());
    assert_eq!(state.virtual_balance, virtual_balance_addr.to_string());
    assert_eq!(state.status, "active".to_string());

    // Verify aggregate deposit
    let asset_deposit: AssetDepositResponse = app
        .wrap()
        .query_wasm_smart(
            orderbook_addr.clone(),
            &OrderbookQueryMsg::AssetDeposit {
                token_id: token_id.clone(),
            },
        )
        .unwrap();
    assert_eq!(asset_deposit.amount, deposit_amount);

    // Verify user deposit
    let user_deposit: UserDepositResponse = app
        .wrap()
        .query_wasm_smart(
            orderbook_addr.clone(),
            &OrderbookQueryMsg::UserDeposit {
                user: depositor.to_string(),
                token_id: token_id.clone(),
            },
        )
        .unwrap();
    assert_eq!(user_deposit.amount, deposit_amount);

    // Verify whitelist listing
    let whitelisted: WhitelistListResponse = app
        .wrap()
        .query_wasm_smart(
            orderbook_addr,
            &OrderbookQueryMsg::WhitelistedAssets {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();
    assert!(whitelisted
        .assets
        .iter()
        .any(|w| w.token_id == token_id && w.whitelisted));
}

#[test]
fn withdraw_with_merkle_and_permit() {
    let mut app = App::default();

    let router = app.api().addr_make("router");
    let depositor = app.api().addr_make("depositor");
    let destination = app.api().addr_make("destination");
    let token_id = "token1".to_string();
    let deposit_amount = Uint128::new(500);
    let withdraw_amount = Uint128::new(300);
    let chain_uid = ChainUid::vsl_chain_uid().unwrap();
    let root_id = "root-1".to_string();

    let (signer_key, signer_pubkey) = get_signer_key();
    let signer_address = format!("permit_{}", app.block_info().chain_id);

    let vb_code_id = app.store_code(virtual_balance_contract());
    let ob_code_id = app.store_code(orderbook_contract());

    let virtual_balance_addr = app
        .instantiate_contract(
            vb_code_id,
            router.clone(),
            &VirtualBalanceInstantiateMsg {
                router: router.clone(),
                admin: Some(router.clone()),
            },
            &[],
            "virtual_balance",
            None,
        )
        .unwrap();

    let orderbook_addr = app
        .instantiate_contract(
            ob_code_id,
            router.clone(),
            &OrderbookInstantiateMsg {
                virtual_balance: virtual_balance_addr.to_string(),
                admin: Some(router.to_string()),
                root_challenge_period: Some(0),
                permit_signer_pubkey: Some(signer_pubkey.clone()),
                permit_signer_address: Some(signer_address.clone()),
                authorized_posters: None,
            },
            &[],
            "orderbook_deposits",
            None,
        )
        .unwrap();

    app.execute_contract(
        router.clone(),
        orderbook_addr.clone(),
        &OrderbookExecuteMsg::SetWhitelist {
            token_id: token_id.clone(),
            whitelisted: true,
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        router.clone(),
        virtual_balance_addr.clone(),
        &VirtualBalanceExecuteMsg::Mint(ExecuteMint {
            amount: deposit_amount,
            balance_key: BalanceKey {
                cross_chain_user: CrossChainUser::new(chain_uid.clone(), depositor.to_string()),
                token_id: token_id.clone(),
            },
        }),
        &[],
    )
    .unwrap();

    let hook_msg = to_json_binary(&VirtualBalanceReceiveHookMsg::Deposit {}).unwrap();
    app.execute_contract(
        depositor.clone(),
        virtual_balance_addr.clone(),
        &VirtualBalanceExecuteMsg::Transfer(euclid::msgs::virtual_balance::ExecuteTransfer {
            amount: deposit_amount,
            token_id: token_id.clone(),
            sender: None,
            to: CrossChainUser::new(chain_uid.clone(), orderbook_addr.to_string()),
            from: None,
            msg: Some(hook_msg),
        }),
        &[],
    )
    .unwrap();

    let nonce = 1u64;
    let leaf = WithdrawalLeaf {
        user: depositor.to_string(),
        token_id: token_id.clone(),
        balance: deposit_amount,
    };
    let sibling = WithdrawalLeaf {
        user: "other".to_string(),
        token_id: token_id.clone(),
        balance: Uint128::zero(),
    };
    let leaf_hash = hash_leaf(&leaf);
    let sibling_hash = hash_leaf(&sibling);
    let root_hash = hash_pair(&leaf_hash, &sibling_hash);

    app.execute_contract(
        router.clone(),
        orderbook_addr.clone(),
        &OrderbookExecuteMsg::ProposeRoot {
            root_id: root_id.clone(),
            root_hash: Binary::from(root_hash.to_vec()),
            per_asset_totals: vec![AssetTotal {
                token_id: token_id.clone(),
                amount: deposit_amount,
            }],
            da_hash: None,
            da_url: None,
        },
        &[],
    )
    .unwrap();

    let permit_data = PermitData {
        root_id: root_id.clone(),
        user: depositor.to_string(),
        token_id: token_id.clone(),
        amount: withdraw_amount,
        nonce,
        destination_chain_uid: chain_uid.as_str().to_string(),
        destination: destination.to_string(),
        expiry: app.block_info().time.plus_seconds(60).seconds(),
    };
    let permit = sign_permit(&signer_key, signer_address.clone(), permit_data);

    let proof = vec![MerkleProofStep {
        hash: Binary::from(sibling_hash.to_vec()),
        position: ProofPosition::Right,
    }];

    app.execute_contract(
        depositor.clone(),
        orderbook_addr.clone(),
        &OrderbookExecuteMsg::Withdraw {
            root_id: root_id.clone(),
            amount: withdraw_amount,
            nonce,
            leaf: leaf.clone(),
            proof: proof.clone(),
            permit: permit.clone(),
            destination_chain_uid: chain_uid.as_str().to_string(),
            destination: destination.to_string(),
        },
        &[],
    )
    .unwrap();

    let asset_deposit: AssetDepositResponse = app
        .wrap()
        .query_wasm_smart(
            orderbook_addr.clone(),
            &OrderbookQueryMsg::AssetDeposit {
                token_id: token_id.clone(),
            },
        )
        .unwrap();
    assert_eq!(
        asset_deposit.amount,
        deposit_amount.checked_sub(withdraw_amount).unwrap()
    );

    let destination_balance: GetBalanceResponse = app
        .wrap()
        .query_wasm_smart(
            virtual_balance_addr.clone(),
            &VirtualBalanceQueryMsg::GetBalance {
                balance_key: BalanceKey {
                    cross_chain_user: CrossChainUser::new(chain_uid.clone(), destination.to_string()),
                    token_id: token_id.clone(),
                },
            },
        )
        .unwrap();
    assert_eq!(destination_balance.amount, withdraw_amount);

    let replay = app.execute_contract(
        depositor,
        orderbook_addr,
        &OrderbookExecuteMsg::Withdraw {
            root_id,
            amount: withdraw_amount,
            nonce,
            leaf,
            proof,
            permit,
            destination_chain_uid: chain_uid.as_str().to_string(),
            destination: destination.to_string(),
        },
        &[],
    );
    assert!(replay.is_err());
}

#[test]
fn withdraw_rejects_invalid_merkle_proof() {
    let mut app = App::default();

    let router = app.api().addr_make("router");
    let depositor = app.api().addr_make("depositor");
    let destination = app.api().addr_make("destination");
    let token_id = "token1".to_string();
    let deposit_amount = Uint128::new(500);
    let withdraw_amount = Uint128::new(200);
    let chain_uid = ChainUid::vsl_chain_uid().unwrap();
    let root_id = "root-invalid".to_string();

    let (signer_key, signer_pubkey) = get_signer_key();
    let signer_address = format!("permit_{}", app.block_info().chain_id);

    let vb_code_id = app.store_code(virtual_balance_contract());
    let ob_code_id = app.store_code(orderbook_contract());

    let virtual_balance_addr = app
        .instantiate_contract(
            vb_code_id,
            router.clone(),
            &VirtualBalanceInstantiateMsg {
                router: router.clone(),
                admin: Some(router.clone()),
            },
            &[],
            "virtual_balance",
            None,
        )
        .unwrap();

    let orderbook_addr = app
        .instantiate_contract(
            ob_code_id,
            router.clone(),
            &OrderbookInstantiateMsg {
                virtual_balance: virtual_balance_addr.to_string(),
                admin: Some(router.to_string()),
                root_challenge_period: Some(0),
                permit_signer_pubkey: Some(signer_pubkey.clone()),
                permit_signer_address: Some(signer_address.clone()),
                authorized_posters: None,
            },
            &[],
            "orderbook_deposits",
            None,
        )
        .unwrap();

    app.execute_contract(
        router.clone(),
        orderbook_addr.clone(),
        &OrderbookExecuteMsg::SetWhitelist {
            token_id: token_id.clone(),
            whitelisted: true,
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        router.clone(),
        virtual_balance_addr.clone(),
        &VirtualBalanceExecuteMsg::Mint(ExecuteMint {
            amount: deposit_amount,
            balance_key: BalanceKey {
                cross_chain_user: CrossChainUser::new(chain_uid.clone(), depositor.to_string()),
                token_id: token_id.clone(),
            },
        }),
        &[],
    )
    .unwrap();

    let hook_msg = to_json_binary(&VirtualBalanceReceiveHookMsg::Deposit {}).unwrap();
    app.execute_contract(
        depositor.clone(),
        virtual_balance_addr.clone(),
        &VirtualBalanceExecuteMsg::Transfer(euclid::msgs::virtual_balance::ExecuteTransfer {
            amount: deposit_amount,
            token_id: token_id.clone(),
            sender: None,
            to: CrossChainUser::new(chain_uid.clone(), orderbook_addr.to_string()),
            from: None,
            msg: Some(hook_msg),
        }),
        &[],
    )
    .unwrap();

    let nonce = 7u64;
    let leaf = WithdrawalLeaf {
        user: depositor.to_string(),
        token_id: token_id.clone(),
        balance: deposit_amount,
    };
    let sibling = WithdrawalLeaf {
        user: "other".to_string(),
        token_id: token_id.clone(),
        balance: Uint128::zero(),
    };
    let leaf_hash = hash_leaf(&leaf);
    let sibling_hash = hash_leaf(&sibling);
    let root_hash = hash_pair(&leaf_hash, &sibling_hash);

    app.execute_contract(
        router.clone(),
        orderbook_addr.clone(),
        &OrderbookExecuteMsg::ProposeRoot {
            root_id: root_id.clone(),
            root_hash: Binary::from(root_hash.to_vec()),
            per_asset_totals: vec![AssetTotal {
                token_id: token_id.clone(),
                amount: deposit_amount,
            }],
            da_hash: None,
            da_url: None,
        },
        &[],
    )
    .unwrap();

    let permit_data = PermitData {
        root_id: root_id.clone(),
        user: depositor.to_string(),
        token_id: token_id.clone(),
        amount: withdraw_amount,
        nonce,
        destination_chain_uid: chain_uid.as_str().to_string(),
        destination: destination.to_string(),
        expiry: app.block_info().time.plus_seconds(60).seconds(),
    };
    let permit = sign_permit(&signer_key, signer_address.clone(), permit_data);

    let bad_position_proof = vec![MerkleProofStep {
        hash: Binary::from(sibling_hash.to_vec()),
        position: ProofPosition::Left,
    }];
    let bad_position = app.execute_contract(
        depositor.clone(),
        orderbook_addr.clone(),
        &OrderbookExecuteMsg::Withdraw {
            root_id: root_id.clone(),
            amount: withdraw_amount,
            nonce,
            leaf: leaf.clone(),
            proof: bad_position_proof,
            permit,
            destination_chain_uid: chain_uid.as_str().to_string(),
            destination: destination.to_string(),
        },
        &[],
    );
    assert!(bad_position.is_err());

    let mut bad_hash = sibling_hash;
    bad_hash[0] ^= 0x01;
    let permit_data = PermitData {
        root_id: root_id.clone(),
        user: depositor.to_string(),
        token_id: token_id.clone(),
        amount: withdraw_amount,
        nonce,
        destination_chain_uid: chain_uid.as_str().to_string(),
        destination: destination.to_string(),
        expiry: app.block_info().time.plus_seconds(120).seconds(),
    };
    let permit = sign_permit(&signer_key, signer_address, permit_data);

    let bad_hash_proof = vec![MerkleProofStep {
        hash: Binary::from(bad_hash.to_vec()),
        position: ProofPosition::Right,
    }];
    let bad_hash = app.execute_contract(
        depositor,
        orderbook_addr,
        &OrderbookExecuteMsg::Withdraw {
            root_id,
            amount: withdraw_amount,
            nonce,
            leaf,
            proof: bad_hash_proof,
            permit,
            destination_chain_uid: chain_uid.as_str().to_string(),
            destination: destination.to_string(),
        },
        &[],
    );
    assert!(bad_hash.is_err());
}

fn sign_permit(
    signer_key: &SigningKey,
    signer_address: String,
    permit_data: PermitData,
) -> Permit {
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
