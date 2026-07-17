//! Sample builders for `FactoryReceiveMsg` (plan §6.3): both
//! variants, all four `RegisterFactoryChainType` arms, and both
//! `forwarding_message` shapes for `ReleaseEscrow`.
//!
//! `tests/common` is one shared module tree compiled fresh into every
//! integration-test binary; a helper used by one binary and not another is
//! legitimately "unused" from that binary's point of view, so dead-code is
//! allowed at the module level rather than per function.
#![allow(dead_code)]

use cosmwasm_std::Uint256;
use euclid::chain::ChainUid;
use euclid::cross_chain_user::CrossChainUser;
use euclid::msgs::router::execute::{
    RegisterFactoryChainCosmos, RegisterFactoryChainEvm, RegisterFactoryChainNative,
    RegisterFactoryChainTvm, RegisterFactoryChainType,
};
use euclid::token::{Token, TokenType};
use euclid_ibc::wire::envelope::factory::FactoryReceiveMsg;
use euclid_ibc::wire::msgs::{RegisterFactorySendMsg, ReleaseEscrowSendMsg};

fn chain_uid(uid: &str) -> ChainUid {
    ChainUid::create(uid.to_string()).unwrap()
}

fn token(id: &str) -> Token {
    Token::create(id.to_string()).unwrap()
}

pub fn register_factory_chain_type_samples() -> Vec<RegisterFactoryChainType> {
    vec![
        RegisterFactoryChainType::Native(RegisterFactoryChainNative {
            factory_address: "native1factory".to_string(),
            factory_chain_id: "native".to_string(),
        }),
        RegisterFactoryChainType::Cosmos(RegisterFactoryChainCosmos {
            factory_address: "cosmos1factory".to_string(),
            factory_chain_id: "cosmoshub-4".to_string(),
        }),
        RegisterFactoryChainType::Evm(RegisterFactoryChainEvm {
            factory_address: "0xfactory".to_string(),
            factory_chain_id: "1".to_string(),
        }),
        RegisterFactoryChainType::Tvm(RegisterFactoryChainTvm {
            factory_address: "Tfactory".to_string(),
            factory_chain_id: "728126428".to_string(),
        }),
    ]
}

/// One `RegisterFactory` message per `RegisterFactoryChainType` arm.
pub fn register_factory_samples() -> Vec<FactoryReceiveMsg> {
    register_factory_chain_type_samples()
        .into_iter()
        .enumerate()
        .map(|(i, chain_type)| {
            FactoryReceiveMsg::RegisterFactory(RegisterFactorySendMsg {
                chain_uid: chain_uid(&format!("chain{i}")),
                chain_type,
                tx_id: format!("tx-register-{i}"),
            })
        })
        .collect()
}

/// `ReleaseEscrow` messages covering both `forwarding_message` shapes and a
/// max-value `Uint256` amount.
pub fn release_escrow_samples() -> Vec<FactoryReceiveMsg> {
    vec![
        FactoryReceiveMsg::ReleaseEscrow(ReleaseEscrowSendMsg {
            sender: CrossChainUser::new(chain_uid("cosmos"), "cosmos1abc".to_string()),
            token: token("abc"),
            recipient: "cosmos1recipient".to_string(),
            amount: Uint256::MAX,
            denom: TokenType::Native {
                denom: "uatom".to_string(),
                decimals: Some(6),
            },
            forwarding_message: Some("do-something".to_string()),
            tx_id: "tx-release-1".to_string(),
        }),
        FactoryReceiveMsg::ReleaseEscrow(ReleaseEscrowSendMsg {
            sender: CrossChainUser::new(chain_uid("evm"), "0xsender".to_string()),
            token: token("def"),
            recipient: "0xrecipient".to_string(),
            amount: Uint256::zero(),
            denom: TokenType::Voucher {},
            forwarding_message: None,
            tx_id: "tx-release-2".to_string(),
        }),
    ]
}

/// Both variants together, for whole-family table-driven coverage.
pub fn all_samples() -> Vec<FactoryReceiveMsg> {
    let mut samples = register_factory_samples();
    samples.extend(release_escrow_samples());
    samples
}
