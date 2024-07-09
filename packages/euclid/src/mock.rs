#!

use std::collections::HashMap;

use cosmwasm_std::{coin, BlockInfo, Decimal, Timestamp, Validator};
use cw_multi_test::{App, AppBuilder, BankKeeper, MockAddressGenerator, MockApiBech32, WasmKeeper};
pub const ADMIN_USERNAME: &str = "am";

pub type MockApp = App<BankKeeper, MockApiBech32>;

use cosmwasm_std::{Addr, Coin};
use cw_multi_test::{AppResponse, Executor};

pub use anyhow::Result as AnyResult;

pub type ExecuteResult = AnyResult<AppResponse>;

pub fn mock_app(denoms: Option<Vec<&str>>) -> MockApp {
    let denoms = denoms.unwrap_or(vec!["eucl", "uusd"]);
    AppBuilder::new()
        .with_api(MockApiBech32::new("eucl"))
        .with_wasm(WasmKeeper::new().with_address_generator(MockAddressGenerator))
        .build(|router, api, storage| {
            router
                .bank
                .init_balance(
                    storage,
                    &Addr::unchecked("bank"),
                    denoms
                        .iter()
                        .map(|d| coin(u128::MAX, *d))
                        .collect::<Vec<Coin>>(),
                )
                .unwrap();

            router
                .staking
                .add_validator(
                    api,
                    storage,
                    &BlockInfo {
                        height: 0,
                        time: Timestamp::default(),
                        chain_id: "euclid".to_string(),
                    },
                    Validator {
                        address: MockApiBech32::new("eucl")
                            .addr_make("validator1")
                            .to_string(),
                        commission: Decimal::zero(),
                        max_commission: Decimal::percent(20),
                        max_change_rate: Decimal::percent(1),
                    },
                )
                .unwrap();

            router
                .staking
                .add_validator(
                    api,
                    storage,
                    &BlockInfo {
                        height: 0,
                        time: Timestamp::default(),
                        chain_id: "euclid-1".to_string(),
                    },
                    Validator {
                        address: MockApiBech32::new("eucl")
                            .addr_make("validator2")
                            .to_string(),
                        commission: Decimal::zero(),
                        max_commission: Decimal::percent(20),
                        max_change_rate: Decimal::percent(1),
                    },
                )
                .unwrap();
        })
}

pub fn init_balances(app: &mut MockApp, balances: Vec<(Addr, &[Coin])>) {
    for (addr, coins) in balances {
        app.send_tokens(Addr::unchecked("bank"), addr, coins)
            .unwrap();
    }
}

pub struct MockEuclid {
    pub admin_address: Addr,
    pub wallets: HashMap<String, Addr>,
}

impl MockEuclid {
    pub fn new(app: &mut MockApp, admin_name: &str) -> MockEuclid {
        let mut wallets = HashMap::new();
        let admin_address = app.api().addr_make(admin_name);
        wallets
            .entry(admin_name.to_string())
            .and_modify(|_| {
                panic!("Wallet already exists");
            })
            .or_insert(admin_address.clone());

        MockEuclid {
            admin_address,
            wallets,
        }
    }

    pub fn add_wallet(&mut self, router: &mut MockApp, name: &str) -> Addr {
        let addr = router.api().addr_make(name);
        self.wallets
            .entry(name.to_string())
            .and_modify(|_| {
                panic!("Wallet already exists");
            })
            .or_insert(addr.clone());
        addr
    }

    pub fn get_wallet(&self, name: &str) -> &Addr {
        self.wallets.get(name).unwrap()
    }
}
