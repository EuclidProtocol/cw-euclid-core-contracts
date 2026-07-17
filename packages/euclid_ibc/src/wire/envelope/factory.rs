use std::ops::Add;

use alloy_sol_types::private::Bytes;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{ensure, to_json_binary, Addr, Binary, DepsMut, Env, SubMsg, WasmMsg};
use euclid::{
    chain::Chain,
    error::ContractError,
    msgs::{factory, router},
};
use euclid_encoding::abi::tagged::{tagged, TaggedSol};
use euclid_encoding::{AbiDecode, AbiEncode, AbiMap, EncodingError};

use crate::state::{
    PendingPacket, NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_COUNT,
    NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_RANGE, NATIVE_CROSS_CHAIN_ORIGINAL_MSG_REPLY_QUEUE,
    NATIVE_CROSS_CHAIN_PENDING_PACKET_SENDER,
};
use crate::wire::msgs::{RegisterFactorySendMsg, ReleaseEscrowSendMsg};

// Top-level envelope for every message a factory receives, on the wire and
// internally. Tags are frozen in Rust declaration order from 0 and must never
// be reordered; the wire ABI codec is the standard `(uint8 tag, bytes payload)`
// shape, same as the old `euclid_encoding::abi::factory` envelope this
// supersedes.
#[cw_serde]
pub enum FactoryReceiveMsg {
    RegisterFactory(RegisterFactorySendMsg),
    ReleaseEscrow(ReleaseEscrowSendMsg),
}

pub const TAG_REGISTER_FACTORY: u8 = 0;
pub const TAG_RELEASE_ESCROW: u8 = 1;

impl AbiMap for FactoryReceiveMsg {
    type Sol = TaggedSol;

    fn type_name() -> &'static str {
        "FactoryReceiveMsg"
    }

    fn to_sol(&self) -> Result<(u8, Bytes), EncodingError> {
        Ok(match self {
            FactoryReceiveMsg::RegisterFactory(inner) => {
                tagged(TAG_REGISTER_FACTORY, inner.to_abi_bytes()?)
            }
            FactoryReceiveMsg::ReleaseEscrow(inner) => {
                tagged(TAG_RELEASE_ESCROW, inner.to_abi_bytes()?)
            }
        })
    }

    fn from_sol((tag, data): (u8, Bytes)) -> Result<Self, EncodingError> {
        match tag {
            TAG_REGISTER_FACTORY => Ok(FactoryReceiveMsg::RegisterFactory(
                RegisterFactorySendMsg::from_abi_bytes(&data)?,
            )),
            TAG_RELEASE_ESCROW => Ok(FactoryReceiveMsg::ReleaseEscrow(
                ReleaseEscrowSendMsg::from_abi_bytes(&data)?,
            )),
            other => Err(EncodingError::UnknownDiscriminant {
                type_name: "FactoryReceiveMsg",
                discriminant: other,
            }),
        }
    }
}

impl FactoryReceiveMsg {
    /// Frozen envelope tag of the active variant (used to key ack transcodes).
    pub fn wire_tag(&self) -> u8 {
        match self {
            FactoryReceiveMsg::RegisterFactory(_) => TAG_REGISTER_FACTORY,
            FactoryReceiveMsg::ReleaseEscrow(_) => TAG_RELEASE_ESCROW,
        }
    }

