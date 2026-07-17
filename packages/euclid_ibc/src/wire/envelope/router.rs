use std::ops::Add;

use alloy_sol_types::private::Bytes;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{ensure, to_json_binary, Addr, Binary, DepsMut, Env, SubMsg, WasmMsg};
use euclid::{
    chain::{ChainType, ChainUid},
    cross_chain_user::CrossChainUser,
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
use crate::wire::msgs::{
    AddConcentratedLiquiditySendMsg, AddLiquiditySendMsg, CollectConcentratedFeesSendMsg,
    CollectConcentratedProtocolFeesSendMsg, DepositTokenSendMsg, DeregisterDenomSendMsg,
    RegisterDenomSendMsg, RemoveConcentratedLiquiditySendMsg, RemoveLiquiditySendMsg,
    RequestConcentratedPoolCreationSendMsg, RequestPoolCreationSendMsg,
    SingleSidedAddLiquiditySendMsg, SwapSendMsg, TransferVoucherSendMsg,
};

// Top-level envelope for every message the router receives, on the wire and
// internally. Tags are frozen in Rust declaration order from 0 and must never
// be reordered; the wire ABI codec is the standard `(uint8 tag, bytes payload)`
// shape, same as the old `euclid_encoding::abi::router` envelope this
// supersedes.
#[cw_serde]
pub enum RouterReceiveMsg {
    RegisterDenom(RegisterDenomSendMsg),
    DeregisterDenom(DeregisterDenomSendMsg),
    TransferVoucher(TransferVoucherSendMsg),
    DepositToken(DepositTokenSendMsg),
    RequestPoolCreation(RequestPoolCreationSendMsg),
    RequestConcentratedPoolCreation(RequestConcentratedPoolCreationSendMsg),
    AddLiquidity(AddLiquiditySendMsg),
    AddConcentratedLiquidity(AddConcentratedLiquiditySendMsg),
    RemoveLiquidity(RemoveLiquiditySendMsg),
    RemoveConcentratedLiquidity(RemoveConcentratedLiquiditySendMsg),
    CollectConcentratedFees(CollectConcentratedFeesSendMsg),
    CollectConcentratedProtocolFees(CollectConcentratedProtocolFeesSendMsg),
    Swap(SwapSendMsg),
    SingleSidedAddLiquidity(SingleSidedAddLiquiditySendMsg),
}

pub const TAG_REGISTER_DENOM: u8 = 0;
pub const TAG_DEREGISTER_DENOM: u8 = 1;
pub const TAG_TRANSFER_VOUCHER: u8 = 2;
pub const TAG_DEPOSIT_TOKEN: u8 = 3;
pub const TAG_REQUEST_POOL_CREATION: u8 = 4;
pub const TAG_REQUEST_CONCENTRATED_POOL_CREATION: u8 = 5;
pub const TAG_ADD_LIQUIDITY: u8 = 6;
pub const TAG_ADD_CONCENTRATED_LIQUIDITY: u8 = 7;
pub const TAG_REMOVE_LIQUIDITY: u8 = 8;
pub const TAG_REMOVE_CONCENTRATED_LIQUIDITY: u8 = 9;
pub const TAG_COLLECT_CONCENTRATED_FEES: u8 = 10;
pub const TAG_COLLECT_CONCENTRATED_PROTOCOL_FEES: u8 = 11;
pub const TAG_SWAP: u8 = 12;
pub const TAG_SINGLE_SIDED_ADD_LIQUIDITY: u8 = 13;

// No `vec_to_sol`/`vec_from_sol` helpers here: unlike the old per-variant
// modules, none of the fourteen `*SendMsg` structs need the envelope to map
// a bare `Vec<T: AbiMap>` on their behalf. Each struct's own `AbiMap` impl
// already handles its internal vec fields (e.g. `recipients`, `swaps`).

impl AbiMap for RouterReceiveMsg {
    type Sol = TaggedSol;

    fn type_name() -> &'static str {
        "RouterReceiveMsg"
    }

    fn to_sol(&self) -> Result<(u8, Bytes), EncodingError> {
        Ok(match self {
            RouterReceiveMsg::RegisterDenom(inner) => {
                tagged(TAG_REGISTER_DENOM, inner.to_abi_bytes()?)
            }
            RouterReceiveMsg::DeregisterDenom(inner) => {
                tagged(TAG_DEREGISTER_DENOM, inner.to_abi_bytes()?)
            }
            RouterReceiveMsg::TransferVoucher(inner) => {
                tagged(TAG_TRANSFER_VOUCHER, inner.to_abi_bytes()?)
            }
            RouterReceiveMsg::DepositToken(inner) => {
                tagged(TAG_DEPOSIT_TOKEN, inner.to_abi_bytes()?)
            }
            RouterReceiveMsg::RequestPoolCreation(inner) => {
                tagged(TAG_REQUEST_POOL_CREATION, inner.to_abi_bytes()?)
            }
            RouterReceiveMsg::RequestConcentratedPoolCreation(inner) => tagged(
                TAG_REQUEST_CONCENTRATED_POOL_CREATION,
                inner.to_abi_bytes()?,
            ),
            RouterReceiveMsg::AddLiquidity(inner) => {
                tagged(TAG_ADD_LIQUIDITY, inner.to_abi_bytes()?)
            }
            RouterReceiveMsg::AddConcentratedLiquidity(inner) => {
                tagged(TAG_ADD_CONCENTRATED_LIQUIDITY, inner.to_abi_bytes()?)
            }
            RouterReceiveMsg::RemoveLiquidity(inner) => {
                tagged(TAG_REMOVE_LIQUIDITY, inner.to_abi_bytes()?)
            }
            RouterReceiveMsg::RemoveConcentratedLiquidity(inner) => {
                tagged(TAG_REMOVE_CONCENTRATED_LIQUIDITY, inner.to_abi_bytes()?)
            }
            RouterReceiveMsg::CollectConcentratedFees(inner) => {
                tagged(TAG_COLLECT_CONCENTRATED_FEES, inner.to_abi_bytes()?)
            }
            RouterReceiveMsg::CollectConcentratedProtocolFees(inner) => tagged(
                TAG_COLLECT_CONCENTRATED_PROTOCOL_FEES,
                inner.to_abi_bytes()?,
            ),
            RouterReceiveMsg::Swap(inner) => tagged(TAG_SWAP, inner.to_abi_bytes()?),
            RouterReceiveMsg::SingleSidedAddLiquidity(inner) => {
                tagged(TAG_SINGLE_SIDED_ADD_LIQUIDITY, inner.to_abi_bytes()?)
            }
        })
    }

    fn from_sol((tag, data): (u8, Bytes)) -> Result<Self, EncodingError> {
        match tag {
            TAG_REGISTER_DENOM => Ok(RouterReceiveMsg::RegisterDenom(
                RegisterDenomSendMsg::from_abi_bytes(&data)?,
            )),
            TAG_DEREGISTER_DENOM => Ok(RouterReceiveMsg::DeregisterDenom(
                DeregisterDenomSendMsg::from_abi_bytes(&data)?,
            )),
            TAG_TRANSFER_VOUCHER => Ok(RouterReceiveMsg::TransferVoucher(
                TransferVoucherSendMsg::from_abi_bytes(&data)?,
            )),
            TAG_DEPOSIT_TOKEN => Ok(RouterReceiveMsg::DepositToken(
                DepositTokenSendMsg::from_abi_bytes(&data)?,
            )),
            TAG_REQUEST_POOL_CREATION => Ok(RouterReceiveMsg::RequestPoolCreation(
                RequestPoolCreationSendMsg::from_abi_bytes(&data)?,
            )),
            TAG_REQUEST_CONCENTRATED_POOL_CREATION => {
                Ok(RouterReceiveMsg::RequestConcentratedPoolCreation(
                    RequestConcentratedPoolCreationSendMsg::from_abi_bytes(&data)?,
                ))
            }
            TAG_ADD_LIQUIDITY => Ok(RouterReceiveMsg::AddLiquidity(
                AddLiquiditySendMsg::from_abi_bytes(&data)?,
            )),
            TAG_ADD_CONCENTRATED_LIQUIDITY => Ok(RouterReceiveMsg::AddConcentratedLiquidity(
                AddConcentratedLiquiditySendMsg::from_abi_bytes(&data)?,
            )),
            TAG_REMOVE_LIQUIDITY => Ok(RouterReceiveMsg::RemoveLiquidity(
                RemoveLiquiditySendMsg::from_abi_bytes(&data)?,
            )),
            TAG_REMOVE_CONCENTRATED_LIQUIDITY => Ok(RouterReceiveMsg::RemoveConcentratedLiquidity(
                RemoveConcentratedLiquiditySendMsg::from_abi_bytes(&data)?,
            )),
            TAG_COLLECT_CONCENTRATED_FEES => Ok(RouterReceiveMsg::CollectConcentratedFees(
                CollectConcentratedFeesSendMsg::from_abi_bytes(&data)?,
            )),
            TAG_COLLECT_CONCENTRATED_PROTOCOL_FEES => {
                Ok(RouterReceiveMsg::CollectConcentratedProtocolFees(
                    CollectConcentratedProtocolFeesSendMsg::from_abi_bytes(&data)?,
                ))
            }
            TAG_SWAP => Ok(RouterReceiveMsg::Swap(SwapSendMsg::from_abi_bytes(&data)?)),
            TAG_SINGLE_SIDED_ADD_LIQUIDITY => Ok(RouterReceiveMsg::SingleSidedAddLiquidity(
                SingleSidedAddLiquiditySendMsg::from_abi_bytes(&data)?,
            )),
            other => Err(EncodingError::UnknownDiscriminant {
                type_name: "RouterReceiveMsg",
                discriminant: other,
            }),
        }
    }
}

