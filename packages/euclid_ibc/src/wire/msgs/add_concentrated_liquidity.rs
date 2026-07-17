use alloy_sol_types::sol_data::{Int as SolInt, String as SolString, Uint as SolUint};
use alloy_sol_types::SolType;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;
use euclid::cross_chain_user::CrossChainUser;
use euclid::liquidity::ConcentratedAddLiquidityResponse;
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
use euclid_encoding::abi::bridge::{uint128_from_sol, uint128_to_sol};
use euclid_encoding::abi::option::{opt_prim_from_sol, OptPrimSol};
use euclid_encoding::{AbiMap, EncodingError};

#[cw_serde]
pub struct AddConcentratedLiquiditySendMsg {
    pub sender: CrossChainUser,
    pub pair: PairWithDenomAndAmount,
    pub pool_key: PoolKey,
    pub lower_tick_index: i64,
    pub upper_tick_index: i64,
    pub position_id: Option<Uint128>,
    pub slippage_tolerance_bps: u64,
    pub tx_id: String,
}

#[cw_serde]
pub struct AddConcentratedLiquidityAckMsg {
    pub vlp_address: String,
    pub tx_id: String,
    pub sender: CrossChainUser,
    pub position_id: Uint128,
    pub liquidity_delta: Uint128,
}

impl AbiMap for AddConcentratedLiquiditySendMsg {
    type Sol = (
        CrossChainUserSol,
        PairWithDenomAndAmountSol,
        PoolKeySol,
        SolInt<64>,
        SolInt<64>,
        OptPrimSol<SolUint<128>>,
        SolUint<64>,
        SolString,
    );

    fn type_name() -> &'static str {
        "AddConcentratedLiquiditySendMsg"
    }

    fn to_sol(&self) -> Result<<Self::Sol as SolType>::RustType, EncodingError> {
        Ok((
            cross_chain_user_to_sol(&self.sender)?,
            pair_with_denom_and_amount_to_sol(&self.pair)?,
            pool_key_to_sol(&self.pool_key)?,
            self.lower_tick_index,
            self.upper_tick_index,
            (
                self.position_id.is_some(),
                self.position_id
                    .as_ref()
                    .map(uint128_to_sol)
                    .unwrap_or_default(),
            ),
            self.slippage_tolerance_bps,
            self.tx_id.clone(),
        ))
    }

    fn from_sol(sol: <Self::Sol as SolType>::RustType) -> Result<Self, EncodingError> {
        let (
            sender,
            pair,
            pool_key,
            lower_tick_index,
            upper_tick_index,
            position_id,
            slippage_tolerance_bps,
            tx_id,
        ) = sol;
        Ok(Self {
            sender: cross_chain_user_from_sol(sender)?,
            pair: pair_with_denom_and_amount_from_sol(pair)?,
            pool_key: pool_key_from_sol(pool_key)?,
            lower_tick_index,
            upper_tick_index,
            position_id: opt_prim_from_sol(
                "RouterCrossChainConcentratedAddLiquidityExecuteMsg",
                position_id,
            )?
            .map(uint128_from_sol),
            slippage_tolerance_bps,
            tx_id,
        })
    }
}

impl From<ConcentratedAddLiquidityResponse> for AddConcentratedLiquidityAckMsg {
    fn from(v: ConcentratedAddLiquidityResponse) -> Self {
        Self {
            vlp_address: v.vlp_address,
            tx_id: v.tx_id,
            sender: v.sender,
            position_id: v.position_id,
            liquidity_delta: v.liquidity_delta,
        }
    }
}

impl From<AddConcentratedLiquidityAckMsg> for ConcentratedAddLiquidityResponse {
    fn from(v: AddConcentratedLiquidityAckMsg) -> Self {
        Self {
            vlp_address: v.vlp_address,
            tx_id: v.tx_id,
            sender: v.sender,
            position_id: v.position_id,
            liquidity_delta: v.liquidity_delta,
        }
    }
}

impl AbiMap for AddConcentratedLiquidityAckMsg {
    type Sol = (
        SolString,
        SolString,
        CrossChainUserSol,
        SolUint<128>,
        SolUint<128>,
    );

    fn type_name() -> &'static str {
        "AddConcentratedLiquidityAckMsg"
    }

    fn to_sol(&self) -> Result<<Self::Sol as SolType>::RustType, EncodingError> {
        Ok((
            self.vlp_address.clone(),
            self.tx_id.clone(),
            cross_chain_user_to_sol(&self.sender)?,
            uint128_to_sol(&self.position_id),
            uint128_to_sol(&self.liquidity_delta),
        ))
    }

    fn from_sol(sol: <Self::Sol as SolType>::RustType) -> Result<Self, EncodingError> {
        Ok(Self {
            vlp_address: sol.0,
            tx_id: sol.1,
            sender: cross_chain_user_from_sol(sol.2)?,
            position_id: uint128_from_sol(sol.3),
            liquidity_delta: uint128_from_sol(sol.4),
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

    fn send() -> AddConcentratedLiquiditySendMsg {
        AddConcentratedLiquiditySendMsg {
            sender: ccu(),
            pair: pda(),
            pool_key: pool_key(),
            lower_tick_index: -887_272,
            upper_tick_index: 887_272,
            position_id: Some(Uint128::new(7)),
            slippage_tolerance_bps: 25,
            tx_id: "tx-add-clp".to_string(),
        }
    }

    fn ack() -> AddConcentratedLiquidityAckMsg {
        AddConcentratedLiquidityAckMsg {
            vlp_address: "cosmos1vlp".to_string(),
            tx_id: "tx-add-clp".to_string(),
            sender: ccu(),
            position_id: Uint128::MAX,
            liquidity_delta: Uint128::new(42),
        }
    }

    #[test]
    fn send_json_roundtrips() {
        let msg = send();
        let bytes = encode(&msg, Encoding::Json).unwrap();
        let decoded: AddConcentratedLiquiditySendMsg = decode(&bytes, Encoding::Json).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn send_abi_roundtrips() {
        let msg = send();
        let bytes = encode(&msg, Encoding::Abi).unwrap();
        let decoded: AddConcentratedLiquiditySendMsg = decode(&bytes, Encoding::Abi).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn ack_json_roundtrips() {
        let msg = ack();
        let bytes = encode(&msg, Encoding::Json).unwrap();
        let decoded: AddConcentratedLiquidityAckMsg = decode(&bytes, Encoding::Json).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn ack_abi_roundtrips() {
        let msg = ack();
        let bytes = encode(&msg, Encoding::Abi).unwrap();
        let decoded: AddConcentratedLiquidityAckMsg = decode(&bytes, Encoding::Abi).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn ack_from_conversion_roundtrips() {
        let domain = ConcentratedAddLiquidityResponse::from(ack());
        let wire = AddConcentratedLiquidityAckMsg::from(domain.clone());
        assert_eq!(ConcentratedAddLiquidityResponse::from(wire), domain);
    }
}
