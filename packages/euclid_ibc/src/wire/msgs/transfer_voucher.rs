use alloy_sol_types::private::Bytes;
use alloy_sol_types::sol_data::{Array, String as SolString, Uint as SolUint};
use alloy_sol_types::SolType;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint256;
use euclid::cross_chain_user::CrossChainUser;
use euclid::recipient::Recipient;
use euclid::swap::TransferVoucherResponse;
use euclid::token::Token;

use crate::wire::types::cross_chain_user::{
    cross_chain_user_from_sol, cross_chain_user_to_sol, CrossChainUserSol,
};
use crate::wire::types::recipient::{recipient_from_sol, recipient_to_sol, RecipientSol};
use euclid_encoding::abi::bridge::{newtype_from_string, uint256_from_sol, uint256_to_sol};
use euclid_encoding::abi::option::OptDynSol;
use euclid_encoding::abi::params::decode_params_canonical;
use euclid_encoding::abi::tagged::ensure_empty;
use euclid_encoding::{AbiMap, EncodingError};

#[cw_serde]
pub struct TransferVoucherSendMsg {
    // Factory will set this to info.sender
    pub sender: CrossChainUser,
    // User will provide this
    pub token: Token,
    pub amount: Uint256,
    pub from: Option<CrossChainUser>,
    pub recipients: Vec<Recipient>,
    // Unique per tx
    pub tx_id: String,
}

// `Option<CrossChainUser>` is a composite option: `CrossChainUser` is a domain
// struct, not a bare SolType primitive, so it rides as (bool some, bytes
// inner) with `inner` the nested ABI params encoding, wired through the
// sibling `cross_chain_user_to_sol`/`cross_chain_user_from_sol` free
// functions instead of an `AbiMap` impl.
fn cross_chain_user_opt_to_sol(v: &Option<CrossChainUser>) -> Result<(bool, Bytes), EncodingError> {
    match v {
        Some(inner) => {
            let encoded =
                <CrossChainUserSol as SolType>::abi_encode_params(&cross_chain_user_to_sol(inner)?);
            Ok((true, encoded.into()))
        }
        None => Ok((false, Bytes::new())),
    }
}

fn cross_chain_user_opt_from_sol(
    sol: (bool, Bytes),
) -> Result<Option<CrossChainUser>, EncodingError> {
    let (some, data) = sol;
    if some {
        let decoded = decode_params_canonical::<CrossChainUserSol>("CrossChainUser", &data)?;
        Ok(Some(cross_chain_user_from_sol(decoded)?))
    } else {
        ensure_empty("CrossChainUser", &data)?;
        Ok(None)
    }
}

#[cw_serde]
pub struct TransferVoucherAckMsg {
    pub token: Token,
    pub tx_id: String,
}

impl AbiMap for TransferVoucherSendMsg {
    type Sol = (
        CrossChainUserSol,
        SolString,
        SolUint<256>,
        OptDynSol,
        Array<RecipientSol>,
        SolString,
    );

    fn type_name() -> &'static str {
        "TransferVoucherSendMsg"
    }

    fn to_sol(&self) -> Result<<Self::Sol as SolType>::RustType, EncodingError> {
        Ok((
            cross_chain_user_to_sol(&self.sender)?,
            self.token.to_string(),
            uint256_to_sol(&self.amount),
            cross_chain_user_opt_to_sol(&self.from)?,
            self.recipients
                .iter()
                .map(recipient_to_sol)
                .collect::<Result<Vec<_>, _>>()?,
            self.tx_id.clone(),
        ))
    }

    fn from_sol(sol: <Self::Sol as SolType>::RustType) -> Result<Self, EncodingError> {
        let (sender, token, amount, from, recipients, tx_id) = sol;
        Ok(Self {
            sender: cross_chain_user_from_sol(sender)?,
            token: newtype_from_string("Token", token)?,
            amount: uint256_from_sol(amount),
            from: cross_chain_user_opt_from_sol(from)?,
            recipients: recipients
                .into_iter()
                .map(recipient_from_sol)
                .collect::<Result<Vec<_>, _>>()?,
            tx_id,
        })
    }
}

impl From<TransferVoucherResponse> for TransferVoucherAckMsg {
    fn from(v: TransferVoucherResponse) -> Self {
        Self {
            token: v.token,
            tx_id: v.tx_id,
        }
    }
}

impl From<TransferVoucherAckMsg> for TransferVoucherResponse {
    fn from(v: TransferVoucherAckMsg) -> Self {
        Self {
            token: v.token,
            tx_id: v.tx_id,
        }
    }
}

impl AbiMap for TransferVoucherAckMsg {
    type Sol = (SolString, SolString);

    fn type_name() -> &'static str {
        "TransferVoucherAckMsg"
    }

    fn to_sol(&self) -> Result<<Self::Sol as SolType>::RustType, EncodingError> {
        Ok((self.token.to_string(), self.tx_id.clone()))
    }

    fn from_sol(sol: <Self::Sol as SolType>::RustType) -> Result<Self, EncodingError> {
        Ok(Self {
            token: newtype_from_string("Token", sol.0)?,
            tx_id: sol.1,
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

    fn recipient() -> Recipient {
        Recipient {
            recipient: ccu("recipient-addr"),
            amount: Limit::LessThanOrEqual(Uint256::from(1_000u128)),
            denom: TokenType::Voucher {},
            forwarding_message: None,
            unsafe_refund_as_voucher: None,
        }
    }

    fn send() -> TransferVoucherSendMsg {
        TransferVoucherSendMsg {
            sender: ccu("sender-addr"),
            token: Token::create("abc".to_string()).unwrap(),
            amount: Uint256::MAX,
            from: Some(ccu("from-addr")),
            recipients: vec![recipient()],
            tx_id: "tx-transfer".to_string(),
        }
    }

    fn ack() -> TransferVoucherAckMsg {
        TransferVoucherAckMsg {
            token: Token::create("abc".to_string()).unwrap(),
            tx_id: "tx-transfer".to_string(),
        }
    }

    #[test]
    fn send_json_roundtrips() {
        let msg = send();
        let bytes = encode(&msg, Encoding::Json).unwrap();
        let decoded: TransferVoucherSendMsg = decode(&bytes, Encoding::Json).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn send_abi_roundtrips() {
        let msg = send();
        let bytes = encode(&msg, Encoding::Abi).unwrap();
        let decoded: TransferVoucherSendMsg = decode(&bytes, Encoding::Abi).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn ack_json_roundtrips() {
        let msg = ack();
        let bytes = encode(&msg, Encoding::Json).unwrap();
        let decoded: TransferVoucherAckMsg = decode(&bytes, Encoding::Json).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn ack_abi_roundtrips() {
        let msg = ack();
        let bytes = encode(&msg, Encoding::Abi).unwrap();
        let decoded: TransferVoucherAckMsg = decode(&bytes, Encoding::Abi).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn ack_from_conversion_roundtrips() {
        let domain = TransferVoucherResponse::from(ack());
        let wire = TransferVoucherAckMsg::from(domain.clone());
        assert_eq!(TransferVoucherResponse::from(wire), domain);
    }
}