    pub fn get_tx_id(&self) -> String {
        match self {
            Self::RegisterFactory(msg) => msg.tx_id.clone(),
            Self::ReleaseEscrow(msg) => msg.tx_id.clone(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn to_msg(
        &self,
        deps: &mut DepsMut,
        env: &Env,
        sender: String,
        chain: Chain,
        timeout: Option<u64>,
        ack_response: Option<Binary>,
    ) -> Result<SubMsg, ContractError> {
        match chain.chain_type {
            euclid::chain::ChainType::Native {} => {
                let factory_msg = factory::ExecuteMsg::NativeReceiveCallback {
                    msg: to_json_binary(self)?,
                };
                // Clamp the counter to the reserved range (2001–3000); equivalent to
                // the wrap-around in RouterReceiveMsg::to_msg but expressed as a clamp.
                // The `ensure!` below is the hard guard: if the slot is still occupied
                // (i.e. all 1 000 slots are in-flight simultaneously), the call errors.
                let mut count = NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_COUNT
                    .load(deps.storage)
                    .unwrap_or(NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_RANGE.0);

                count = count
                    .min(NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_RANGE.1)
                    .max(NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_RANGE.0);

                ensure!(
                    !NATIVE_CROSS_CHAIN_ORIGINAL_MSG_REPLY_QUEUE.has(deps.storage, count),
                    ContractError::new("Msg Queue is full")
                );
                NATIVE_CROSS_CHAIN_ORIGINAL_MSG_REPLY_QUEUE.save(
                    deps.storage,
                    count,
                    &PendingPacket {
                        chain_uid: chain.chain_uid.clone(),
                        original_msg: to_json_binary(self)?,
                        ack_response,
                        encoding: 0,
                        // The native path replies in-process and never reaches
                        // the relayer ack byte check, so no wire bytes are
                        // committed here.
                        wire_msg: Binary::default(),
                    },
                )?;
                NATIVE_CROSS_CHAIN_PENDING_PACKET_SENDER.save(
                    deps.storage,
                    count,
                    &Addr::unchecked(sender),
                )?;

                NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_COUNT.save(deps.storage, &count.add(1))?;

                Ok(SubMsg::reply_always(
                    WasmMsg::Execute {
                        contract_addr: chain.factory_address.clone(),
                        msg: to_json_binary(&factory_msg)?,
                        funds: vec![],
                    },
                    count,
                ))
            }
            _ => {
                let router_internal_msg = router::execute::ExecuteMsg::SendPacket {
                    msg: to_json_binary(self)?,
                    chain,
                    timeout,
                    ack_response,
                    sender,
                };
                // Trigger a Send Packet execute call to the same contract
                Ok(SubMsg::new(WasmMsg::Execute {
                    contract_addr: env.contract.address.to_string(),
                    msg: to_json_binary(&router_internal_msg)?,
                    funds: vec![],
                }))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::Uint256;
    use euclid::chain::ChainUid;
    use euclid::cross_chain_user::CrossChainUser;
    use euclid::msgs::router::execute::{RegisterFactoryChainEvm, RegisterFactoryChainType};
    use euclid::token::{Token, TokenType};

    fn chain(uid: &str) -> ChainUid {
        ChainUid::create(uid.to_string()).unwrap()
    }

    fn ccu(chain_uid: &str, addr: &str) -> CrossChainUser {
        CrossChainUser::new(chain(chain_uid), addr.to_string())
    }

    fn token(denom: &str) -> Token {
        Token::create(denom.to_string()).unwrap()
    }

    fn json_string<T: serde::Serialize>(value: &T) -> String {
        String::from_utf8(cosmwasm_std::to_json_vec(value).unwrap()).unwrap()
    }

    // JSON snapshots (spec decision 12.3.4): the serialized envelope forms are
    // pinned as full string literals so a future field edit cannot silently
    // break decoding of state stored before the amendment (reply queue
    // original_msg and native callback payloads carry this JSON).
    // Bootstrapped from the live serializer; do not hand-edit.

    const SNAPSHOT_REGISTER_FACTORY: &str = r#"{"register_factory":{"chain_uid":"chain1","chain_type":{"evm":{"factory_address":"0xabc","factory_chain_id":"1"}},"tx_id":"tx-register"}}"#;

    const SNAPSHOT_RELEASE_ESCROW: &str = r#"{"release_escrow":{"sender":{"chain_uid":"chain1","address":"factory-addr"},"token":"abc","recipient":"recipient-addr","amount":"1000","denom":{"native":{"denom":"uabc","decimals":null}},"forwarding_message":"fwd","tx_id":"tx-release"}}"#;

    #[test]
    fn factory_receive_msg_register_factory_json_snapshot() {
        let msg = FactoryReceiveMsg::RegisterFactory(RegisterFactorySendMsg {
            chain_uid: chain("chain1"),
            chain_type: RegisterFactoryChainType::Evm(RegisterFactoryChainEvm {
                factory_address: "0xabc".to_string(),
                factory_chain_id: "1".to_string(),
            }),
            tx_id: "tx-register".to_string(),
        });
        assert_eq!(json_string(&msg), SNAPSHOT_REGISTER_FACTORY);
    }

    #[test]
    fn factory_receive_msg_release_escrow_json_snapshot() {
        let msg = FactoryReceiveMsg::ReleaseEscrow(ReleaseEscrowSendMsg {
            sender: ccu("chain1", "factory-addr"),
            token: token("abc"),
            recipient: "recipient-addr".to_string(),
            amount: Uint256::from(1_000u128),
            denom: TokenType::Native {
                denom: "uabc".to_string(),
                decimals: None,
            },
            forwarding_message: Some("fwd".to_string()),
            tx_id: "tx-release".to_string(),
        });
        assert_eq!(json_string(&msg), SNAPSHOT_RELEASE_ESCROW);
    }

    #[test]
    fn get_tx_id_reads_the_payload() {
        let msg = FactoryReceiveMsg::RegisterFactory(RegisterFactorySendMsg {
            chain_uid: chain("chain1"),
            chain_type: RegisterFactoryChainType::Evm(RegisterFactoryChainEvm {
                factory_address: "0xabc".to_string(),
                factory_chain_id: "1".to_string(),
            }),
            tx_id: "t0".to_string(),
        });
        assert_eq!(msg.get_tx_id(), "t0");
    }

    #[test]
    fn abi_roundtrip_both_variants() {
        let register = FactoryReceiveMsg::RegisterFactory(RegisterFactorySendMsg {
            chain_uid: chain("chain1"),
            chain_type: RegisterFactoryChainType::Evm(RegisterFactoryChainEvm {
                factory_address: "0xabc".to_string(),
                factory_chain_id: "1".to_string(),
            }),
            tx_id: "t0".to_string(),
        });
        let release = FactoryReceiveMsg::ReleaseEscrow(ReleaseEscrowSendMsg {
            sender: ccu("chain1", "a"),
            token: token("abc"),
            recipient: "recipient-addr".to_string(),
            amount: Uint256::from(1_000u128),
            denom: TokenType::Voucher {},
            forwarding_message: None,
            tx_id: "t1".to_string(),
        });

        for sample in [register, release] {
            let bytes = sample.to_abi_bytes().unwrap();
            let decoded = FactoryReceiveMsg::from_abi_bytes(&bytes).unwrap();
            assert_eq!(sample, decoded);
        }
    }

    #[test]
    fn unknown_discriminant_is_rejected() {
        let err = FactoryReceiveMsg::from_sol((2u8, Bytes::new())).unwrap_err();
        assert_eq!(
            err,
            EncodingError::UnknownDiscriminant {
                type_name: "FactoryReceiveMsg",
                discriminant: 2,
            }
        );
    }
}
