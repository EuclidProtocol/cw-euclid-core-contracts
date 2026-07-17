use alloy_sol_types::sol_data::{String as SolString, Uint as SolUint};
use alloy_sol_types::SolType;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint256;
use euclid::cross_chain_user::CrossChainUser;
use euclid::msgs::factory::msg::ReleaseEscrowResponse;
use euclid::token::{Token, TokenType};

use crate::wire::types::cross_chain_user::{
    cross_chain_user_from_sol, cross_chain_user_to_sol, CrossChainUserSol,
};
use crate::wire::types::token_type::{token_type_from_sol, token_type_to_sol, TokenTypeSol};
use euclid_encoding::abi::bridge::{newtype_from_string, uint256_from_sol, uint256_to_sol};
use euclid_encoding::abi::option::{opt_prim_from_sol, OptPrimSol};
use euclid_encoding::{AbiMap, EncodingError};

#[cw_serde]
pub struct ReleaseEscrowSendMsg {
    pub sender: CrossChainUser,
    pub token: Token,
    pub recipient: String,
    pub amount: Uint256,
    pub denom: TokenType,
    pub forwarding_message: Option<String>,
    // Unique per tx
    pub tx_id: String,
}

#[cw_serde]
pub struct ReleaseEscrowAckMsg {
    pub amount: Uint256,
    pub to_address: String,
    pub escrow_balance: Uint256,
}

impl AbiMap for ReleaseEscrowSendMsg {
    type Sol = (
        CrossChainUserSol,
        SolString,
        SolString,
        SolUint<256>,
        TokenTypeSol,
        OptPrimSol<SolString>,
        SolString,
    );

    fn type_name() -> &'static str {
        "ReleaseEscrowSendMsg"
    }

    fn to_sol(&self) -> Result<<Self::Sol as SolType>::RustType, EncodingError> {
        Ok((
            cross_chain_user_to_sol(&self.sender)?,
            self.token.to_string(),
            self.recipient.clone(),
            uint256_to_sol(&self.amount),
            token_type_to_sol(&self.denom)?,
            (
                self.forwarding_message.is_some(),
                self.forwarding_message.clone().unwrap_or_default(),
            ),
            self.tx_id.clone(),
        ))
    }

    fn from_sol(sol: <Self::Sol as SolType>::RustType) -> Result<Self, EncodingError> {
        let (sender, token, recipient, amount, denom, forwarding_message, tx_id) = sol;
        Ok(Self {
            sender: cross_chain_user_from_sol(sender)?,
            token: newtype_from_string("Token", token)?,
            recipient,
            amount: uint256_from_sol(amount),
            denom: token_type_from_sol(denom)?,
            forwarding_message: opt_prim_from_sol("ReleaseEscrowSendMsg", forwarding_message)?,
            tx_id,
        })
    }
}

impl From<ReleaseEscrowResponse> for ReleaseEscrowAckMsg {
    fn from(v: ReleaseEscrowResponse) -> Self {
        Self {
            amount: v.amount,
            to_address: v.to_address,
            escrow_balance: v.escrow_balance,
        }
    }
}

impl From<ReleaseEscrowAckMsg> for ReleaseEscrowResponse {
    fn from(v: ReleaseEscrowAckMsg) -> Self {
        Self {
            amount: v.amount,
            to_address: v.to_address,
            escrow_balance: v.escrow_balance,
        }
    }
}

impl AbiMap for ReleaseEscrowAckMsg {
    type Sol = (SolUint<256>, SolString, SolUint<256>);

    fn type_name() -> &'static str {
        "ReleaseEscrowAckMsg"
    }

    fn to_sol(&self) -> Result<<Self::Sol as SolType>::RustType, EncodingError> {
        Ok((
            uint256_to_sol(&self.amount),
            self.to_address.clone(),
            uint256_to_sol(&self.escrow_balance),
        ))
    }

    fn from_sol(sol: <Self::Sol as SolType>::RustType) -> Result<Self, EncodingError> {
        Ok(Self {
            amount: uint256_from_sol(sol.0),
            to_address: sol.1,
            escrow_balance: uint256_from_sol(sol.2),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use euclid::chain::ChainUid;
    use euclid_encoding::{decode, encode, AbiDecode, AbiEncode, Encoding};

    fn send() -> ReleaseEscrowSendMsg {
        ReleaseEscrowSendMsg {
            sender: CrossChainUser::new(
                ChainUid::create("chain1".to_string()).unwrap(),
                "sender-addr".to_string(),
            ),
            token: Token::create("abc".to_string()).unwrap(),
            recipient: "recipient-addr".to_string(),
            amount: Uint256::MAX,
            denom: TokenType::Native {
                denom: "uatom".to_string(),
                decimals: Some(6),
            },
            forwarding_message: Some("do-something".to_string()),
            tx_id: "tx-release".to_string(),
        }
    }

    fn ack() -> ReleaseEscrowAckMsg {
        ReleaseEscrowAckMsg {
            amount: Uint256::MAX,
            to_address: "cosmos1recipient".to_string(),
            escrow_balance: Uint256::zero(),
        }
    }

    #[test]
    fn send_json_roundtrips() {
        let msg = send();
        let bytes = encode(&msg, Encoding::Json).unwrap();
        let decoded: ReleaseEscrowSendMsg = decode(&bytes, Encoding::Json).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn send_abi_roundtrips() {
        let msg = send();
        let bytes = encode(&msg, Encoding::Abi).unwrap();
        let decoded: ReleaseEscrowSendMsg = decode(&bytes, Encoding::Abi).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn ack_json_roundtrips() {
        let msg = ack();
        let bytes = encode(&msg, Encoding::Json).unwrap();
        let decoded: ReleaseEscrowAckMsg = decode(&bytes, Encoding::Json).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn ack_abi_roundtrips() {
        let msg = ack();
        let bytes = encode(&msg, Encoding::Abi).unwrap();
        let decoded: ReleaseEscrowAckMsg = decode(&bytes, Encoding::Abi).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn ack_from_conversion_roundtrips() {
        let domain = ReleaseEscrowResponse::from(ack());
        let wire = ReleaseEscrowAckMsg::from(domain.clone());
        assert_eq!(ReleaseEscrowResponse::from(wire), domain);
    }

    /// Ported from the old `roundtrip_acks` integration case: the max-value
    /// `Uint256` fields survive the ABI roundtrip on the wire mirror.
    #[test]
    fn release_escrow_ack_roundtrips_max_uint256() {
        let v = ReleaseEscrowAckMsg {
            amount: Uint256::MAX,
            to_address: "cosmos1abc".to_string(),
            escrow_balance: Uint256::zero(),
        };
        let encoded = v.to_abi_bytes().unwrap();
        assert_eq!(ReleaseEscrowAckMsg::from_abi_bytes(&encoded).unwrap(), v);
    }
}
