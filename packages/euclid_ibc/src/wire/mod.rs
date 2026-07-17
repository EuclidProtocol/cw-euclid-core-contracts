//! `IntoAck` keeps its single blanket mapping `AddLiquidityResponse =>
//! AddLiquidityAckMsg` (coherence forbids a second impl on the same domain
//! type), so `SingleSidedAddLiquidityAckMsg` is never produced through
//! `IntoAck`. Ack payload types are instead selected by the originating send
//! tag, where tag 13 maps to `SingleSidedAddLiquidityAckMsg` and tag 6 maps to
//! `AddLiquidityAckMsg`.

pub mod envelope;
pub mod msgs;
pub mod transcode;
pub mod types;

use euclid::deposit::DepositTokenResponse;
use euclid::liquidity::{
    AddLiquidityResponse, ConcentratedAddLiquidityResponse, ConcentratedCollectFeesResponse,
    ConcentratedCollectProtocolFeesResponse, ConcentratedRemoveLiquidityResponse,
    RemoveLiquidityResponse,
};
use euclid::msgs::factory::msg::{RegisterFactoryResponse, ReleaseEscrowResponse};
use euclid::msgs::vlp::base::{DeregisterDenomResponse, RegisterDenomResponse};
use euclid::swap::{SwapResponse, TransferVoucherResponse};

use crate::wire::envelope::AcknowledgementMsg;
use msgs::{
    AddConcentratedLiquidityAckMsg, AddLiquidityAckMsg, CollectConcentratedFeesAckMsg,
    CollectConcentratedProtocolFeesAckMsg, DepositTokenAckMsg, DeregisterDenomAckMsg,
    RegisterDenomAckMsg, RegisterFactoryAckMsg, ReleaseEscrowAckMsg,
    RemoveConcentratedLiquidityAckMsg, RemoveLiquidityAckMsg, SwapAckMsg, TransferVoucherAckMsg,
};

/// Convenience trait for turning a domain response into its wire ack mirror,
/// with a default helper that wraps it in the `Ok` variant of the
/// acknowledgement envelope. Contract handlers reach for `into_ok_ack()` as
/// the single entry point instead of hand-rolling `AcknowledgementMsg::Ok(..
/// .into())` at every call site.
pub trait IntoAck: Sized {
    type Ack;

    fn into_ack_msg(self) -> Self::Ack;

    fn into_ok_ack(self) -> AcknowledgementMsg<Self::Ack> {
        AcknowledgementMsg::Ok(self.into_ack_msg())
    }
}

macro_rules! impl_into_ack {
    ($domain:ty => $ack:ty) => {
        impl IntoAck for $domain {
            type Ack = $ack;

            fn into_ack_msg(self) -> Self::Ack {
                self.into()
            }
        }
    };
}

impl_into_ack!(AddLiquidityResponse => AddLiquidityAckMsg);
impl_into_ack!(RemoveLiquidityResponse => RemoveLiquidityAckMsg);
impl_into_ack!(ConcentratedAddLiquidityResponse => AddConcentratedLiquidityAckMsg);
impl_into_ack!(ConcentratedRemoveLiquidityResponse => RemoveConcentratedLiquidityAckMsg);
impl_into_ack!(ConcentratedCollectFeesResponse => CollectConcentratedFeesAckMsg);
impl_into_ack!(ConcentratedCollectProtocolFeesResponse => CollectConcentratedProtocolFeesAckMsg);
impl_into_ack!(SwapResponse => SwapAckMsg);
impl_into_ack!(TransferVoucherResponse => TransferVoucherAckMsg);
impl_into_ack!(DepositTokenResponse => DepositTokenAckMsg);
// Pool creation has no `IntoAck` mapping: creation chains into the initial
// liquidity add, so the router acks with the (concentrated) add liquidity
// response and the wire carries the add liquidity ack (tags 4/5). See
// `wire::transcode`.
impl_into_ack!(RegisterDenomResponse => RegisterDenomAckMsg);
impl_into_ack!(DeregisterDenomResponse => DeregisterDenomAckMsg);
impl_into_ack!(RegisterFactoryResponse => RegisterFactoryAckMsg);
impl_into_ack!(ReleaseEscrowResponse => ReleaseEscrowAckMsg);

#[cfg(test)]
mod tests {
    use std::fmt::Debug;

    use cosmwasm_std::{Uint128, Uint256};
    use euclid::chain::ChainUid;
    use euclid::cross_chain_user::CrossChainUser;
    use euclid::deposit::DepositTokenResponse;
    use euclid::liquidity::{
        AddLiquidityResponse, ConcentratedAddLiquidityResponse, ConcentratedCollectFeesResponse,
        ConcentratedCollectProtocolFeesResponse, ConcentratedRemoveLiquidityResponse,
        RemoveLiquidityResponse,
    };
    use euclid::msgs::factory::msg::{RegisterFactoryResponse, ReleaseEscrowResponse};
    use euclid::msgs::vlp::base::{
        DeregisterDenomResponse, PoolKey, PoolType, RegisterDenomResponse,
    };
    use euclid::swap::{SwapResponse, TransferVoucherResponse};
    use euclid::token::{Pair, PairWithAmount, Token, TokenWithAmount};

