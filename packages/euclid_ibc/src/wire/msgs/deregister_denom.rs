use alloy_sol_types::sol_data::String as SolString;
use alloy_sol_types::SolType;
use cosmwasm_schema::cw_serde;
use euclid::cross_chain_user::CrossChainUser;
use euclid::msgs::vlp::base::DeregisterDenomResponse;
use euclid::token::TokenWithDenom;

use crate::wire::types::cross_chain_user::{
    cross_chain_user_from_sol, cross_chain_user_to_sol, CrossChainUserSol,
};
use crate::wire::types::token_with_denom::{
    token_with_denom_from_sol, token_with_denom_to_sol, TokenWithDenomSol,
};
use euclid_encoding::{AbiDecode, AbiEncode, AbiMap, EncodingError};

const TYPE_NAME: &str = "DeregisterDenomAckMsg";

#[cw_serde]
pub struct DeregisterDenomSendMsg {
    pub sender: CrossChainUser,
    pub tx_id: String,
    pub token: TokenWithDenom,
}

#[cw_serde]
pub struct DeregisterDenomAckMsg {}

impl AbiMap for DeregisterDenomSendMsg {
    type Sol = (CrossChainUserSol, SolString, TokenWithDenomSol);

    fn type_name() -> &'static str {
        "DeregisterDenomSendMsg"
    }

    fn to_sol(&self) -> Result<<Self::Sol as SolType>::RustType, EncodingError> {
        Ok((
            cross_chain_user_to_sol(&self.sender)?,
            self.tx_id.clone(),
            token_with_denom_to_sol(&self.token)?,
        ))
    }

    fn from_sol(sol: <Self::Sol as SolType>::RustType) -> Result<Self, EncodingError> {
        Ok(Self {
            sender: cross_chain_user_from_sol(sol.0)?,
            tx_id: sol.1,
            token: token_with_denom_from_sol(sol.2)?,
        })
    }
}

impl From<DeregisterDenomResponse> for DeregisterDenomAckMsg {
    fn from(_: DeregisterDenomResponse) -> Self {
        Self {}
    }
}

impl From<DeregisterDenomAckMsg> for DeregisterDenomResponse {
    fn from(_: DeregisterDenomAckMsg) -> Self {
        Self {}
    }
}

/// See `register_denom.rs` for why this is hand-rolled rather than routed
/// through `AbiMap`/`Sol = ()`.
impl AbiEncode for DeregisterDenomAckMsg {
    fn to_abi_bytes(&self) -> Result<Vec<u8>, EncodingError> {
        Ok(Vec::new())
    }
}

impl AbiDecode for DeregisterDenomAckMsg {
    fn from_abi_bytes(bytes: &[u8]) -> Result<Self, EncodingError> {
        if bytes.is_empty() {
            Ok(DeregisterDenomAckMsg {})
        } else {
            Err(EncodingError::NonEmptyPayload {
                type_name: TYPE_NAME,
                len: bytes.len(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use euclid::chain::ChainUid;
    use euclid::token::TokenType;
    use euclid_encoding::{decode, encode, Encoding};

    fn ccu() -> CrossChainUser {
        CrossChainUser::new(
            ChainUid::create("chain1".to_string()).unwrap(),
            "addr1".to_string(),
        )
    }

    fn token_with_denom() -> TokenWithDenom {
        TokenWithDenom {
            token: euclid::token::Token::create("abc".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "uabc".to_string(),
                decimals: None,
            },
        }
    }

    fn send() -> DeregisterDenomSendMsg {
        DeregisterDenomSendMsg {
            sender: ccu(),
            tx_id: "tx-deregister".to_string(),
            token: token_with_denom(),
        }
    }

    #[test]
    fn send_json_roundtrips() {
        let msg = send();
        let bytes = encode(&msg, Encoding::Json).unwrap();
        let decoded: DeregisterDenomSendMsg = decode(&bytes, Encoding::Json).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn send_abi_roundtrips() {
        let msg = send();
        let bytes = encode(&msg, Encoding::Abi).unwrap();
        let decoded: DeregisterDenomSendMsg = decode(&bytes, Encoding::Abi).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn ack_json_roundtrips() {
        let ack = DeregisterDenomAckMsg {};
        let bytes = encode(&ack, Encoding::Json).unwrap();
        let decoded: DeregisterDenomAckMsg = decode(&bytes, Encoding::Json).unwrap();
        assert_eq!(decoded, ack);
    }

    #[test]
    fn ack_abi_roundtrips() {
        let ack = DeregisterDenomAckMsg {};
        let bytes = encode(&ack, Encoding::Abi).unwrap();
        let decoded: DeregisterDenomAckMsg = decode(&bytes, Encoding::Abi).unwrap();
        assert_eq!(decoded, ack);
    }

    #[test]
    fn ack_from_conversion_roundtrips() {
        let domain = DeregisterDenomResponse {};
        let wire = DeregisterDenomAckMsg::from(domain.clone());
        assert_eq!(DeregisterDenomResponse::from(wire.clone()), domain);
        assert_eq!(
            DeregisterDenomAckMsg::from(DeregisterDenomResponse::from(wire)),
            DeregisterDenomAckMsg {}
        );
    }

    /// §6.5/§7.6: the empty ack mirror ABI-encodes to zero bytes, decodes from
    /// zero bytes, and rejects any nonempty payload on decode. Ported from the
    /// old `roundtrip_acks` integration case.
    #[test]
    fn deregister_denom_ack_zero_bytes_roundtrip_and_rejects_nonempty() {
        let encoded = DeregisterDenomAckMsg {}
            .to_abi_bytes()
            .expect("encode empty ack");
        assert!(encoded.is_empty());

        assert_eq!(
            DeregisterDenomAckMsg::from_abi_bytes(&[]).unwrap(),
            DeregisterDenomAckMsg {}
        );

        let err = DeregisterDenomAckMsg::from_abi_bytes(&[9]).unwrap_err();
        assert_eq!(
            err,
            EncodingError::NonEmptyPayload {
                type_name: "DeregisterDenomAckMsg",
                len: 1
            }
        );
    }
}
