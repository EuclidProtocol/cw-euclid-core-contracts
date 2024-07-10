use cosmwasm_std::{Addr, Coin, Empty};
use cw_multi_test::Contract;

use super::mock::{MockApp, MockEuclid};
use cw_multi_test::Executor;

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