impl RouterReceiveMsg {
    /// Frozen envelope tag of the active variant (used to key ack transcodes).
    pub fn wire_tag(&self) -> u8 {
        match self {
            RouterReceiveMsg::RegisterDenom(_) => TAG_REGISTER_DENOM,
            RouterReceiveMsg::DeregisterDenom(_) => TAG_DEREGISTER_DENOM,
            RouterReceiveMsg::TransferVoucher(_) => TAG_TRANSFER_VOUCHER,
            RouterReceiveMsg::DepositToken(_) => TAG_DEPOSIT_TOKEN,
            RouterReceiveMsg::RequestPoolCreation(_) => TAG_REQUEST_POOL_CREATION,
            RouterReceiveMsg::RequestConcentratedPoolCreation(_) => {
                TAG_REQUEST_CONCENTRATED_POOL_CREATION
            }
            RouterReceiveMsg::AddLiquidity(_) => TAG_ADD_LIQUIDITY,
            RouterReceiveMsg::AddConcentratedLiquidity(_) => TAG_ADD_CONCENTRATED_LIQUIDITY,
            RouterReceiveMsg::RemoveLiquidity(_) => TAG_REMOVE_LIQUIDITY,
            RouterReceiveMsg::RemoveConcentratedLiquidity(_) => TAG_REMOVE_CONCENTRATED_LIQUIDITY,
            RouterReceiveMsg::CollectConcentratedFees(_) => TAG_COLLECT_CONCENTRATED_FEES,
            RouterReceiveMsg::CollectConcentratedProtocolFees(_) => {
                TAG_COLLECT_CONCENTRATED_PROTOCOL_FEES
            }
            RouterReceiveMsg::Swap(_) => TAG_SWAP,
            RouterReceiveMsg::SingleSidedAddLiquidity(_) => TAG_SINGLE_SIDED_ADD_LIQUIDITY,
        }
    }

