use alloy_sol_types::sol_data::{Array, String as SolString, Uint as SolUint};
use alloy_sol_types::SolType;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint256;
use euclid::cross_chain_user::CrossChainUser;
use euclid::recipient::Recipient;
use euclid::swap::{NextSwapPair, SwapResponse};
use euclid::token::{Token, TokenWithDenom};

use crate::wire::types::cross_chain_user::{
    cross_chain_user_from_sol, cross_chain_user_to_sol, CrossChainUserSol,
};
use crate::wire::types::next_swap_pair::{
    next_swap_pair_from_sol, next_swap_pair_to_sol, NextSwapPairSol,
};
use crate::wire::types::recipient::{recipient_from_sol, recipient_to_sol, RecipientSol};
use crate::wire::types::token_with_denom::{
    token_with_denom_from_sol, token_with_denom_to_sol, TokenWithDenomSol,
};
use euclid_encoding::abi::bridge::{newtype_from_string, uint256_from_sol, uint256_to_sol};
use euclid_encoding::{AbiMap, EncodingError};

#[cw_serde]
pub struct SwapSendMsg {
    // Factory will set this to info.sender
    pub sender: CrossChainUser,

    // User will provide this
    pub asset_in: TokenWithDenom,
    pub amount_in: Uint256,
    pub asset_out: Token,
    pub min_amount_out: Uint256,
    pub swaps: Vec<NextSwapPair>,

    // First element in array has highest priority
    pub recipients: Vec<Recipient>,
    pub partner_fee_amount: Uint256,
    pub partner_fee_recipient: CrossChainUser,

    // Unique per tx
    pub tx_id: String,
}

#[cw_serde]
pub struct SwapAckMsg {
    pub amount_out: Uint256,
    pub tx_id: String,
}

impl AbiMap for SwapSendMsg {
    type Sol = (
        CrossChainUserSol,
        TokenWithDenomSol,
        SolUint<256>,
        SolString,
        SolUint<256>,
        Array<NextSwapPairSol>,
        Array<RecipientSol>,
        SolUint<256>,
        CrossChainUserSol,
        SolString,
    );

    fn type_name() -> &'static str {
        "SwapSendMsg"
    }

    fn to_sol(&self) -> Result<<Self::Sol as SolType>::RustType, EncodingError> {
        Ok((
            cross_chain_user_to_sol(&self.sender)?,
            token_with_denom_to_sol(&self.asset_in)?,
            uint256_to_sol(&self.amount_in),
            self.asset_out.to_string(),
            uint256_to_sol(&self.min_amount_out),
            self.swaps
                .iter()
                .map(next_swap_pair_to_sol)
                .collect::<Result<Vec<_>, _>>()?,
            self.recipients
                .iter()
                .map(recipient_to_sol)
                .collect::<Result<Vec<_>, _>>()?,
            uint256_to_sol(&self.partner_fee_amount),
            cross_chain_user_to_sol(&self.partner_fee_recipient)?,
            self.tx_id.clone(),
        ))
    }

    fn from_sol(sol: <Self::Sol as SolType>::RustType) -> Result<Self, EncodingError> {
        let (
            sender,
            asset_in,
            amount_in,
            asset_out,
            min_amount_out,
            swaps,
            recipients,
            partner_fee_amount,
            partner_fee_recipient,
            tx_id,
        ) = sol;
        Ok(Self {
            sender: cross_chain_user_from_sol(sender)?,
            asset_in: token_with_denom_from_sol(asset_in)?,
            amount_in: uint256_from_sol(amount_in),
            asset_out: newtype_from_string("Token", asset_out)?,
            min_amount_out: uint256_from_sol(min_amount_out),
            swaps: swaps
                .into_iter()
                .map(next_swap_pair_from_sol)
                .collect::<Result<Vec<_>, _>>()?,
            recipients: recipients
                .into_iter()
                .map(recipient_from_sol)
                .collect::<Result<Vec<_>, _>>()?,
            partner_fee_amount: uint256_from_sol(partner_fee_amount),
            partner_fee_recipient: cross_chain_user_from_sol(partner_fee_recipient)?,
            tx_id,
        })
    }
}

impl From<SwapResponse> for SwapAckMsg {
    fn from(v: SwapResponse) -> Self {
        Self {
            amount_out: v.amount_out,
            tx_id: v.tx_id,
        }
    }
}

impl From<SwapAckMsg> for SwapResponse {
    fn from(v: SwapAckMsg) -> Self {
        Self {
            amount_out: v.amount_out,
            tx_id: v.tx_id,
        }
    }
}

impl AbiMap for SwapAckMsg {
    type Sol = (SolUint<256>, SolString);

    fn type_name() -> &'static str {
        "SwapAckMsg"
    }

    fn to_sol(&self) -> Result<<Self::Sol as SolType>::RustType, EncodingError> {
        Ok((uint256_to_sol(&self.amount_out), self.tx_id.clone()))
    }

    fn from_sol(sol: <Self::Sol as SolType>::RustType) -> Result<Self, EncodingError> {
        Ok(Self {
            amount_out: uint256_from_sol(sol.0),
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

    fn token(denom: &str) -> Token {
        Token::create(denom.to_string()).unwrap()
    }

    fn token_with_denom() -> TokenWithDenom {
        TokenWithDenom {
            token: token("abc"),
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

    fn send() -> SwapSendMsg {
        SwapSendMsg {
            sender: ccu("sender-addr"),
            asset_in: token_with_denom(),
            amount_in: Uint256::MAX,
            asset_out: token("out"),
            min_amount_out: Uint256::from(1u128),
            swaps: vec![NextSwapPair {
                token_in: token("abc"),
                token_out: token("xyz"),
                pool_key: None,
                test_fail: None,
            }],
            recipients: vec![recipient()],
            partner_fee_amount: Uint256::from(999u128),
            partner_fee_recipient: ccu("partner-addr"),
            tx_id: "tx-swap".to_string(),
        }
    }

    fn ack() -> SwapAckMsg {
        SwapAckMsg {
            amount_out: Uint256::MAX,
            tx_id: "tx-swap".to_string(),
        }
    }

    #[test]
    fn send_json_roundtrips() {
        let msg = send();
        let bytes = encode(&msg, Encoding::Json).unwrap();
        let decoded: SwapSendMsg = decode(&bytes, Encoding::Json).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn send_abi_roundtrips() {
        let msg = send();
        let bytes = encode(&msg, Encoding::Abi).unwrap();
        let decoded: SwapSendMsg = decode(&bytes, Encoding::Abi).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn ack_json_roundtrips() {
        let msg = ack();
        let bytes = encode(&msg, Encoding::Json).unwrap();
        let decoded: SwapAckMsg = decode(&bytes, Encoding::Json).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn ack_abi_roundtrips() {
        let msg = ack();
        let bytes = encode(&msg, Encoding::Abi).unwrap();
        let decoded: SwapAckMsg = decode(&bytes, Encoding::Abi).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn ack_from_conversion_roundtrips() {
        let domain = SwapResponse::from(ack());
        let wire = SwapAckMsg::from(domain.clone());
        assert_eq!(SwapResponse::from(wire), domain);
    }
}
