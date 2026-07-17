use alloy_sol_types::sol_data::{Int as SolInt, String as SolString, Uint as SolUint};
use alloy_sol_types::SolType;
use cosmwasm_schema::cw_serde;
use euclid::cross_chain_user::CrossChainUser;
use euclid::msgs::vlp::base::PoolKey;
use euclid::token::PairWithDenomAndAmount;

use crate::wire::types::cross_chain_user::{
    cross_chain_user_from_sol, cross_chain_user_to_sol, CrossChainUserSol,
};
use crate::wire::types::pair_with_denom_and_amount::{
    pair_with_denom_and_amount_from_sol, pair_with_denom_and_amount_to_sol,
    PairWithDenomAndAmountSol,
};
use crate::wire::types::pool_key::{pool_key_from_sol, pool_key_to_sol, PoolKeySol};
use euclid_encoding::abi::option::{opt_prim_from_sol, OptPrimSol};
use euclid_encoding::{AbiMap, EncodingError};

#[cw_serde]
pub struct RequestConcentratedPoolCreationSendMsg {
    pub sender: CrossChainUser,
    pub tx_id: String,
    pub pair: PairWithDenomAndAmount,
    pub pool_key: PoolKey,
    pub slippage_tolerance_bps: u64,
    /// Initial tick for the pool price. `None` means tick 0 (1:1 price).
    pub initial_tick: Option<i64>,
}

// The concentrated pool-creation ack has no dedicated wire mirror: creation
// chains into the initial concentrated liquidity add, so the router acks with
// `ConcentratedAddLiquidityResponse` and the wire carries
// `AddConcentratedLiquidityAckMsg` (tag 5). See `wire::transcode` tag-5 arm.

impl AbiMap for RequestConcentratedPoolCreationSendMsg {
    type Sol = (
        CrossChainUserSol,
        SolString,
        PairWithDenomAndAmountSol,
        PoolKeySol,
        SolUint<64>,
        OptPrimSol<SolInt<64>>,
    );

    fn type_name() -> &'static str {
        "RequestConcentratedPoolCreationSendMsg"
    }

    fn to_sol(&self) -> Result<<Self::Sol as SolType>::RustType, EncodingError> {
        Ok((
            cross_chain_user_to_sol(&self.sender)?,
            self.tx_id.clone(),
            pair_with_denom_and_amount_to_sol(&self.pair)?,
            pool_key_to_sol(&self.pool_key)?,
            self.slippage_tolerance_bps,
            (
                self.initial_tick.is_some(),
                self.initial_tick.unwrap_or_default(),
            ),
        ))
    }

    fn from_sol(sol: <Self::Sol as SolType>::RustType) -> Result<Self, EncodingError> {
        let (sender, tx_id, pair, pool_key, slippage_tolerance_bps, initial_tick) = sol;
        Ok(Self {
            sender: cross_chain_user_from_sol(sender)?,
            tx_id,
            pair: pair_with_denom_and_amount_from_sol(pair)?,
            pool_key: pool_key_from_sol(pool_key)?,
            slippage_tolerance_bps,
            initial_tick: opt_prim_from_sol(
                "RouterCrossChainConcentratedRequestPoolCreationExecuteMsg",
                initial_tick,
            )?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::Uint256;
    use euclid::chain::ChainUid;
    use euclid::msgs::vlp::base::PoolType;
    use euclid::token::{Pair, Token, TokenType, TokenWithDenomAndAmount};
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

    fn pool_key() -> PoolKey {
        PoolKey {
            pair: Pair::new(token("abc"), token("def")).unwrap(),
            pool_type: PoolType::Concentrated {
                fee_tier_bps: 30,
                tick_spacing: 60,
            },
        }
    }

    fn send() -> RequestConcentratedPoolCreationSendMsg {
        RequestConcentratedPoolCreationSendMsg {
            sender: ccu(),
            tx_id: "tx-request-clp".to_string(),
            pair: pda(),
            pool_key: pool_key(),
            slippage_tolerance_bps: 30,
            initial_tick: Some(-42),
        }
    }

    #[test]
    fn send_json_roundtrips() {
        let msg = send();
        let bytes = encode(&msg, Encoding::Json).unwrap();
        let decoded: RequestConcentratedPoolCreationSendMsg =
            decode(&bytes, Encoding::Json).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn send_abi_roundtrips() {
        let msg = send();
        let bytes = encode(&msg, Encoding::Abi).unwrap();
        let decoded: RequestConcentratedPoolCreationSendMsg =
            decode(&bytes, Encoding::Abi).unwrap();
        assert_eq!(decoded, msg);
    }
}