    pub fn get_tx_id(&self) -> String {
        match self {
            Self::RegisterDenom(msg) => msg.tx_id.clone(),
            Self::DeregisterDenom(msg) => msg.tx_id.clone(),
            Self::DepositToken(msg) => msg.tx_id.clone(),
            Self::TransferVoucher(msg) => msg.tx_id.clone(),
            Self::RequestPoolCreation(msg) => msg.tx_id.clone(),
            Self::RequestConcentratedPoolCreation(msg) => msg.tx_id.clone(),
            Self::AddLiquidity(msg) => msg.tx_id.clone(),
            Self::AddConcentratedLiquidity(msg) => msg.tx_id.clone(),
            Self::RemoveLiquidity(msg) => msg.tx_id.clone(),
            Self::RemoveConcentratedLiquidity(msg) => msg.tx_id.clone(),
            Self::CollectConcentratedFees(msg) => msg.tx_id.clone(),
            Self::CollectConcentratedProtocolFees(msg) => msg.tx_id.clone(),
            Self::Swap(msg) => msg.tx_id.clone(),
            Self::SingleSidedAddLiquidity(msg) => msg.tx_id.clone(),
        }
    }

    /// Returns true if `self` is a pool-related variant currently owned by
    /// `pool_factory`. Used in two places that MUST stay in lockstep:
    ///   1. The inbound ack dispatcher on main factory, to decide whether to
    ///      forward an ack to `pool_factory::OnPoolAck`.
    ///   2. The outbound reply handler on main factory
    ///      (`on_pool_factory_delegate_reply`), to reject any non-pool packet
    ///      returned by `pool_factory` as defence in depth.
    ///
    /// Extended slice-by-slice as additional pool flows are delegated. Adding
    /// a new variant here without also retrofitting both sites will cause
    /// either an unrouted ack (false negative) or an unsendable packet
    /// (false positive); reviewers should confirm both sites match.
    pub fn is_pool_variant(&self) -> bool {
        matches!(
            self,
            Self::RequestPoolCreation(_)
                | Self::RequestConcentratedPoolCreation(_)
                | Self::AddLiquidity(_)
                | Self::RemoveLiquidity(_)
        )
    }

