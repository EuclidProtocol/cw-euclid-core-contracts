use alloy_sol_types::sol_data::{String as SolString, Uint as SolUint};
use alloy_sol_types::SolType;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;
use euclid::cross_chain_user::CrossChainUser;
use euclid::liquidity::ConcentratedCollectFeesResponse;
use euclid::msgs::vlp::base::PoolKey;

use crate::wire::types::cross_chain_user::{
    cross_chain_user_from_sol, cross_chain_user_to_sol, CrossChainUserSol,
};
use crate::wire::types::pool_key::{pool_key_from_sol, pool_key_to_sol, PoolKeySol};
use euclid_encoding::abi::bridge::{uint128_from_sol, uint128_to_sol};
use euclid_encoding::{AbiMap, EncodingError};

#[cw_serde]
pub struct CollectConcentratedFeesSendMsg {
    pub sender: CrossChainUser,
    pub pool_key: PoolKey,
    pub position_id: Uint128,
    pub recipient: CrossChainUser,
    pub tx_id: String,
}

#[cw_serde]
pub struct CollectConcentratedFeesAckMsg {
    pub pool_key: PoolKey,
    pub position_id: Uint128,
    pub amount_0: Uint128,
    pub amount_1: Uint128,
    pub vlp_address: String,
    pub tx_id: String,
    pub sender: CrossChainUser,
    pub recipient: CrossChainUser,
}

impl AbiMap for CollectConcentratedFeesSendMsg {
    type Sol = (
        CrossChainUserSol,
        PoolKeySol,
        SolUint<128>,
        CrossChainUserSol,
        SolString,
    );

    fn type_name() -> &'static str {
        "CollectConcentratedFeesSendMsg"
    }

    fn to_sol(&self) -> Result<<Self::Sol as SolType>::RustType, EncodingError> {
        Ok((
            cross_chain_user_to_sol(&self.sender)?,
            pool_key_to_sol(&self.pool_key)?,
            uint128_to_sol(&self.position_id),
            cross_chain_user_to_sol(&self.recipient)?,
            self.tx_id.clone(),
        ))
    }

    fn from_sol(sol: <Self::Sol as SolType>::RustType) -> Result<Self, EncodingError> {
        let (sender, pool_key, position_id, recipient, tx_id) = sol;
        Ok(Self {
            sender: cross_chain_user_from_sol(sender)?,
            pool_key: pool_key_from_sol(pool_key)?,
            position_id: uint128_from_sol(position_id),
            recipient: cross_chain_user_from_sol(recipient)?,
            tx_id,
        })
    }
}

impl From<ConcentratedCollectFeesResponse> for CollectConcentratedFeesAckMsg {
    fn from(v: ConcentratedCollectFeesResponse) -> Self {
        Self {
            pool_key: v.pool_key,
            position_id: v.position_id,
            amount_0: v.amount_0,
            amount_1: v.amount_1,
            vlp_address: v.vlp_address,
            tx_id: v.tx_id,
            sender: v.sender,
            recipient: v.recipient,
        }
    }
}

impl From<CollectConcentratedFeesAckMsg> for ConcentratedCollectFeesResponse {
    fn from(v: CollectConcentratedFeesAckMsg) -> Self {
        Self {
            pool_key: v.pool_key,
            position_id: v.position_id,
            amount_0: v.amount_0,
            amount_1: v.amount_1,
            vlp_address: v.vlp_address,
            tx_id: v.tx_id,
            sender: v.sender,
            recipient: v.recipient,
        }
    }
}

#[allow(clippy::type_complexity)]
impl AbiMap for CollectConcentratedFeesAckMsg {
    type Sol = (
        PoolKeySol,
        SolUint<128>,
        SolUint<128>,
        SolUint<128>,
        SolString,
        SolString,
        CrossChainUserSol,
        CrossChainUserSol,
    );

    fn type_name() -> &'static str {
        "CollectConcentratedFeesAckMsg"
    }

    fn to_sol(&self) -> Result<<Self::Sol as SolType>::RustType, EncodingError> {
        Ok((
            pool_key_to_sol(&self.pool_key)?,
            uint128_to_sol(&self.position_id),
            uint128_to_sol(&self.amount_0),
            uint128_to_sol(&self.amount_1),
            self.vlp_address.clone(),
            self.tx_id.clone(),
            cross_chain_user_to_sol(&self.sender)?,
            cross_chain_user_to_sol(&self.recipient)?,
        ))
    }

    fn from_sol(sol: <Self::Sol as SolType>::RustType) -> Result<Self, EncodingError> {
        Ok(Self {
            pool_key: pool_key_from_sol(sol.0)?,
            position_id: uint128_from_sol(sol.1),
            amount_0: uint128_from_sol(sol.2),
            amount_1: uint128_from_sol(sol.3),
            vlp_address: sol.4,
            tx_id: sol.5,
            sender: cross_chain_user_from_sol(sol.6)?,
            recipient: cross_chain_user_from_sol(sol.7)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use euclid::chain::ChainUid;
    use euclid::msgs::vlp::base::PoolType;
    use euclid::token::{Pair, Token};
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

    fn send() -> CollectConcentratedFeesSendMsg {
        CollectConcentratedFeesSendMsg {
            sender: ccu("sender-addr"),
            pool_key: pool_key(),
            position_id: Uint128::new(3),
            recipient: ccu("recipient-addr"),
            tx_id: "tx-collect-fees".to_string(),
        }
    }

    fn ack() -> CollectConcentratedFeesAckMsg {
        CollectConcentratedFeesAckMsg {
            pool_key: pool_key(),
            position_id: Uint128::MAX,
            amount_0: Uint128::new(1),
            amount_1: Uint128::new(2),
            vlp_address: "cosmos1vlp".to_string(),
            tx_id: "tx-collect-fees".to_string(),
            sender: ccu("sender-addr"),
            recipient: ccu("recipient-addr"),
        }
    }

    #[test]
    fn send_json_roundtrips() {
        let msg = send();
        let bytes = encode(&msg, Encoding::Json).unwrap();
        let decoded: CollectConcentratedFeesSendMsg = decode(&bytes, Encoding::Json).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn send_abi_roundtrips() {
        let msg = send();
        let bytes = encode(&msg, Encoding::Abi).unwrap();
        let decoded: CollectConcentratedFeesSendMsg = decode(&bytes, Encoding::Abi).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn ack_json_roundtrips() {
        let msg = ack();
        let bytes = encode(&msg, Encoding::Json).unwrap();
        let decoded: CollectConcentratedFeesAckMsg = decode(&bytes, Encoding::Json).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn ack_abi_roundtrips() {
        let msg = ack();
        let bytes = encode(&msg, Encoding::Abi).unwrap();
        let decoded: CollectConcentratedFeesAckMsg = decode(&bytes, Encoding::Abi).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn ack_from_conversion_roundtrips() {
        let domain = ConcentratedCollectFeesResponse::from(ack());
        let wire = CollectConcentratedFeesAckMsg::from(domain.clone());
        assert_eq!(ConcentratedCollectFeesResponse::from(wire), domain);
    }
}
