#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{to_json_binary, to_json_string, Binary, Uint128};
use cw_orch::prelude::CallAs;
use cw_orch::prelude::ContractInstance;
use cw_orch::prelude::CwOrchExecute;
use cw_orch::prelude::CwOrchInstantiate;
use cw_orch::prelude::CwOrchQuery;
use cw_orch::prelude::CwOrchUpload;
use cw_orch_interchain::core::InterchainEnv;
use cw_orch_interchain::prelude::IbcQueryHandler;
use euclid::msgs::orderbook_deposits::{
    AssetDepositResponse, AssetTotal, ExecuteMsg as OrderbookExecuteMsg,
    InstantiateMsg as OrderbookInstantiateMsg, MerkleProofStep, Permit, PermitData, ProofPosition,
    QueryMsg as OrderbookQueryMsg, StateResponse, UserDepositResponse,
    VirtualBalanceReceiveHookMsg, WhitelistListResponse, WithdrawalLeaf,
};
use euclid::msgs::router::QueryMsgFns;
use euclid::{
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    msgs::virtual_balance::{
        ExecuteMint, ExecuteMsg as VirtualBalanceExecuteMsg, GetBalanceResponse,
        QueryMsg as VirtualBalanceQueryMsg,
    },
    voucher::BalanceKey,
};
use k256::ecdsa::SigningKey;
use orderbook_deposits::OrderbookDepositsContract;
use relayer::verify::{MsgSignData, MsgSignDataMsg, MsgSignDataValue};
use sha2::{digest::Update, Digest, Sha256};

use crate::helpers::chains::get_virtual_balance;
use crate::helpers::chains::{setup_interchain, setup_router};
use crate::helpers::relayer::get_signer_key;
use crate::tests_reusable::constants::{FACTORY_CHAIN_ID_LOCAL, ROUTER_CHAIN_ID};