    /// Returns a reference to the sender CrossChainUser from any variant.
    pub fn get_sender(&self) -> &CrossChainUser {
        match self {
            Self::RegisterDenom(msg) => &msg.sender,
            Self::DeregisterDenom(msg) => &msg.sender,
            Self::DepositToken(msg) => &msg.sender,
            Self::TransferVoucher(msg) => &msg.sender,
            Self::RequestPoolCreation(msg) => &msg.sender,
            Self::AddLiquidity(msg) => &msg.sender,
            Self::RemoveLiquidity(msg) => &msg.sender,
            Self::Swap(msg) => &msg.sender,
            Self::RequestConcentratedPoolCreation(msg) => &msg.sender,
            Self::AddConcentratedLiquidity(msg) => &msg.sender,
            Self::RemoveConcentratedLiquidity(msg) => &msg.sender,
            Self::CollectConcentratedFees(msg) => &msg.sender,
            Self::CollectConcentratedProtocolFees(msg) => &msg.sender,
            Self::SingleSidedAddLiquidity(msg) => &msg.sender,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn to_msg(
        &self,
        deps: &mut DepsMut,
        env: &Env,
        router_contract: String,
        sender: Addr,
        chain_uid: ChainUid,
        chain_type: ChainType,
        timeout: Option<u64>,
        ack_response: Option<Binary>,
    ) -> Result<SubMsg, ContractError> {
        match chain_type {
            ChainType::Native {} => {
                let router_msg = router::execute::ExecuteMsg::NativeReceiveCallback {
                    msg: to_json_binary(self)?,
                    chain_uid: chain_uid.clone(),
                };
                // Advance the counter within the reserved range (2001–3000).
                // When it exceeds 3000, wrap back to 2001 for slot reuse.
                // The `ensure!` below is the hard guard: if the slot is still occupied
                // (i.e. all 1 000 slots are in-flight simultaneously), the call errors.
                let mut reply_id = NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_COUNT
                    .load(deps.storage)
                    .unwrap_or(NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_RANGE.0);

                if reply_id > NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_RANGE.1 {
                    reply_id = NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_RANGE.0;
                }

                ensure!(
                    !NATIVE_CROSS_CHAIN_ORIGINAL_MSG_REPLY_QUEUE.has(deps.storage, reply_id),
                    ContractError::new("Reply ID is already in use")
                );
                NATIVE_CROSS_CHAIN_ORIGINAL_MSG_REPLY_QUEUE.save(
                    deps.storage,
                    reply_id,
                    &PendingPacket {
                        chain_uid,
                        original_msg: to_json_binary(self)?,
                        ack_response,
                        encoding: 0,
                        // The native path replies in-process and never reaches
                        // the relayer ack byte check, so no wire bytes are
                        // committed here.
                        wire_msg: Binary::default(),
                    },
                )?;
                NATIVE_CROSS_CHAIN_PENDING_PACKET_SENDER.save(deps.storage, reply_id, &sender)?;
                NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_COUNT.save(deps.storage, &reply_id.add(1))?;

                // Return the reply message
                Ok(SubMsg::reply_always(
                    WasmMsg::Execute {
                        contract_addr: router_contract,
                        msg: to_json_binary(&router_msg)?,
                        funds: vec![],
                    },
                    reply_id,
                ))
            }
            ChainType::Cosmos(_) => {
                let factory_internal_msg = factory::msg::ExecuteMsg::SendPacket {
                    msg: to_json_binary(self)?,
                    timeout,
                    ack_response,
                    sender,
                };
                // Trigger a Send Packet execute call to the same contract
                Ok(SubMsg::new(WasmMsg::Execute {
                    contract_addr: env.contract.address.to_string(),
                    msg: to_json_binary(&factory_internal_msg)?,
                    funds: vec![],
                }))
            }
            _ => Err(ContractError::new(
                "Router only supported on cosmos type chain",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::envelope::AcknowledgementMsg;
    use crate::wire::msgs::AddLiquidityAckMsg;
    use cosmwasm_std::Uint256;
    use euclid::chain::ChainUid;
    use euclid::cross_chain_user::CrossChainUser;
    use euclid::limit::Limit;
    use euclid::recipient::Recipient;
    use euclid::swap::NextSwapPair;
    use euclid::token::{
        PairWithDenomAndAmount, Token, TokenType, TokenWithDenom, TokenWithDenomAndAmount,
    };

    fn chain(uid: &str) -> ChainUid {
        ChainUid::create(uid.to_string()).unwrap()
    }

    fn ccu(chain_uid: &str, addr: &str) -> CrossChainUser {
        CrossChainUser::new(chain(chain_uid), addr.to_string())
    }

    fn token(denom: &str) -> Token {
        Token::create(denom.to_string()).unwrap()
    }

    fn native_type(denom: &str) -> TokenType {
        TokenType::Native {
            denom: denom.to_string(),
            decimals: None,
        }
    }

    fn token_with_denom(denom: &str, native_denom: &str) -> TokenWithDenom {
        TokenWithDenom {
            token: token(denom),
            token_type: native_type(native_denom),
        }
    }

    fn pda() -> PairWithDenomAndAmount {
        PairWithDenomAndAmount {
            token_1: TokenWithDenomAndAmount {
                token: token("abc"),
                amount: Uint256::from(1_000u128),
                token_type: native_type("uabc"),
            },
            token_2: TokenWithDenomAndAmount {
                token: token("xyz"),
                amount: Uint256::from(2_000u128),
                token_type: native_type("uxyz"),
            },
        }
    }

    fn json_string<T: serde::Serialize>(value: &T) -> String {
        String::from_utf8(cosmwasm_std::to_json_vec(value).unwrap()).unwrap()
    }

    // JSON snapshots (spec decision 12.3.4): the serialized envelope forms are
    // pinned as full string literals so a future field edit cannot silently
    // break decoding of state stored before the amendment (reply queue
    // original_msg, native callback payloads, meta-tx call_data all carry this
    // JSON). Bootstrapped from the live serializer; do not hand-edit.

    const SNAPSHOT_ADD_LIQUIDITY: &str = r#"{"add_liquidity":{"sender":{"chain_uid":"chain1","address":"factory-addr"},"slippage_tolerance_bps":100,"pair":{"token_1":{"token":"abc","amount":"1000","token_type":{"native":{"denom":"uabc","decimals":null}}},"token_2":{"token":"xyz","amount":"2000","token_type":{"native":{"denom":"uxyz","decimals":null}}}},"tx_id":"tx-add-liq"}}"#;

    const SNAPSHOT_SWAP: &str = r#"{"swap":{"sender":{"chain_uid":"chain1","address":"factory-addr"},"asset_in":{"token":"abc","token_type":{"native":{"denom":"uabc","decimals":null}}},"amount_in":"1000","asset_out":"out","min_amount_out":"1","swaps":[{"token_in":"abc","token_out":"xyz","test_fail":null}],"recipients":[{"recipient":{"chain_uid":"chain2","address":"recipient-addr"},"amount":{"less_than_or_equal":"1000"},"denom":{"voucher":{}},"forwarding_message":null,"unsafe_refund_as_voucher":null}],"partner_fee_amount":"10","partner_fee_recipient":{"chain_uid":"chain1","address":"partner-addr"},"tx_id":"tx-swap"}}"#;

    const SNAPSHOT_ACK_OK: &str = r#"{"ok":{"mint_lp_tokens":"1000","vlp_address":"vlp1","tx_id":"tx-1","sender":{"chain_uid":"chain1","address":"sender-addr"}}}"#;

    const SNAPSHOT_ACK_ERROR: &str = r#"{"error":"boom"}"#;

    #[test]
    fn router_receive_msg_add_liquidity_json_snapshot() {
        let msg = RouterReceiveMsg::AddLiquidity(AddLiquiditySendMsg {
            sender: ccu("chain1", "factory-addr"),
            slippage_tolerance_bps: 100,
            pair: pda(),
            tx_id: "tx-add-liq".to_string(),
        });
        assert_eq!(json_string(&msg), SNAPSHOT_ADD_LIQUIDITY);
    }

    #[test]
    fn router_receive_msg_swap_json_snapshot() {
        let msg = RouterReceiveMsg::Swap(SwapSendMsg {
            sender: ccu("chain1", "factory-addr"),
            asset_in: token_with_denom("abc", "uabc"),
            amount_in: Uint256::from(1_000u128),
            asset_out: token("out"),
            min_amount_out: Uint256::from(1u128),
            swaps: vec![NextSwapPair {
                token_in: token("abc"),
                token_out: token("xyz"),
                pool_key: None,
                test_fail: None,
            }],
            recipients: vec![Recipient {
                recipient: ccu("chain2", "recipient-addr"),
                amount: Limit::LessThanOrEqual(Uint256::from(1_000u128)),
                denom: TokenType::Voucher {},
                forwarding_message: None,
                unsafe_refund_as_voucher: None,
            }],
            partner_fee_amount: Uint256::from(10u128),
            partner_fee_recipient: ccu("chain1", "partner-addr"),
            tx_id: "tx-swap".to_string(),
        });
        assert_eq!(json_string(&msg), SNAPSHOT_SWAP);
    }

    #[test]
    fn acknowledgement_msg_ok_json_snapshot() {
        let ack = AcknowledgementMsg::Ok(AddLiquidityAckMsg {
            mint_lp_tokens: Uint256::from(1_000u128),
            vlp_address: "vlp1".to_string(),
            tx_id: "tx-1".to_string(),
            sender: ccu("chain1", "sender-addr"),
        });
        assert_eq!(json_string(&ack), SNAPSHOT_ACK_OK);
    }

    #[test]
    fn acknowledgement_msg_error_json_snapshot() {
        let ack: AcknowledgementMsg<AddLiquidityAckMsg> =
            AcknowledgementMsg::Error("boom".to_string());
        assert_eq!(json_string(&ack), SNAPSHOT_ACK_ERROR);
    }

    #[test]
    fn get_tx_id_and_sender_read_the_payload() {
        let msg = RouterReceiveMsg::AddLiquidity(AddLiquiditySendMsg {
            sender: ccu("chain1", "factory-addr"),
            slippage_tolerance_bps: 100,
            pair: pda(),
            tx_id: "tx-add-liq".to_string(),
        });
        assert_eq!(msg.get_tx_id(), "tx-add-liq");
        assert_eq!(msg.get_sender(), &ccu("chain1", "factory-addr"));
        assert!(msg.is_pool_variant());
    }

    #[test]
    fn abi_roundtrip_inline_and_struct_variants() {
        let register = RouterReceiveMsg::RegisterDenom(RegisterDenomSendMsg {
            sender: ccu("chain1", "a"),
            tx_id: "t0".to_string(),
            token: token_with_denom("abc", "uabc"),
        });
        let deregister = RouterReceiveMsg::DeregisterDenom(DeregisterDenomSendMsg {
            sender: ccu("chain1", "a"),
            tx_id: "t1".to_string(),
            token: token_with_denom("abc", "uabc"),
        });
        let add_liquidity = RouterReceiveMsg::AddLiquidity(AddLiquiditySendMsg {
            sender: ccu("chain1", "a"),
            slippage_tolerance_bps: 100,
            pair: pda(),
            tx_id: "t6".to_string(),
        });

        for sample in [register, deregister, add_liquidity] {
            let bytes = sample.to_abi_bytes().unwrap();
            let decoded = RouterReceiveMsg::from_abi_bytes(&bytes).unwrap();
            assert_eq!(sample, decoded);
        }
    }

    #[test]
    fn unknown_discriminant_is_rejected() {
        let err = RouterReceiveMsg::from_sol((14u8, Bytes::new())).unwrap_err();
        assert_eq!(
            err,
            EncodingError::UnknownDiscriminant {
                type_name: "RouterReceiveMsg",
                discriminant: 14,
            }
        );
    }
}
