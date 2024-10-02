#![cfg(not(target_arch = "wasm32"))]
use cosmwasm_std::Addr;
use cw_orch::prelude::*;
use escrow::EscrowContract;
use euclid::msgs::escrow::AllowedDenomsResponse;
use euclid::msgs::escrow::QueryMsgFns;
use euclid::msgs::escrow::{ExecuteMsgFns, InstantiateMsg};
use euclid::token::{Token, TokenType};

const _USER: &str = "user";
const _NATIVE_DENOM: &str = "native";
const _IBC_DENOM_1: &str = "ibc/denom1";
const _IBC_DENOM_2: &str = "ibc/denom2";
const _SUPPLY: u128 = 1_000_000;

#[test]
fn test_escrow() {
    let sender = Addr::unchecked("juno16g2rahf5846rxzp3fwlswy08fz8ccuwk03k57y");

    let mock = Mock::new(&sender);
    let contract_counter = EscrowContract::new(mock);

    let upload_res = contract_counter.upload();
    upload_res.unwrap();

    let _res = contract_counter
        .instantiate(
            &InstantiateMsg {
                token_id: Token::create("token".to_string()).unwrap(),
                allowed_denom: None,
            },
            None,
            None,
        )
        .unwrap();

    let native_denom = TokenType::Native {
        denom: "native".to_string(),
    };

    contract_counter
        .add_allowed_denom(native_denom.clone())
        .unwrap();

    let allowed_denoms: AllowedDenomsResponse = contract_counter.allowed_denoms().unwrap();
    assert_eq!(allowed_denoms.denoms, vec![native_denom]);
}
