use crate::state::{
    CHAIN_UID_TO_CHAIN, ESCROW_BALANCES, LOCKED_CHAINS, TOKEN_DENOMS, VIRTUAL_BALANCE_CONTRACT,
};
use crate::tests::tests::tests::{init, seed_virtual_balance, TEST_VIRTUAL_BALANCE};
use cosmwasm_std::testing::{message_info, mock_dependencies};
use cosmwasm_std::{Addr, Uint128};
use euclid::chain::{Chain, ChainType, ChainUid};
use euclid::msgs::router::TokenDenom;
use euclid::token::{Token, TokenType};
use rstest::fixture;

use crate::tests::tests::tests::MockDeps;
/// Fixture: deps with the router contract already instantiated.
#[fixture]
pub fn initialized() -> MockDeps {
    let mut deps = mock_dependencies();
    let creator = deps.api.addr_make("creator");
    init(deps.as_mut(), message_info(&creator, &[]));
    deps
}

/// Fixture: deps ready for WithdrawVoucher / TransferVoucher tests.
/// Pre-seeds VIRTUAL_BALANCE_CONTRACT, CHAIN_UID_TO_CHAIN (Native),
/// LOCKED_CHAINS (empty), TOKEN_DENOMS (usdc → uusdc on chain1),
/// and ESCROW_BALANCES (usdc on chain1 = 500).
#[fixture]
pub(crate) fn voucher_deps() -> MockDeps {
    let mut deps = mock_dependencies();
    let creator = deps.api.addr_make("creator");
    init(deps.as_mut(), message_info(&creator, &[]));

    let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
    let token = Token::create("usdc".to_string()).unwrap();

    VIRTUAL_BALANCE_CONTRACT
        .save(
            deps.as_mut().storage,
            &Addr::unchecked(TEST_VIRTUAL_BALANCE),
        )
        .unwrap();
    CHAIN_UID_TO_CHAIN
        .save(
            deps.as_mut().storage,
            chain_uid.clone(),
            &Chain {
                chain_uid: chain_uid.clone(),
                factory_address: "factory1".to_string(),
                chain_type: ChainType::Native {},
            },
        )
        .unwrap();
    LOCKED_CHAINS.save(deps.as_mut().storage, &vec![]).unwrap();
    TOKEN_DENOMS
        .save(
            deps.as_mut().storage,
            token.clone(),
            &vec![TokenDenom {
                chain_uid: chain_uid.clone(),
                token_type: TokenType::Native {
                    denom: "uusdc".to_string(),
                },
            }],
        )
        .unwrap();
    ESCROW_BALANCES
        .save(
            deps.as_mut().storage,
            (token.to_string(), chain_uid),
            &Uint128::new(500),
        )
        .unwrap();
    deps
}

/// Fixture: deps for TransferVoucher tests.
/// Pre-seeds VIRTUAL_BALANCE_CONTRACT and TOKEN_DENOMS (usdc → empty denoms list).
#[fixture]
pub(crate) fn transfer_deps() -> MockDeps {
    let mut deps = initialized();
    seed_virtual_balance(&mut deps);
    TOKEN_DENOMS
        .save(
            deps.as_mut().storage,
            Token::create("usdc".to_string()).unwrap(),
            &vec![],
        )
        .unwrap();
    deps
}
