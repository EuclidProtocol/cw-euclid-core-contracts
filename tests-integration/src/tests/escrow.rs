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
use cw_orch_interchain::prelude::*;
use cw_orch_interchain::InterchainEnv;

#[test]
fn test_escrow() {
    // Here `juno-1` is the chain-id and `juno` is the address prefix for this chain
    let mut interchain =
        MockBech32InterchainEnv::new(vec![("juno-1", "juno"), ("osmosis-1", "osmo")]);
    let _local_juno = interchain.chain("juno-1").unwrap();
    let _local_osmo = interchain.chain("osmosis-1").unwrap();
    let test_migaloo = MockBech32::new_with_chain_id("migaloo-1", "migaloo");
    interchain.add_mocks(vec![test_migaloo]);

    let sender = Addr::unchecked("juno16g2rahf5846rxzp3fwlswy08fz8ccuwk03k57y");

    let mock = Mock::new(&sender);
    let escrow_contract = EscrowContract::new(mock);

    let upload_res = escrow_contract.upload();
    upload_res.unwrap();

    let _res = escrow_contract
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

    escrow_contract
        .add_allowed_denom(native_denom.clone())
        .unwrap();

    let allowed_denoms: AllowedDenomsResponse = escrow_contract.allowed_denoms().unwrap();
    assert_eq!(allowed_denoms.denoms, vec![native_denom]);
}