#[test]
fn deposit_and_query_flow() {
    let token_id = "token1".to_string();
    let deposit_amount = Uint128::new(500);
    let chain_uid = ChainUid::vsl_chain_uid().unwrap();

    let sender = "sender_for_all_chains";
    let interchain = setup_interchain(sender, FACTORY_CHAIN_ID_LOCAL);
    let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
    let depositor = router_chain.addr_make("depositor");
    let router = setup_router(&router_chain, vec![FACTORY_CHAIN_ID_LOCAL]).unwrap();
    let virtual_balance_address = router.get_state().unwrap().virtual_balance_address;
    let mut virutal_balance_contract = get_virtual_balance(&router_chain, &virtual_balance_address);

    let mut orderbook_deposits_contract = OrderbookDepositsContract::new(router_chain.clone());

    orderbook_deposits_contract.upload().unwrap();
    orderbook_deposits_contract
        .instantiate(
            &OrderbookInstantiateMsg {
                virtual_balance: virutal_balance_contract.address().unwrap().to_string(),
                admin: Some(router.address().unwrap().to_string()),
                root_challenge_period: None,
                permit_signer_pubkey: None,
                permit_signer_address: None,
                authorized_posters: Some(vec![router.address().unwrap().to_string()]),
            },
            None,
            &[],
        )
        .unwrap();
    let orderbook_addr = orderbook_deposits_contract.address().unwrap();

    // Whitelist token as admin
    orderbook_deposits_contract.set_sender(&router.address().unwrap());
    orderbook_deposits_contract
        .execute(
            &OrderbookExecuteMsg::SetWhitelist {
                token_id: token_id.clone(),
                whitelisted: true,
            },
            &[],
        )
        .unwrap();

    virutal_balance_contract.set_sender(&router.address().unwrap());
    virutal_balance_contract
        .execute(
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

    virutal_balance_contract.set_sender(&depositor);
    virutal_balance_contract
        .execute(
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
    let state: StateResponse = orderbook_deposits_contract
        .query(&OrderbookQueryMsg::State {})
        .unwrap();

    assert_eq!(state.admin, router.address().unwrap().to_string());
    assert_eq!(
        state.virtual_balance,
        virutal_balance_contract.address().unwrap().to_string()
    );
    assert_eq!(state.status, "active".to_string());

    // Verify aggregate deposit
    let asset_deposit: AssetDepositResponse = orderbook_deposits_contract
        .query(&OrderbookQueryMsg::AssetDeposit {
            token_id: token_id.clone(),
        })
        .unwrap();

    assert_eq!(asset_deposit.amount, deposit_amount);

    // Verify user deposit
    let user_deposit: UserDepositResponse = orderbook_deposits_contract
        .query(&OrderbookQueryMsg::UserDeposit {
            user: depositor.to_string(),
            token_id: token_id.clone(),
        })
        .unwrap();

    assert_eq!(user_deposit.amount, deposit_amount);

    // Verify whitelist listing
    let whitelisted: WhitelistListResponse = orderbook_deposits_contract
        .query(&OrderbookQueryMsg::WhitelistedAssets {
            start_after: None,
            limit: None,
        })
        .unwrap();

    assert!(whitelisted
        .assets
        .iter()
        .any(|w| w.token_id == token_id && w.whitelisted));
}

#[test]
fn withdraw_with_merkle_and_permit() {
    let sender = "sender_for_all_chains";
    let interchain = setup_interchain(sender, FACTORY_CHAIN_ID_LOCAL);
    let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
    let depositor = router_chain.addr_make("depositor");
    let destination = router_chain.addr_make("destination");
    let token_id = "token1".to_string();
    let deposit_amount = Uint128::new(500);
    let withdraw_amount = Uint128::new(300);
    let chain_uid = ChainUid::vsl_chain_uid().unwrap();
    let root_id = "root-1".to_string();

    let router = setup_router(&router_chain, vec![FACTORY_CHAIN_ID_LOCAL]).unwrap();
    let router_address = router.address().unwrap();
    let virtual_balance_address = router.get_state().unwrap().virtual_balance_address;
    let mut virutal_balance_contract = get_virtual_balance(&router_chain, &virtual_balance_address);
    let mut orderbook_deposits_contract = OrderbookDepositsContract::new(router_chain.clone());

    let (signer_key, signer_pubkey) = get_signer_key();
    let signer_address = format!("permit_{}", router_chain.chain_id());
    // Keep this high enough to avoid accidental expiry in tests, but below Timestamp nanosecond overflow.
    let permit_expiry = 4_102_444_800u64; // 2100-01-01T00:00:00Z

    orderbook_deposits_contract.upload().unwrap();
    orderbook_deposits_contract
        .instantiate(
            &OrderbookInstantiateMsg {
                virtual_balance: virutal_balance_contract.address().unwrap().to_string(),
                admin: Some(router_address.to_string()),
                root_challenge_period: Some(0),
                permit_signer_pubkey: Some(signer_pubkey.clone()),
                permit_signer_address: Some(signer_address.clone()),
                authorized_posters: Some(vec![router_address.to_string()]),
            },
            None,
            &[],
        )
        .unwrap();
    let orderbook_addr = orderbook_deposits_contract.address().unwrap();

    orderbook_deposits_contract.set_sender(&router_address);
    orderbook_deposits_contract
        .execute(
            &OrderbookExecuteMsg::SetWhitelist {
                token_id: token_id.clone(),
                whitelisted: true,
            },
            &[],
        )
        .unwrap();

    virutal_balance_contract.set_sender(&router_address);
    virutal_balance_contract
        .execute(
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
    virutal_balance_contract.set_sender(&depositor);
    virutal_balance_contract
        .execute(
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

    orderbook_deposits_contract.set_sender(&router_address);
    orderbook_deposits_contract
        .execute(
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
        expiry: permit_expiry,
    };
    let permit = sign_permit(&signer_key, signer_address.clone(), permit_data);

    let proof = vec![MerkleProofStep {
        hash: Binary::from(sibling_hash.to_vec()),
        position: ProofPosition::Right,
    }];

    orderbook_deposits_contract.set_sender(&depositor);
    orderbook_deposits_contract
        .execute(
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
    let asset_deposit: AssetDepositResponse = orderbook_deposits_contract
        .query(&OrderbookQueryMsg::AssetDeposit {
            token_id: token_id.clone(),
        })
        .unwrap();
    assert_eq!(
        asset_deposit.amount,
        deposit_amount.checked_sub(withdraw_amount).unwrap()
    );

    let destination_balance: GetBalanceResponse = virutal_balance_contract
        .query(&VirtualBalanceQueryMsg::GetBalance {
            balance_key: BalanceKey {
                cross_chain_user: CrossChainUser::new(chain_uid.clone(), destination.to_string()),
                token_id: token_id.clone(),
            },
        })
        .unwrap();
    assert_eq!(destination_balance.amount, withdraw_amount);

    let replay = orderbook_deposits_contract.execute(
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
    let sender = "sender_for_all_chains";
    let interchain = setup_interchain(sender, FACTORY_CHAIN_ID_LOCAL);
    let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
    let depositor = router_chain.addr_make("depositor");
    let destination = router_chain.addr_make("destination");
    let token_id = "token1".to_string();
    let deposit_amount = Uint128::new(500);
    let withdraw_amount = Uint128::new(200);
    let chain_uid = ChainUid::vsl_chain_uid().unwrap();
    let root_id = "root-invalid".to_string();

    let router = setup_router(&router_chain, vec![FACTORY_CHAIN_ID_LOCAL]).unwrap();
    let router_address = router.address().unwrap();
    let virtual_balance_address = router.get_state().unwrap().virtual_balance_address;
    let mut virutal_balance_contract = get_virtual_balance(&router_chain, &virtual_balance_address);
    let mut orderbook_deposits_contract = OrderbookDepositsContract::new(router_chain.clone());

    let (signer_key, signer_pubkey) = get_signer_key();
    let signer_address = format!("permit_{}", router_chain.chain_id());
    // Keep this high enough to avoid accidental expiry in tests, but below Timestamp nanosecond overflow.
    let permit_expiry = 4_102_444_800u64; // 2100-01-01T00:00:00Z

    orderbook_deposits_contract.upload().unwrap();
    orderbook_deposits_contract
        .instantiate(
            &OrderbookInstantiateMsg {
                virtual_balance: virutal_balance_contract.address().unwrap().to_string(),
                admin: Some(router_address.to_string()),
                root_challenge_period: Some(0),
                permit_signer_pubkey: Some(signer_pubkey.clone()),
                permit_signer_address: Some(signer_address.clone()),
                authorized_posters: Some(vec![router_address.to_string()]),
            },
            None,
            &[],
        )
        .unwrap();
    let orderbook_addr = orderbook_deposits_contract.address().unwrap();

    orderbook_deposits_contract.set_sender(&router_address);
    orderbook_deposits_contract
        .execute(
            &OrderbookExecuteMsg::SetWhitelist {
                token_id: token_id.clone(),
                whitelisted: true,
            },
            &[],
        )
        .unwrap();

    virutal_balance_contract.set_sender(&router_address);
    virutal_balance_contract
        .execute(
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
    virutal_balance_contract.set_sender(&depositor);
    virutal_balance_contract
        .execute(
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

    orderbook_deposits_contract.set_sender(&router_address);
    orderbook_deposits_contract
        .execute(
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
        expiry: permit_expiry,
    };
    let permit = sign_permit(&signer_key, signer_address.clone(), permit_data);

    let bad_position_proof = vec![MerkleProofStep {
        hash: Binary::from(sibling_hash.to_vec()),
        position: ProofPosition::Left,
    }];
    orderbook_deposits_contract.set_sender(&depositor);
    let bad_position = orderbook_deposits_contract.execute(
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
        expiry: permit_expiry,
    };
    let permit = sign_permit(&signer_key, signer_address, permit_data);

    let bad_hash_proof = vec![MerkleProofStep {
        hash: Binary::from(bad_hash.to_vec()),
        position: ProofPosition::Right,
    }];
    let bad_hash = orderbook_deposits_contract.execute(
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
