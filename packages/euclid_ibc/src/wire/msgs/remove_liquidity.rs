use alloy_sol_types::sol_data::{String as SolString, Uint as SolUint};
use alloy_sol_types::SolType;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint256;
use euclid::cross_chain_user::CrossChainUser;
use euclid::liquidity::RemoveLiquidityResponse;
use euclid::token::{Pair, PairWithAmount};

use crate::wire::types::cross_chain_user::{
    cross_chain_user_from_sol, cross_chain_user_to_sol, CrossChainUserSol,
};
use crate::wire::types::pair::{pair_from_sol, pair_to_sol, PairSol};
use crate::wire::types::pair_with_amount::{
    pair_with_amount_from_sol, pair_with_amount_to_sol, PairWithAmountSol,
};
use euclid_encoding::abi::bridge::{uint256_from_sol, uint256_to_sol};
use euclid_encoding::{AbiMap, EncodingError};

#[cw_serde]
pub struct RemoveLiquiditySendMsg {
    // Factory will set this using info.sender
    pub sender: CrossChainUser,
    pub lp_allocation: Uint256,
    pub pair: Pair,
    pub recipient: CrossChainUser,
    // Unique per tx
    pub tx_id: String,
}

#[cw_serde]
pub struct RemoveLiquidityAckMsg {
    pub liquidity_removed: PairWithAmount,
    pub burn_lp_tokens: Uint256,
    pub vlp_address: String,
}

impl AbiMap for RemoveLiquiditySendMsg {
    type Sol = (
        CrossChainUserSol,
        SolUint<256>,
        PairSol,
        CrossChainUserSol,
        SolString,
    );

    fn type_name() -> &'static str {
        "RemoveLiquiditySendMsg"
    }

    fn to_sol(&self) -> Result<<Self::Sol as SolType>::RustType, EncodingError> {
        Ok((
            cross_chain_user_to_sol(&self.sender)?,
            uint256_to_sol(&self.lp_allocation),
            pair_to_sol(&self.pair)?,
            cross_chain_user_to_sol(&self.recipient)?,
            self.tx_id.clone(),
        ))
    }

    fn from_sol(sol: <Self::Sol as SolType>::RustType) -> Result<Self, EncodingError> {
        let (sender, lp_allocation, pair, recipient, tx_id) = sol;
        Ok(Self {
            sender: cross_chain_user_from_sol(sender)?,
            lp_allocation: uint256_from_sol(lp_allocation),
            pair: pair_from_sol(pair)?,
            recipient: cross_chain_user_from_sol(recipient)?,
            tx_id,
        })
    }
}

impl From<RemoveLiquidityResponse> for RemoveLiquidityAckMsg {
    fn from(v: RemoveLiquidityResponse) -> Self {
        Self {
            liquidity_removed: v.liquidity_removed,
            burn_lp_tokens: v.burn_lp_tokens,
            vlp_address: v.vlp_address,
        }
    }
}

impl From<RemoveLiquidityAckMsg> for RemoveLiquidityResponse {
    fn from(v: RemoveLiquidityAckMsg) -> Self {
        Self {
            liquidity_removed: v.liquidity_removed,
            burn_lp_tokens: v.burn_lp_tokens,
            vlp_address: v.vlp_address,
        }
    }
}

impl AbiMap for RemoveLiquidityAckMsg {
    type Sol = (PairWithAmountSol, SolUint<256>, SolString);

    fn type_name() -> &'static str {
        "RemoveLiquidityAckMsg"
    }

    fn to_sol(&self) -> Result<<Self::Sol as SolType>::RustType, EncodingError> {
        Ok((
            pair_with_amount_to_sol(&self.liquidity_removed)?,
            uint256_to_sol(&self.burn_lp_tokens),
            self.vlp_address.clone(),
        ))
    }

    fn from_sol(sol: <Self::Sol as SolType>::RustType) -> Result<Self, EncodingError> {
        Ok(Self {
            liquidity_removed: pair_with_amount_from_sol(sol.0)?,
            burn_lp_tokens: uint256_from_sol(sol.1),
            vlp_address: sol.2,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use euclid::chain::ChainUid;
    use euclid::token::{Token, TokenWithAmount};
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

    fn pair() -> Pair {
        Pair::new(token("abc"), token("def")).unwrap()
    }

    fn liquidity_removed() -> PairWithAmount {
        PairWithAmount::new(
            TokenWithAmount {
                token: token("abc"),
                amount: Uint256::zero(),
            },
            TokenWithAmount {
                token: token("def"),
                amount: Uint256::MAX,
            },
        )
        .unwrap()
    }

    fn send() -> RemoveLiquiditySendMsg {
        RemoveLiquiditySendMsg {
            sender: ccu("sender-addr"),
            lp_allocation: Uint256::MAX,
            pair: pair(),
            recipient: ccu("recipient-addr"),
            tx_id: "tx-remove-liq".to_string(),
        }
    }

    fn ack() -> RemoveLiquidityAckMsg {
        RemoveLiquidityAckMsg {
            liquidity_removed: liquidity_removed(),
            burn_lp_tokens: Uint256::MAX,
            vlp_address: "cosmos1vlp".to_string(),
        }
    }

    #[test]
    fn send_json_roundtrips() {
        let msg = send();
        let bytes = encode(&msg, Encoding::Json).unwrap();
        let decoded: RemoveLiquiditySendMsg = decode(&bytes, Encoding::Json).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn send_abi_roundtrips() {
        let msg = send();
        let bytes = encode(&msg, Encoding::Abi).unwrap();
        let decoded: RemoveLiquiditySendMsg = decode(&bytes, Encoding::Abi).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn ack_json_roundtrips() {
        let msg = ack();
        let bytes = encode(&msg, Encoding::Json).unwrap();
        let decoded: RemoveLiquidityAckMsg = decode(&bytes, Encoding::Json).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn ack_abi_roundtrips() {
        let msg = ack();
        let bytes = encode(&msg, Encoding::Abi).unwrap();
        let decoded: RemoveLiquidityAckMsg = decode(&bytes, Encoding::Abi).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn ack_from_conversion_roundtrips() {
        let domain = RemoveLiquidityResponse::from(ack());
        let wire = RemoveLiquidityAckMsg::from(domain.clone());
        assert_eq!(RemoveLiquidityResponse::from(wire), domain);
    }
}
