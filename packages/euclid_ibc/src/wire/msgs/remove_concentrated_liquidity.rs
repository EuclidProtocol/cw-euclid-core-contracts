use alloy_sol_types::sol_data::{Bool, String as SolString, Uint as SolUint};
use alloy_sol_types::SolType;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;
use euclid::cross_chain_user::CrossChainUser;
use euclid::liquidity::ConcentratedRemoveLiquidityResponse;
use euclid::msgs::vlp::base::PoolKey;
use euclid::token::PairWithAmount;

use crate::wire::types::cross_chain_user::{
    cross_chain_user_from_sol, cross_chain_user_to_sol, CrossChainUserSol,
};
use crate::wire::types::pair_with_amount::{
    pair_with_amount_from_sol, pair_with_amount_to_sol, PairWithAmountSol,
};
use crate::wire::types::pool_key::{pool_key_from_sol, pool_key_to_sol, PoolKeySol};
use euclid_encoding::abi::bridge::{uint128_from_sol, uint128_to_sol};
use euclid_encoding::{AbiMap, EncodingError};

#[cw_serde]
pub struct RemoveConcentratedLiquiditySendMsg {
    pub sender: CrossChainUser,
    pub pool_key: PoolKey,
    pub position_id: Uint128,
    pub liquidity_delta: Uint128,
    pub recipient: CrossChainUser,
    pub tx_id: String,
}

#[cw_serde]
pub struct RemoveConcentratedLiquidityAckMsg {
    pub pool_key: PoolKey,
    pub position_id: Uint128,
    pub liquidity_removed: PairWithAmount,
    pub liquidity_delta: Uint128,
    pub liquidity_after: Uint128,
    pub vlp_address: String,
    pub tx_id: String,
    pub sender: CrossChainUser,
    pub position_burned: bool,
}

impl AbiMap for RemoveConcentratedLiquiditySendMsg {
    type Sol = (
        CrossChainUserSol,
        PoolKeySol,
        SolUint<128>,
        SolUint<128>,
        CrossChainUserSol,
        SolString,
    );

    fn type_name() -> &'static str {
        "RemoveConcentratedLiquiditySendMsg"
    }

    fn to_sol(&self) -> Result<<Self::Sol as SolType>::RustType, EncodingError> {
        Ok((
            cross_chain_user_to_sol(&self.sender)?,
            pool_key_to_sol(&self.pool_key)?,
            uint128_to_sol(&self.position_id),
            uint128_to_sol(&self.liquidity_delta),
            cross_chain_user_to_sol(&self.recipient)?,
            self.tx_id.clone(),
        ))
    }

    fn from_sol(sol: <Self::Sol as SolType>::RustType) -> Result<Self, EncodingError> {
        let (sender, pool_key, position_id, liquidity_delta, recipient, tx_id) = sol;
        Ok(Self {
            sender: cross_chain_user_from_sol(sender)?,
            pool_key: pool_key_from_sol(pool_key)?,
            position_id: uint128_from_sol(position_id),
            liquidity_delta: uint128_from_sol(liquidity_delta),
            recipient: cross_chain_user_from_sol(recipient)?,
            tx_id,
        })
    }
}

impl From<ConcentratedRemoveLiquidityResponse> for RemoveConcentratedLiquidityAckMsg {
    fn from(v: ConcentratedRemoveLiquidityResponse) -> Self {
        Self {
            pool_key: v.pool_key,
            position_id: v.position_id,
            liquidity_removed: v.liquidity_removed,
            liquidity_delta: v.liquidity_delta,
            liquidity_after: v.liquidity_after,
            vlp_address: v.vlp_address,
            tx_id: v.tx_id,
            sender: v.sender,
            position_burned: v.position_burned,
        }
    }
}

impl From<RemoveConcentratedLiquidityAckMsg> for ConcentratedRemoveLiquidityResponse {
    fn from(v: RemoveConcentratedLiquidityAckMsg) -> Self {
        Self {
            pool_key: v.pool_key,
            position_id: v.position_id,
            liquidity_removed: v.liquidity_removed,
            liquidity_delta: v.liquidity_delta,
            liquidity_after: v.liquidity_after,
            vlp_address: v.vlp_address,
            tx_id: v.tx_id,
            sender: v.sender,
            position_burned: v.position_burned,
        }
    }
}

