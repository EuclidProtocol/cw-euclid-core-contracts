use alloy_sol_types::sol_data::{Array, String as SolString, Uint as SolUint};
use alloy_sol_types::SolType;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint256;
use euclid::cross_chain_user::CrossChainUser;
use euclid::deposit::DepositTokenResponse;
use euclid::recipient::Recipient;
use euclid::token::{Token, TokenWithDenom};

use crate::wire::types::cross_chain_user::{
    cross_chain_user_from_sol, cross_chain_user_to_sol, CrossChainUserSol,
};
use crate::wire::types::recipient::{recipient_from_sol, recipient_to_sol, RecipientSol};
use crate::wire::types::token_with_denom::{
    token_with_denom_from_sol, token_with_denom_to_sol, TokenWithDenomSol,
};
use euclid_encoding::abi::bridge::{newtype_from_string, uint256_from_sol, uint256_to_sol};
use euclid_encoding::{AbiMap, EncodingError};

#[cw_serde]
pub struct DepositTokenSendMsg {
    // Factory will set this to info.sender
    pub sender: CrossChainUser,
    // User will provide this
    pub asset_in: TokenWithDenom,
    pub amount_in: Uint256,
    pub recipients: Vec<Recipient>,
    // Unique per tx
    pub tx_id: String,
}

#[cw_serde]
pub struct DepositTokenAckMsg {
    pub amount: Uint256,
    pub token: Token,
    pub sender: CrossChainUser,
}

impl AbiMap for DepositTokenSendMsg {
    type Sol = (
        CrossChainUserSol,
        TokenWithDenomSol,
        SolUint<256>,
        Array<RecipientSol>,
        SolString,
    );

    fn type_name() -> &'static str {
        "DepositTokenSendMsg"
    }

    fn to_sol(&self) -> Result<<Self::Sol as SolType>::RustType, EncodingError> {
        Ok((
            cross_chain_user_to_sol(&self.sender)?,
            token_with_denom_to_sol(&self.asset_in)?,
            uint256_to_sol(&self.amount_in),
            self.recipients
                .iter()
                .map(recipient_to_sol)
                .collect::<Result<Vec<_>, _>>()?,
            self.tx_id.clone(),
        ))
    }

    fn from_sol(sol: <Self::Sol as SolType>::RustType) -> Result<Self, EncodingError> {
        let (sender, asset_in, amount_in, recipients, tx_id) = sol;
        Ok(Self {
            sender: cross_chain_user_from_sol(sender)?,
            asset_in: token_with_denom_from_sol(asset_in)?,
            amount_in: uint256_from_sol(amount_in),
            recipients: recipients
                .into_iter()
                .map(recipient_from_sol)
                .collect::<Result<Vec<_>, _>>()?,
            tx_id,
        })
    }
}

impl From<DepositTokenResponse> for DepositTokenAckMsg {
    fn from(v: DepositTokenResponse) -> Self {
        Self {
            amount: v.amount,
            token: v.token,
            sender: v.sender,
        }
    }
}

impl From<DepositTokenAckMsg> for DepositTokenResponse {
    fn from(v: DepositTokenAckMsg) -> Self {
        Self {
            amount: v.amount,
            token: v.token,
            sender: v.sender,
        }
    }
}

impl AbiMap for DepositTokenAckMsg {
    type Sol = (SolUint<256>, SolString, CrossChainUserSol);

    fn type_name() -> &'static str {
        "DepositTokenAckMsg"
    }

    fn to_sol(&self) -> Result<<Self::Sol as SolType>::RustType, EncodingError> {
        Ok((
            uint256_to_sol(&self.amount),
            self.token.to_string(),
            cross_chain_user_to_sol(&self.sender)?,
        ))
    }

    fn from_sol(sol: <Self::Sol as SolType>::RustType) -> Result<Self, EncodingError> {
        Ok(Self {
            amount: uint256_from_sol(sol.0),
            token: newtype_from_string("Token", sol.1)?,
            sender: cross_chain_user_from_sol(sol.2)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use euclid::chain::ChainUid;
    use euclid::limit::Limit;
    use euclid::token::TokenType;
    use euclid_encoding::{decode, encode, Encoding};

    fn ccu(addr: &str) -> CrossChainUser {
        CrossChainUser::new(
            ChainUid::create("chain1".to_string()).unwrap(),
            addr.to_string(),
        )
    }

    fn token_with_denom() -> TokenWithDenom {
        TokenWithDenom {
            token: Token::create("abc".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "uabc".to_string(),
                decimals: None,
            },
        }
    }

    fn recipient() -> Recipient {
        Recipient {
            recipient: ccu("recipient-addr"),
            amount: Limit::LessThanOrEqual(Uint256::from(1_000u128)),
            denom: TokenType::Voucher {},
            forwarding_message: None,
            unsafe_refund_as_voucher: None,
        }
    }

    fn send() -> DepositTokenSendMsg {
        DepositTokenSendMsg {
            sender: ccu("sender-addr"),
            asset_in: token_with_denom(),
            amount_in: Uint256::from(123_456u128),
            recipients: vec![recipient()],
            tx_id: "tx-deposit".to_string(),
        }
    }

    fn ack() -> DepositTokenAckMsg {
        DepositTokenAckMsg {
            amount: Uint256::MAX,
            token: Token::create("abc".to_string()).unwrap(),
            sender: ccu("sender-addr"),
        }
    }

    #[test]
    fn send_json_roundtrips() {
        let msg = send();
        let bytes = encode(&msg, Encoding::Json).unwrap();
        let decoded: DepositTokenSendMsg = decode(&bytes, Encoding::Json).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn send_abi_roundtrips() {
        let msg = send();
        let bytes = encode(&msg, Encoding::Abi).unwrap();
        let decoded: DepositTokenSendMsg = decode(&bytes, Encoding::Abi).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn ack_json_roundtrips() {
        let msg = ack();
        let bytes = encode(&msg, Encoding::Json).unwrap();
        let decoded: DepositTokenAckMsg = decode(&bytes, Encoding::Json).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn ack_abi_roundtrips() {
        let msg = ack();
        let bytes = encode(&msg, Encoding::Abi).unwrap();
        let decoded: DepositTokenAckMsg = decode(&bytes, Encoding::Abi).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn ack_from_conversion_roundtrips() {
        let domain = DepositTokenResponse::from(ack());
        let wire = DepositTokenAckMsg::from(domain.clone());
        assert_eq!(DepositTokenResponse::from(wire), domain);
    }
}
