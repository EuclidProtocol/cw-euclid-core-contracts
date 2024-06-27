#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::Empty;
use cw_multi_test::Contract;

use std::collections::HashMap;

use cosmwasm_std::{coin, BlockInfo, Decimal, Timestamp, Validator};
use cw_multi_test::{App, AppBuilder, BankKeeper, MockAddressGenerator, MockApiBech32, WasmKeeper};
pub const ADMIN_USERNAME: &str = "am";

pub type MockApp = App<BankKeeper, MockApiBech32>;

use core::fmt;

use cosmwasm_std::{Addr, Coin};
use cw_multi_test::{AppResponse, Executor};
use serde::{de::DeserializeOwned, Serialize};

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

/// ////
/// ////
///

pub struct MockEuclidBuilder {
    contracts: Vec<(&'static str, Box<dyn Contract<Empty>>)>,
    eucl: MockEuclid,
    wallets: Vec<(&'static str, Vec<Coin>)>,
    raw_balances: Vec<(Addr, Vec<Coin>)>,
}

impl MockEuclidBuilder {
    pub fn new(app: &mut MockApp, admin_wallet_name: &'static str) -> Self {
        let eucl = MockEuclid::new(app, admin_wallet_name);
        Self {
            contracts: vec![],
            eucl,
            raw_balances: vec![],
            wallets: vec![],
        }
    }

    pub fn with_wallets(self, wallets: Vec<(&'static str, Vec<Coin>)>) -> Self {
        Self { wallets, ..self }
    }

    pub fn with_balances(self, raw_balances: &[(Addr, Vec<Coin>)]) -> Self {
        Self {
            raw_balances: raw_balances.to_vec(),
            ..self
        }
    }

    pub fn with_contracts(self, contracts: Vec<(&'static str, Box<dyn Contract<Empty>>)>) -> Self {
        Self { contracts, ..self }
    }

    pub fn build(mut self, app: &mut MockApp) -> MockEuclid {
        for (_version, contract) in self.contracts {
            app.store_code(contract);
        }

        for (wallet, balance) in self.wallets {
            let addr = self.eucl.add_wallet(app, wallet);
            if !balance.is_empty() {
                app.send_tokens(Addr::unchecked("bank"), addr, &balance)
                    .unwrap();
            }
        }

        for (addr, balance) in self.raw_balances {
            if !balance.is_empty() {
                app.send_tokens(Addr::unchecked("bank"), addr, &balance)
                    .unwrap();
            }
        }

        self.eucl
    }
}

pub trait MockContract<E: Serialize + fmt::Debug, Q: Serialize + fmt::Debug> {
    fn addr(&self) -> &Addr;

    fn execute(
        &self,
        app: &mut MockApp,
        msg: &E,
        sender: Addr,
        funds: &[Coin],
    ) -> AnyResult<AppResponse> {
        app.execute_contract(sender, self.addr().clone(), &msg, funds)
    }

    fn query<T: DeserializeOwned>(&self, app: &MockApp, msg: Q) -> T {
        app.wrap()
            .query_wasm_smart::<T>(self.addr().clone(), &msg)
            .unwrap()
    }
}

// #[macro_export]
// macro_rules! mock_eucl {
//     ($t:ident, $e:ident, $q:ident) => {
//         impl MockContract<$e, $q> for $t {
//             fn addr(&self) -> &Addr {
//                 &self.0
//             }
//         }

//         impl From<Addr> for $t {
//             fn from(addr: Addr) -> Self {
//                 Self(addr)
//             }
//         }

//         impl MockADO<$e, $q> for $t {}
//     };
// }
