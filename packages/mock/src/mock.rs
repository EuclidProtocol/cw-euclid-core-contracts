// use std::collections::HashMap;

// use cosmwasm_std::{coin, Addr, BlockInfo, Coin, Decimal, Timestamp, Validator};
// use secret_multi_test::{App, AppBuilder, BankKeeper, Executor, WasmKeeper};
// pub const ADMIN_USERNAME: &str = "am";

// pub type MockApp = App<BankKeeper>;

// pub use anyhow::Result as AnyResult;
// pub type ExecuteResult = AnyResult<secret_multi_test::AppResponse>;

// /// Creates a mock application with default or custom denominations.
// pub fn mock_app(denoms: Option<Vec<&str>>) -> MockApp {
//     let denoms = denoms.unwrap_or(vec!["eucl", "uusd"]);
//     AppBuilder::new()
//         .with_wasm(WasmKeeper::new()) // Removed the call to with_address_generator
//         .build(|router, _api, storage| {
//             router
//                 .bank
//                 .init_balance(
//                     storage,
//                     &Addr::unchecked("bank"),
//                     denoms
//                         .iter()
//                         .map(|d| coin(u128::MAX, *d))
//                         .collect::<Vec<Coin>>(),
//                 )
//                 .unwrap();

//             router
//                 .staking
//                 .add_validator(
//                     storage,
//                     &BlockInfo {
//                         height: 0,
//                         time: Timestamp::default(),
//                         chain_id: "euclid".to_string(),
//                     },
//                     Validator {
//                         address: "validator1".to_string(),
//                         commission: Decimal::zero(),
//                         max_commission: Decimal::percent(20),
//                         max_change_rate: Decimal::percent(1),
//                     },
//                 )
//                 .unwrap();

//             router
//                 .staking
//                 .add_validator(
//                     storage,
//                     &BlockInfo {
//                         height: 0,
//                         time: Timestamp::default(),
//                         chain_id: "euclid-1".to_string(),
//                     },
//                     Validator {
//                         address: "validator2".to_string(),
//                         commission: Decimal::zero(),
//                         max_commission: Decimal::percent(20),
//                         max_change_rate: Decimal::percent(1),
//                     },
//                 )
//                 .unwrap();
//         })
// }

// /// Initializes balances in the mock app.
// pub fn init_balances(app: &mut MockApp, balances: Vec<(Addr, &[Coin])>) {
//     for (addr, coins) in balances {
//         app.send_tokens(Addr::unchecked("bank"), addr, coins)
//             .unwrap();
//     }
// }

// /// Represents a mock environment for testing.
// pub struct MockEuclid {
//     pub admin_address: Addr,
//     pub wallets: HashMap<String, Addr>,
// }

// impl MockEuclid {
//     /// Creates a new `MockEuclid` instance with an admin wallet.
//     pub fn new(app: &mut MockApp, admin_name: &str) -> MockEuclid {
//         let mut wallets = HashMap::new();
//         let admin_address = Addr::unchecked(admin_name);
//         wallets.insert(admin_name.to_string(), admin_address.clone());

//         MockEuclid {
//             admin_address,
//             wallets,
//         }
//     }

//     /// Adds a new wallet with a given name.
//     pub fn add_wallet(&mut self, name: &str) -> Addr {
//         let addr = Addr::unchecked(name);
//         if self.wallets.insert(name.to_string(), addr.clone()).is_some() {
//             panic!("Wallet already exists");
//         }
//         addr
//     }

//     /// Retrieves the wallet address for a given name.
//     pub fn get_wallet(&self, name: &str) -> &Addr {
//         self.wallets.get(name).expect("Wallet not found")
//     }
// }
