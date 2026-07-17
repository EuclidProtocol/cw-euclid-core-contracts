use alloy_sol_types::sol_data::{String as SolString, Uint as SolUint};
use alloy_sol_types::SolType;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint256;
use euclid::cross_chain_user::CrossChainUser;
use euclid::liquidity::AddLiquidityResponse;
use euclid::token::PairWithDenomAndAmount;

use crate::wire::types::cross_chain_user::{
    cross_chain_user_from_sol, cross_chain_user_to_sol, CrossChainUserSol,
};
use crate::wire::types::pair_with_denom_and_amount::{
    pair_with_denom_and_amount_from_sol, pair_with_denom_and_amount_to_sol,
    PairWithDenomAndAmountSol,
};
use euclid_encoding::abi::bridge::{uint256_from_sol, uint256_to_sol};
use euclid_encoding::{AbiMap, EncodingError};

#[cw_serde]
pub struct AddLiquiditySendMsg {
    pub sender: CrossChainUser,
    pub slippage_tolerance_bps: u64,
    pub pair: PairWithDenomAndAmount,
    pub tx_id: String,
}

#[cw_serde]
pub struct AddLiquidityAckMsg {
    pub mint_lp_tokens: Uint256,
    pub vlp_address: String,
    pub tx_id: String,
    pub sender: CrossChainUser,
}

impl AbiMap for AddLiquiditySendMsg {
    type Sol = (
        CrossChainUserSol,
        SolUint<64>,
        PairWithDenomAndAmountSol,
        SolString,
    );

    fn type_name() -> &'static str {
        "AddLiquiditySendMsg"
    }

    fn to_sol(&self) -> Result<<Self::Sol as SolType>::RustType, EncodingError> {
        Ok((
            cross_chain_user_to_sol(&self.sender)?,
            self.slippage_tolerance_bps,
            pair_with_denom_and_amount_to_sol(&self.pair)?,
            self.tx_id.clone(),
        ))
    }

    fn from_sol(sol: <Self::Sol as SolType>::RustType) -> Result<Self, EncodingError> {
        Ok(Self {
            sender: cross_chain_user_from_sol(sol.0)?,
            slippage_tolerance_bps: sol.1,
            pair: pair_with_denom_and_amount_from_sol(sol.2)?,
            tx_id: sol.3,
        })
    }
}

impl From<AddLiquidityResponse> for AddLiquidityAckMsg {
    fn from(v: AddLiquidityResponse) -> Self {
        Self {
            mint_lp_tokens: v.mint_lp_tokens,
            vlp_address: v.vlp_address,
            tx_id: v.tx_id,
            sender: v.sender,
        }
    }
}

impl From<AddLiquidityAckMsg> for AddLiquidityResponse {
    fn from(v: AddLiquidityAckMsg) -> Self {
        Self {
            mint_lp_tokens: v.mint_lp_tokens,
            vlp_address: v.vlp_address,
            tx_id: v.tx_id,
            sender: v.sender,
        }
    }
}

impl AbiMap for AddLiquidityAckMsg {
    type Sol = (SolUint<256>, SolString, SolString, CrossChainUserSol);

    fn type_name() -> &'static str {
        "AddLiquidityAckMsg"
    }

    fn to_sol(&self) -> Result<<Self::Sol as SolType>::RustType, EncodingError> {
        Ok((
            uint256_to_sol(&self.mint_lp_tokens),
            self.vlp_address.clone(),
            self.tx_id.clone(),
            cross_chain_user_to_sol(&self.sender)?,
        ))
    }

    fn from_sol(sol: <Self::Sol as SolType>::RustType) -> Result<Self, EncodingError> {
        Ok(Self {
            mint_lp_tokens: uint256_from_sol(sol.0),
            vlp_address: sol.1,
            tx_id: sol.2,
            sender: cross_chain_user_from_sol(sol.3)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use euclid::chain::ChainUid;
    use euclid::token::{Token, TokenType, TokenWithDenomAndAmount};
    use euclid_encoding::{decode, encode, Encoding};

    fn ccu() -> CrossChainUser {
        CrossChainUser::new(
            ChainUid::create("chain1".to_string()).unwrap(),
            "addr1".to_string(),
        )
    }

    fn token(denom: &str) -> Token {
        Token::create(denom.to_string()).unwrap()
    }

    fn native(denom: &str) -> TokenType {
        TokenType::Native {
            denom: denom.to_string(),
            decimals: None,
        }
    }

    fn pda() -> PairWithDenomAndAmount {
        PairWithDenomAndAmount {
            token_1: TokenWithDenomAndAmount {
                token: token("abc"),
                amount: Uint256::from(1_000u128),
                token_type: native("uabc"),
            },
            token_2: TokenWithDenomAndAmount {
                token: token("xyz"),
                amount: Uint256::from(2_000u128),
                token_type: native("uxyz"),
            },
        }
    }

    fn send() -> AddLiquiditySendMsg {
        AddLiquiditySendMsg {
            sender: ccu(),
            slippage_tolerance_bps: 100,
            pair: pda(),
            tx_id: "tx-add-liq".to_string(),
        }
    }

    fn ack() -> AddLiquidityAckMsg {
        AddLiquidityAckMsg {
            mint_lp_tokens: Uint256::MAX,
            vlp_address: "cosmos1vlp".to_string(),
            tx_id: "tx-add-liq".to_string(),
            sender: ccu(),
        }
    }

    #[test]
    fn send_json_roundtrips() {
        let msg = send();
        let bytes = encode(&msg, Encoding::Json).unwrap();
        let decoded: AddLiquiditySendMsg = decode(&bytes, Encoding::Json).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn send_abi_roundtrips() {
        let msg = send();
        let bytes = encode(&msg, Encoding::Abi).unwrap();
        let decoded: AddLiquiditySendMsg = decode(&bytes, Encoding::Abi).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn ack_json_roundtrips() {
        let msg = ack();
        let bytes = encode(&msg, Encoding::Json).unwrap();
        let decoded: AddLiquidityAckMsg = decode(&bytes, Encoding::Json).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn ack_abi_roundtrips() {
        let msg = ack();
        let bytes = encode(&msg, Encoding::Abi).unwrap();
        let decoded: AddLiquidityAckMsg = decode(&bytes, Encoding::Abi).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn ack_from_conversion_roundtrips() {
        let domain = AddLiquidityResponse::from(ack());
        let wire = AddLiquidityAckMsg::from(domain.clone());
        assert_eq!(AddLiquidityResponse::from(wire), domain);
    }
}