    use crate::wire::envelope::AcknowledgementMsg;
    use crate::wire::msgs::AddLiquidityAckMsg;
    use crate::wire::IntoAck;

    #[test]
    fn into_ok_ack_wraps_mirror() {
        let resp = AddLiquidityResponse {
            mint_lp_tokens: Uint256::from(1_000u128),
            vlp_address: "vlp1".to_string(),
            tx_id: "tx-1".to_string(),
            sender: CrossChainUser::new(
                ChainUid::create("chain1".to_string()).unwrap(),
                "addr1".to_string(),
            ),
        };

        let ack = resp.clone().into_ok_ack();
        assert_eq!(ack, AcknowledgementMsg::Ok(AddLiquidityAckMsg::from(resp)));
    }

    fn sender() -> CrossChainUser {
        CrossChainUser::new(
            ChainUid::create("chain1".to_string()).unwrap(),
            "addr1".to_string(),
        )
    }

    fn token(id: &str) -> Token {
        Token::create(id.to_string()).unwrap()
    }

    fn pair_with_amount() -> PairWithAmount {
        PairWithAmount::new(
            TokenWithAmount {
                token: token("abc"),
                amount: Uint256::from(1u128),
            },
            TokenWithAmount {
                token: token("def"),
                amount: Uint256::from(2u128),
            },
        )
        .unwrap()
    }

    fn pool_key() -> PoolKey {
        PoolKey {
            pair: Pair::new(token("abc"), token("def")).unwrap(),
            pool_type: PoolType::ConstantProduct {},
        }
    }

    /// `into_ok_ack` is a thin wrapper around `into_ack_msg` (i.e. the
    /// `impl_into_ack!`-generated `From` conversion) plus
    /// `AcknowledgementMsg::Ok`. Asserting `domain.into_ok_ack() ==
    /// AcknowledgementMsg::Ok(Ack::from(domain))` for every domain type pins
    /// the macro output through the single entry point contract handlers
    /// actually call, rather than only through `AddLiquidityResponse` as
    /// before.
    fn assert_into_ok_ack<D>(domain: D)
    where
        D: IntoAck + Clone,
        D::Ack: PartialEq + Debug + From<D>,
    {
        let expected = AcknowledgementMsg::Ok(D::Ack::from(domain.clone()));
        assert_eq!(domain.into_ok_ack(), expected);
    }

    #[test]
    fn into_ok_ack_covers_every_impl_into_ack_conversion() {
        assert_into_ok_ack(AddLiquidityResponse {
            mint_lp_tokens: Uint256::from(1_000u128),
            vlp_address: "vlp1".to_string(),
            tx_id: "tx-1".to_string(),
            sender: sender(),
        });
        assert_into_ok_ack(RemoveLiquidityResponse {
            liquidity_removed: pair_with_amount(),
            burn_lp_tokens: Uint256::MAX,
            vlp_address: "vlp1".to_string(),
        });
        assert_into_ok_ack(ConcentratedAddLiquidityResponse {
            vlp_address: "vlp1".to_string(),
            tx_id: "tx-concentrated-add".to_string(),
            sender: sender(),
            position_id: Uint128::MAX,
            liquidity_delta: Uint128::new(42),
        });
        assert_into_ok_ack(ConcentratedRemoveLiquidityResponse {
            pool_key: pool_key(),
            position_id: Uint128::MAX,
            liquidity_removed: pair_with_amount(),
            liquidity_delta: Uint128::new(7),
            liquidity_after: Uint128::zero(),
            vlp_address: "vlp1".to_string(),
            tx_id: "tx-concentrated-remove".to_string(),
            sender: sender(),
            position_burned: true,
        });
        assert_into_ok_ack(ConcentratedCollectFeesResponse {
            pool_key: pool_key(),
            position_id: Uint128::MAX,
            amount_0: Uint128::new(1),
            amount_1: Uint128::new(2),
            vlp_address: "vlp1".to_string(),
            tx_id: "tx-collect-fees".to_string(),
            sender: sender(),
            recipient: sender(),
        });
        assert_into_ok_ack(ConcentratedCollectProtocolFeesResponse {
            pool_key: pool_key(),
            amount_0: Uint128::MAX,
            amount_1: Uint128::zero(),
            vlp_address: "vlp1".to_string(),
            tx_id: "tx-collect-protocol-fees".to_string(),
            sender: sender(),
            recipient: sender(),
        });
        assert_into_ok_ack(SwapResponse {
            amount_out: Uint256::MAX,
            tx_id: "tx-swap".to_string(),
        });
        assert_into_ok_ack(TransferVoucherResponse {
            token: token("abc"),
            tx_id: "tx-transfer-voucher".to_string(),
        });
        assert_into_ok_ack(DepositTokenResponse {
            amount: Uint256::MAX,
            token: token("abc"),
            sender: sender(),
        });
        assert_into_ok_ack(RegisterDenomResponse {});
        assert_into_ok_ack(DeregisterDenomResponse {});
        assert_into_ok_ack(RegisterFactoryResponse {
            factory_address: "factory1".to_string(),
            chain_id: "chain-1".to_string(),
        });
        assert_into_ok_ack(ReleaseEscrowResponse {
            amount: Uint256::MAX,
            to_address: "recipient1".to_string(),
            escrow_balance: Uint256::zero(),
        });
    }
}
