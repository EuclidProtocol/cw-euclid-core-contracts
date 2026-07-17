use alloy_sol_types::sol_data::{String as SolString, Uint as SolUint};
use alloy_sol_types::SolType;
use cosmwasm_schema::cw_serde;
use euclid::cross_chain_user::CrossChainUser;
use euclid::msgs::vlp::base::PoolConfig;
use euclid::token::PairWithDenomAndAmount;

use crate::wire::types::cross_chain_user::{
    cross_chain_user_from_sol, cross_chain_user_to_sol, CrossChainUserSol,
};
use crate::wire::types::pair_with_denom_and_amount::{
    pair_with_denom_and_amount_from_sol, pair_with_denom_and_amount_to_sol,
    PairWithDenomAndAmountSol,
};
use crate::wire::types::pool_config::{pool_config_from_sol, pool_config_to_sol, PoolConfigSol};
use euclid_encoding::{AbiMap, EncodingError};

#[cw_serde]
pub struct RequestPoolCreationSendMsg {
    // Factory will set this using info.sender
    pub sender: CrossChainUser,
    pub tx_id: String,
    pub pair: PairWithDenomAndAmount,
    pub pool_config: PoolConfig,
    // User will provide this data
    pub slippage_tolerance_bps: u64,
}

// The pool-creation ack has no dedicated wire mirror: creation chains into the
// initial liquidity add, so the router acks with `AddLiquidityResponse` and the
// wire carries `AddLiquidityAckMsg` (tag 4). See `wire::transcode` tag-4 arm.

impl AbiMap for RequestPoolCreationSendMsg {
    type Sol = (
        CrossChainUserSol,
        SolString,
        PairWithDenomAndAmountSol,
        PoolConfigSol,
        SolUint<64>,
    );

    fn type_name() -> &'static str {
        "RequestPoolCreationSendMsg"
    }

    fn to_sol(&self) -> Result<<Self::Sol as SolType>::RustType, EncodingError> {
        Ok((
            cross_chain_user_to_sol(&self.sender)?,
            self.tx_id.clone(),
            pair_with_denom_and_amount_to_sol(&self.pair)?,
            pool_config_to_sol(&self.pool_config)?,
            self.slippage_tolerance_bps,
        ))
    }

    fn from_sol(sol: <Self::Sol as SolType>::RustType) -> Result<Self, EncodingError> {
        Ok(Self {
            sender: cross_chain_user_from_sol(sol.0)?,
            tx_id: sol.1,
            pair: pair_with_denom_and_amount_from_sol(sol.2)?,
            pool_config: pool_config_from_sol(sol.3)?,
            slippage_tolerance_bps: sol.4,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::Uint256;
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

    fn send() -> RequestPoolCreationSendMsg {
        RequestPoolCreationSendMsg {
            sender: ccu(),
            tx_id: "tx-request-pool".to_string(),
            pair: pda(),
            pool_config: PoolConfig::ConstantProduct {},
            slippage_tolerance_bps: 50,
        }
    }

    #[test]
    fn send_json_roundtrips() {
        let msg = send();
        let bytes = encode(&msg, Encoding::Json).unwrap();
        let decoded: RequestPoolCreationSendMsg = decode(&bytes, Encoding::Json).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn send_abi_roundtrips() {
        let msg = send();
        let bytes = encode(&msg, Encoding::Abi).unwrap();
        let decoded: RequestPoolCreationSendMsg = decode(&bytes, Encoding::Abi).unwrap();
        assert_eq!(decoded, msg);
    }
}