#[allow(clippy::type_complexity)]
impl AbiMap for RemoveConcentratedLiquidityAckMsg {
    type Sol = (
        PoolKeySol,
        SolUint<128>,
        PairWithAmountSol,
        SolUint<128>,
        SolUint<128>,
        SolString,
        SolString,
        CrossChainUserSol,
        Bool,
    );

    fn type_name() -> &'static str {
        "RemoveConcentratedLiquidityAckMsg"
    }

    fn to_sol(&self) -> Result<<Self::Sol as SolType>::RustType, EncodingError> {
        Ok((
            pool_key_to_sol(&self.pool_key)?,
            uint128_to_sol(&self.position_id),
            pair_with_amount_to_sol(&self.liquidity_removed)?,
            uint128_to_sol(&self.liquidity_delta),
            uint128_to_sol(&self.liquidity_after),
            self.vlp_address.clone(),
            self.tx_id.clone(),
            cross_chain_user_to_sol(&self.sender)?,
            self.position_burned,
        ))
    }

    fn from_sol(sol: <Self::Sol as SolType>::RustType) -> Result<Self, EncodingError> {
        Ok(Self {
            pool_key: pool_key_from_sol(sol.0)?,
            position_id: uint128_from_sol(sol.1),
            liquidity_removed: pair_with_amount_from_sol(sol.2)?,
            liquidity_delta: uint128_from_sol(sol.3),
            liquidity_after: uint128_from_sol(sol.4),
            vlp_address: sol.5,
            tx_id: sol.6,
            sender: cross_chain_user_from_sol(sol.7)?,
            position_burned: sol.8,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::Uint256;
    use euclid::chain::ChainUid;
    use euclid::msgs::vlp::base::PoolType;
    use euclid::token::{Pair, Token, TokenWithAmount};
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

    fn pool_key() -> PoolKey {
        PoolKey {
            pair: Pair::new(token("abc"), token("def")).unwrap(),
            pool_type: PoolType::Concentrated {
                fee_tier_bps: 30,
                tick_spacing: 60,
            },
        }
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

    fn send() -> RemoveConcentratedLiquiditySendMsg {
        RemoveConcentratedLiquiditySendMsg {
            sender: ccu("sender-addr"),
            pool_key: pool_key(),
            position_id: Uint128::new(9),
            liquidity_delta: Uint128::MAX,
            recipient: ccu("recipient-addr"),
            tx_id: "tx-remove-clp".to_string(),
        }
    }

    fn ack() -> RemoveConcentratedLiquidityAckMsg {
        RemoveConcentratedLiquidityAckMsg {
            pool_key: pool_key(),
            position_id: Uint128::MAX,
            liquidity_removed: liquidity_removed(),
            liquidity_delta: Uint128::new(7),
            liquidity_after: Uint128::zero(),
            vlp_address: "cosmos1vlp".to_string(),
            tx_id: "tx-remove-clp".to_string(),
            sender: ccu("sender-addr"),
            position_burned: true,
        }
    }

    #[test]
    fn send_json_roundtrips() {
        let msg = send();
        let bytes = encode(&msg, Encoding::Json).unwrap();
        let decoded: RemoveConcentratedLiquiditySendMsg = decode(&bytes, Encoding::Json).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn send_abi_roundtrips() {
        let msg = send();
        let bytes = encode(&msg, Encoding::Abi).unwrap();
        let decoded: RemoveConcentratedLiquiditySendMsg = decode(&bytes, Encoding::Abi).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn ack_json_roundtrips() {
        let msg = ack();
        let bytes = encode(&msg, Encoding::Json).unwrap();
        let decoded: RemoveConcentratedLiquidityAckMsg = decode(&bytes, Encoding::Json).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn ack_abi_roundtrips() {
        let msg = ack();
        let bytes = encode(&msg, Encoding::Abi).unwrap();
        let decoded: RemoveConcentratedLiquidityAckMsg = decode(&bytes, Encoding::Abi).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn ack_from_conversion_roundtrips() {
        let domain = ConcentratedRemoveLiquidityResponse::from(ack());
        let wire = RemoveConcentratedLiquidityAckMsg::from(domain.clone());
        assert_eq!(ConcentratedRemoveLiquidityResponse::from(wire), domain);
    }
}
