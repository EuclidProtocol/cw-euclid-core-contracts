#![cfg(not(target_arch = "wasm32"))]

use euclid::msgs::escrow::AllowedDenomsResponse;
use euclid::token::{Token, TokenType};

use crate::helpers::app::EuclidApp;
use crate::helpers::chains::escrow_code;

#[test]
fn test_escrow() {
    let mut app = EuclidApp::new("juno-1", "sender");
    let code_id = escrow_code(&mut app);
    let sender = app.sender();

    let escrow_addr = app.instantiate(
        code_id,
        &sender,
        &euclid::msgs::escrow::InstantiateMsg {
            token_id: Token::create("token".to_string()).unwrap(),
            allowed_denom: None,
        },
        &[],
        "escrow",
    );

    let native_denom = TokenType::Native {
        denom: "native".to_string(),
        decimals: Some(18),
    };

    app.execute(
        &sender,
        &escrow_addr,
        &euclid::msgs::escrow::ExecuteMsg::AddAllowedDenom {
            denom: native_denom.clone(),
        },
        &[],
    );

    let allowed_denoms: AllowedDenomsResponse = app.query(
        &escrow_addr,
        &euclid::msgs::escrow::QueryMsg::AllowedDenoms {},
    );
    assert_eq!(allowed_denoms.denoms, vec![native_denom]);
}

#[test]
fn test_escrow_add_remove_denom() {
    let mut app = EuclidApp::new("juno-1", "sender");
    let code_id = escrow_code(&mut app);
    let sender = app.sender();

    let escrow_addr = app.instantiate(
        code_id,
        &sender,
        &euclid::msgs::escrow::InstantiateMsg {
            token_id: Token::create("token".to_string()).unwrap(),
            allowed_denom: None,
        },
        &[],
        "escrow",
    );

    let native_denom = TokenType::Native {
        denom: "native".to_string(),
            decimals: Some(6),
        };
    let ibc_denom = TokenType::Native {
        denom: "ibc/denom1".to_string(),
            decimals: Some(6),
        };

    app.execute(
        &sender,
        &escrow_addr,
        &euclid::msgs::escrow::ExecuteMsg::AddAllowedDenom {
            denom: native_denom.clone(),
        },
        &[],
    );
    app.execute(
        &sender,
        &escrow_addr,
        &euclid::msgs::escrow::ExecuteMsg::AddAllowedDenom {
            denom: ibc_denom.clone(),
        },
        &[],
    );

    let allowed_denoms: AllowedDenomsResponse = app.query(
        &escrow_addr,
        &euclid::msgs::escrow::QueryMsg::AllowedDenoms {},
    );
    assert_eq!(allowed_denoms.denoms.len(), 2);
    assert!(allowed_denoms.denoms.contains(&native_denom));
    assert!(allowed_denoms.denoms.contains(&ibc_denom));

    app.execute(
        &sender,
        &escrow_addr,
        &euclid::msgs::escrow::ExecuteMsg::DisallowDenom {
            denom: native_denom.clone(),
        },
        &[],
    );

    let allowed_denoms: AllowedDenomsResponse = app.query(
        &escrow_addr,
        &euclid::msgs::escrow::QueryMsg::AllowedDenoms {},
    );
    assert_eq!(allowed_denoms.denoms, vec![ibc_denom]);
}
