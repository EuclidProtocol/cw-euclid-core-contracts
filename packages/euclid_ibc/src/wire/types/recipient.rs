use alloy_sol_types::sol_data::{Bool, String as SolString};
use alloy_sol_types::SolType;
use euclid::recipient::Recipient;
use euclid_encoding::abi::option::{opt_prim_from_sol, OptPrimSol};
use euclid_encoding::EncodingError;

use super::cross_chain_user::{
    cross_chain_user_from_sol, cross_chain_user_to_sol, CrossChainUserSol,
};
use super::limit::{limit_from_sol, limit_to_sol, LimitSol};
use super::token_type::{token_type_from_sol, token_type_to_sol, TokenTypeSol};

pub type RecipientSol = (
    CrossChainUserSol,
    LimitSol,
    TokenTypeSol,
    OptPrimSol<SolString>,
    OptPrimSol<Bool>,
);

#[allow(clippy::type_complexity)]
pub fn recipient_to_sol(
    v: &Recipient,
) -> Result<<RecipientSol as SolType>::RustType, EncodingError> {
    Ok((
        cross_chain_user_to_sol(&v.recipient)?,
        limit_to_sol(&v.amount)?,
        token_type_to_sol(&v.denom)?,
        (
            v.forwarding_message.is_some(),
            v.forwarding_message.clone().unwrap_or_default(),
        ),
        (
            v.unsafe_refund_as_voucher.is_some(),
            v.unsafe_refund_as_voucher.unwrap_or_default(),
        ),
    ))
}

#[allow(clippy::type_complexity)]
pub fn recipient_from_sol(
    sol: <RecipientSol as SolType>::RustType,
) -> Result<Recipient, EncodingError> {
    let (recipient, amount, denom, forwarding_message, unsafe_refund_as_voucher) = sol;
    Ok(Recipient {
        recipient: cross_chain_user_from_sol(recipient)?,
        amount: limit_from_sol(amount)?,
        denom: token_type_from_sol(denom)?,
        forwarding_message: opt_prim_from_sol("Recipient", forwarding_message)?,
        unsafe_refund_as_voucher: opt_prim_from_sol("Recipient", unsafe_refund_as_voucher)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::Uint256;
    use euclid::chain::ChainUid;
    use euclid::cross_chain_user::CrossChainUser;
    use euclid::limit::Limit;
    use euclid::token::TokenType;

    fn ccu() -> CrossChainUser {
        CrossChainUser::new(
            ChainUid::create("chain1".to_string()).unwrap(),
            "addr1".to_string(),
        )
    }

    fn roundtrip(v: Recipient) {
        let sol = recipient_to_sol(&v).unwrap();
        let back = recipient_from_sol(sol).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn recipient_sol_roundtrip_with_options() {
        roundtrip(Recipient {
            recipient: ccu(),
            amount: Limit::Equal(Uint256::from(500u128)),
            denom: TokenType::Voucher {},
            forwarding_message: Some("fwd".to_string()),
            unsafe_refund_as_voucher: Some(true),
        });
    }

    #[test]
    fn recipient_sol_roundtrip_without_options() {
        roundtrip(Recipient {
            recipient: ccu(),
            amount: Limit::LessThanOrEqual(Uint256::from(1u128)),
            denom: TokenType::Native {
                denom: "uatom".to_string(),
                decimals: Some(6),
            },
            forwarding_message: None,
            unsafe_refund_as_voucher: None,
        });
    }
}
