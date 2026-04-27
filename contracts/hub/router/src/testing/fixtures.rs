use crate::state::{
    CHAIN_UID_TO_CHAIN, ESCROW_BALANCES, LOCKED_CHAINS, TOKEN_DENOMS, VIRTUAL_BALANCE_CONTRACT,
};
use crate::testing::helpers::{init, seed_virtual_balance, MockDeps, TEST_VIRTUAL_BALANCE};
use cosmwasm_std::testing::{message_info, mock_dependencies};
use cosmwasm_std::{
    from_json, to_json_binary, Addr, ContractResult, SystemResult, Uint128, Uint256, WasmQuery,
};
use euclid::chain::{Chain, ChainType, ChainUid};
use euclid::msgs::router::TokenDenom;
use euclid::msgs::virtual_balance::msg::{
    GetEscrowBalanceResponse, GetTokenMetadataByDenomResponse, QueryMsg as VirtualBalanceQueryMsg,
};
use euclid::token::{Token, TokenMetadata, TokenType};
use rstest::fixture;
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
                    decimals: None,
                },
            }],
        )
        .unwrap();
    ESCROW_BALANCES
        .save(
            deps.as_mut().storage,
            (token.to_string(), chain_uid),
            &Uint256::from(500u128),
        )
        .unwrap();

    deps.querier.update_wasm(move |q| match q {
        WasmQuery::Smart { contract_addr, msg } if contract_addr == TEST_VIRTUAL_BALANCE => {
            let parsed: VirtualBalanceQueryMsg = from_json(msg).unwrap();
            match parsed {
                VirtualBalanceQueryMsg::GetEscrowBalance { .. } => {
                    let resp = GetEscrowBalanceResponse {
                        balance: Uint256::from(500u128),
                    };
                    SystemResult::Ok(ContractResult::Ok(to_json_binary(&resp).unwrap()))
                }
                VirtualBalanceQueryMsg::GetTokenMetadataByDenom {
                    token_id,
                    chain_uid,
                    token_type,
                } => {
                    let token_type_with_decimals = match token_type {
                        TokenType::Native { denom, .. } => TokenType::Native {
                            denom,
                            decimals: Some(24),
                        },
                        other => other,
                    };
                    let resp = GetTokenMetadataByDenomResponse {
                        metadata: TokenMetadata {
                            token: Token::create(token_id).unwrap(),
                            chain_uid,
                            token_type: token_type_with_decimals,
                            allowed: true,
                        },
                    };
                    SystemResult::Ok(ContractResult::Ok(to_json_binary(&resp).unwrap()))
                }
                other => panic!("unexpected virtual_balance query: {other:?}"),
            }
        }
        _ => panic!("unexpected wasm query in voucher_deps"),
    });

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
