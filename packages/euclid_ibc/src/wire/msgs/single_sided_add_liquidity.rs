use alloy_sol_types::sol_data::{Array, String as SolString, Uint as SolUint};
use alloy_sol_types::SolType;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint256;
use euclid::cross_chain_user::CrossChainUser;
use euclid::liquidity::AddLiquidityResponse;
use euclid::swap::NextSwapPair;
use euclid::token::{Pair, TokenWithDenom};

use crate::wire::types::cross_chain_user::{
    cross_chain_user_from_sol, cross_chain_user_to_sol, CrossChainUserSol,
};
use crate::wire::types::next_swap_pair::{
    next_swap_pair_from_sol, next_swap_pair_to_sol, NextSwapPairSol,
};
use crate::wire::types::pair::{pair_from_sol, pair_to_sol, PairSol};
use crate::wire::types::token_with_denom::{
    token_with_denom_from_sol, token_with_denom_to_sol, TokenWithDenomSol,
};
use euclid_encoding::abi::bridge::{uint256_from_sol, uint256_to_sol};
use euclid_encoding::{AbiMap, EncodingError};

#[cw_serde]
pub struct SingleSidedAddLiquiditySendMsg {
    // Factory will set this to info.sender
    pub sender: CrossChainUser,
    // The single token the user is depositing
    pub asset_in: TokenWithDenom,
    // Total raw amount of asset_in AFTER partner-fee deduction.
    // This is the amount the hub operates on; the partner-fee portion never crosses IBC.
    pub amount_in: Uint256,
    // Raw amount of asset_in to swap into the other side of the pair (backend-computed)
    pub swap_amount: Uint256,
    // Target VLP pair. The "other" token (asset_out for the swap leg) is
    // derived as pair.get_other_token(asset_in.token).
    pub pair: Pair,
    // Swap route. v1: must be length 1; kept Vec for forward-compat.
    pub swaps: Vec<NextSwapPair>,
    // Minimum LP tokens to receive — sole user-facing slippage guard
    pub min_lp_out: Uint256,
    // Partner-fee accounting (used only by the factory ack handler).
    pub partner_fee_amount: Uint256,
    pub partner_fee_recipient: CrossChainUser,
    // Unique per tx
    pub tx_id: String,
}

impl AbiMap for SingleSidedAddLiquiditySendMsg {
    type Sol = (
        CrossChainUserSol,
        TokenWithDenomSol,
        SolUint<256>,
        SolUint<256>,
        PairSol,
        Array<NextSwapPairSol>,
        SolUint<256>,
        SolUint<256>,
        CrossChainUserSol,
        SolString,
    );

    fn type_name() -> &'static str {
        "SingleSidedAddLiquiditySendMsg"
    }

    fn to_sol(&self) -> Result<<Self::Sol as SolType>::RustType, EncodingError> {
        Ok((
            cross_chain_user_to_sol(&self.sender)?,
            token_with_denom_to_sol(&self.asset_in)?,
            uint256_to_sol(&self.amount_in),
            uint256_to_sol(&self.swap_amount),
            pair_to_sol(&self.pair)?,
            self.swaps
                .iter()
                .map(next_swap_pair_to_sol)
                .collect::<Result<Vec<_>, _>>()?,
            uint256_to_sol(&self.min_lp_out),
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
            swap_amount,
            pair,
            swaps,
            min_lp_out,
            partner_fee_amount,
            partner_fee_recipient,
            tx_id,
        ) = sol;
        Ok(Self {
            sender: cross_chain_user_from_sol(sender)?,
            asset_in: token_with_denom_from_sol(asset_in)?,
            amount_in: uint256_from_sol(amount_in),
            swap_amount: uint256_from_sol(swap_amount),
            pair: pair_from_sol(pair)?,
            swaps: swaps
                .into_iter()
                .map(next_swap_pair_from_sol)
                .collect::<Result<Vec<_>, _>>()?,
            min_lp_out: uint256_from_sol(min_lp_out),
            partner_fee_amount: uint256_from_sol(partner_fee_amount),
            partner_fee_recipient: cross_chain_user_from_sol(partner_fee_recipient)?,
            tx_id,
        })
    }
}

