//! The acknowledgement envelope type `AcknowledgementMsg<S>` and its ABI
//! codec. Mirrors `euclid::msgs::*::*Response` domain acks with wire structs
//! carrying an `AbiMap` (or, for the two empty acks, a hand-rolled
//! `AbiEncode`/`AbiDecode` pair) plus `From` conversions in both directions;
//! those payload structs live alongside their sends in `crate::wire::msgs`.

use alloy_sol_types::private::Bytes;
use alloy_sol_types::sol_data::String as SolString;
use alloy_sol_types::SolType;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{to_json_binary, Binary};
use euclid::error::ContractError;

use euclid_encoding::abi::params::decode_params_canonical;
use euclid_encoding::abi::tagged::{tagged, TaggedSol};
use euclid_encoding::{AbiDecode, AbiEncode, AbiMap, EncodingError};

/// A custom acknowledgement type.
/// The success type `T` depends on the PacketMsg variant.
///
/// This could be refactored to use [StdAck] at some point. However,
/// it has a different success variant name ("ok" vs. "result") and
/// a JSON payload instead of a binary payload.
///
/// [StdAck]: https://github.com/CosmWasm/cosmwasm/issues/1512
#[cw_serde]
pub enum AcknowledgementMsg<S> {
    Ok(S),
    Error(String),
}

pub fn make_ack_fail(err: String) -> Result<Binary, ContractError> {
    let res = AcknowledgementMsg::Error::<()>(err);
    Ok(to_json_binary(&res)?)
}

const TAG_OK: u8 = 0;
const TAG_ERROR: u8 = 1;

type ErrorSol = (SolString,);

/// The envelope is bound on the public `AbiEncode`/`AbiDecode` traits, not on
/// the internal `AbiMap`, so payloads that hand-roll those traits directly
/// (the two empty responses, `register_denom.rs` and `deregister_denom.rs`)
/// still satisfy `S` here. `AcknowledgementMsg` is defined above in this
/// module, and `AbiMap` is defined in `euclid_encoding`, so this is a legal
/// (foreign-trait, local-type) combination for `euclid_ibc` under the orphan
/// rule.
impl<S> AbiMap for AcknowledgementMsg<S>
where
    S: AbiEncode + AbiDecode,
{
    type Sol = TaggedSol;

    fn type_name() -> &'static str {
        "AcknowledgementMsg"
    }

    fn to_sol(&self) -> Result<(u8, Bytes), EncodingError> {
        Ok(match self {
            AcknowledgementMsg::Ok(payload) => tagged(TAG_OK, payload.to_abi_bytes()?),
            AcknowledgementMsg::Error(err) => {
                tagged(TAG_ERROR, <ErrorSol>::abi_encode_params(&(err.clone(),)))
            }
        })
    }

    fn from_sol((tag, data): (u8, Bytes)) -> Result<Self, EncodingError> {
        match tag {
            TAG_OK => Ok(AcknowledgementMsg::Ok(S::from_abi_bytes(&data)?)),
            TAG_ERROR => {
                let (err,) = decode_params_canonical::<ErrorSol>("AcknowledgementMsg", &data)?;
                Ok(AcknowledgementMsg::Error(err))
            }
            other => Err(EncodingError::UnknownDiscriminant {
                type_name: "AcknowledgementMsg",
                discriminant: other,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::msgs::add_liquidity::AddLiquidityAckMsg;
    use cosmwasm_std::{to_json_vec, Uint256};
    use euclid::chain::ChainUid;
    use euclid::cross_chain_user::CrossChainUser;
    use euclid::liquidity::AddLiquidityResponse;
    use euclid_encoding::{decode, encode, Encoding};

    fn sender() -> CrossChainUser {
        CrossChainUser::new(
            ChainUid::create("chain1".to_string()).unwrap(),
            "addr1".to_string(),
        )
    }

    // JSON byte parity: the domain-typed envelope and the wire-typed
    // envelope must serialize identically, since `AddLiquidityAckMsg`'s
    // fields mirror `AddLiquidityResponse` exactly in name and order.
    #[test]
    fn ok_ack_json_bytes_match_domain_and_wire() {
        let domain = AddLiquidityResponse {
            mint_lp_tokens: Uint256::from(1_000u128),
            vlp_address: "vlp1".to_string(),
            tx_id: "tx-1".to_string(),
            sender: sender(),
        };
        let wire = AddLiquidityAckMsg::from(domain.clone());

        let domain_ack = AcknowledgementMsg::<AddLiquidityResponse>::Ok(domain);
        let wire_ack = AcknowledgementMsg::<AddLiquidityAckMsg>::Ok(wire);

        assert_eq!(
            to_json_vec(&domain_ack).unwrap(),
            to_json_vec(&wire_ack).unwrap()
        );
    }

    // ABI roundtrip through the crate's single dispatch point. Only the wire
    // type can go through this path: the envelope's `AbiMap` impl requires
    // `S: AbiEncode + AbiDecode`, which the domain response type does not
    // implement (only wire mirrors do).
    #[test]
    fn ok_ack_abi_roundtrips_through_encode_decode() {
        let domain = AddLiquidityResponse {
            mint_lp_tokens: Uint256::MAX,
            vlp_address: "vlp1".to_string(),
            tx_id: "tx-1".to_string(),
            sender: sender(),
        };
        let wire_ack =
            AcknowledgementMsg::<AddLiquidityAckMsg>::Ok(AddLiquidityAckMsg::from(domain));

        let encoded = encode(&wire_ack, Encoding::Abi).unwrap();
        let decoded: AcknowledgementMsg<AddLiquidityAckMsg> =
            decode(&encoded, Encoding::Abi).unwrap();
        assert_eq!(decoded, wire_ack);
    }

    #[test]
    fn error_ack_abi_roundtrips() {
        let err_ack = AcknowledgementMsg::<AddLiquidityAckMsg>::Error("boom".to_string());
        let encoded = err_ack.to_abi_bytes().unwrap();
        let decoded = AcknowledgementMsg::<AddLiquidityAckMsg>::from_abi_bytes(&encoded).unwrap();
        assert_eq!(decoded, err_ack);
    }
}