// The hub answers a single sided add with an `AddLiquidityResponse`, the same
// domain response the regular add liquidity flow returns. `IntoAck` keeps its
// single blanket mapping `AddLiquidityResponse => AddLiquidityAckMsg` (coherence
// forbids a second `IntoAck` impl on the same domain type), so
// `SingleSidedAddLiquidityAckMsg` is never produced through `IntoAck`. Ack
// payload types are selected exclusively by the originating send tag: tag 13
// maps to `SingleSidedAddLiquidityAckMsg` and tag 6 maps to
// `AddLiquidityAckMsg`.

#[cw_serde]
pub struct SingleSidedAddLiquidityAckMsg {
    pub mint_lp_tokens: Uint256,
    pub vlp_address: String,
    pub tx_id: String,
    pub sender: CrossChainUser,
}

impl AbiMap for SingleSidedAddLiquidityAckMsg {
    type Sol = (SolUint<256>, SolString, SolString, CrossChainUserSol);

    fn type_name() -> &'static str {
        "SingleSidedAddLiquidityAckMsg"
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

impl From<AddLiquidityResponse> for SingleSidedAddLiquidityAckMsg {
    fn from(v: AddLiquidityResponse) -> Self {
        Self {
            mint_lp_tokens: v.mint_lp_tokens,
            vlp_address: v.vlp_address,
            tx_id: v.tx_id,
            sender: v.sender,
        }
    }
}

impl From<SingleSidedAddLiquidityAckMsg> for AddLiquidityResponse {
    fn from(v: SingleSidedAddLiquidityAckMsg) -> Self {
        Self {
            mint_lp_tokens: v.mint_lp_tokens,
            vlp_address: v.vlp_address,
            tx_id: v.tx_id,
            sender: v.sender,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::envelope::AcknowledgementMsg;
    use euclid::chain::ChainUid;
    use euclid::token::{Token, TokenType};
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

    fn send() -> SingleSidedAddLiquiditySendMsg {
        SingleSidedAddLiquiditySendMsg {
            sender: ccu("sender-addr"),
            asset_in: token_with_denom(),
            amount_in: Uint256::from(500u128),
            swap_amount: Uint256::from(250u128),
            pair: Pair::new(token("abc"), token("def")).unwrap(),
            swaps: vec![NextSwapPair {
                token_in: token("abc"),
                token_out: token("def"),
                pool_key: None,
                test_fail: None,
            }],
            min_lp_out: Uint256::from(1u128),
            partner_fee_amount: Uint256::MAX,
            partner_fee_recipient: ccu("partner-addr"),
            tx_id: "tx-single-sided".to_string(),
        }
    }

    #[test]
    fn send_json_roundtrips() {
        let msg = send();
        let bytes = encode(&msg, Encoding::Json).unwrap();
        let decoded: SingleSidedAddLiquiditySendMsg = decode(&bytes, Encoding::Json).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn send_abi_roundtrips() {
        let msg = send();
        let bytes = encode(&msg, Encoding::Abi).unwrap();
        let decoded: SingleSidedAddLiquiditySendMsg = decode(&bytes, Encoding::Abi).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn single_sided_ack_json_bytes_match_domain() {
        let domain = AddLiquidityResponse {
            mint_lp_tokens: Uint256::from(1_000u128),
            vlp_address: "vlp1".to_string(),
            tx_id: "tx-1".to_string(),
            sender: CrossChainUser::new(
                ChainUid::create("chain1".to_string()).unwrap(),
                "addr1".to_string(),
            ),
        };
        let wire = SingleSidedAddLiquidityAckMsg::from(domain.clone());
        assert_eq!(
            cosmwasm_std::to_json_vec(&AcknowledgementMsg::Ok(domain)).unwrap(),
            cosmwasm_std::to_json_vec(&AcknowledgementMsg::Ok(wire)).unwrap()
        );
    }

    #[test]
    fn single_sided_ack_abi_roundtrips() {
        let wire = SingleSidedAddLiquidityAckMsg {
            mint_lp_tokens: Uint256::MAX,
            vlp_address: "vlp1".to_string(),
            tx_id: "tx-1".to_string(),
            sender: CrossChainUser::new(
                ChainUid::create("chain1".to_string()).unwrap(),
                "addr1".to_string(),
            ),
        };
        let bytes = encode(&wire, Encoding::Abi).unwrap();
        let back: SingleSidedAddLiquidityAckMsg = decode(&bytes, Encoding::Abi).unwrap();
        assert_eq!(wire, back);
    }
}
