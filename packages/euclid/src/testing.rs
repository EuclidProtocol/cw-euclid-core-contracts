// #![cfg(not(target_arch = "wasm32"))]

// use cosmwasm_std::Empty;
// use cw_multi_test::Contract;

// use std::collections::HashMap;

// use cosmwasm_std::{coin, BlockInfo, Decimal, Timestamp, Validator};
// use cw_multi_test::{App, AppBuilder, BankKeeper, MockAddressGenerator, MockApiBech32, WasmKeeper};
// pub const ADMIN_USERNAME: &str = "am";

// pub type MockApp = App<BankKeeper, MockApiBech32>;

// use core::fmt;

// use cosmwasm_std::{Addr, Coin};
// use cw_multi_test::{AppResponse, Executor};
// use serde::{de::DeserializeOwned, Serialize};

// /// ////
// /// ////
// ///

// pub trait MockContract<E: Serialize + fmt::Debug, Q: Serialize + fmt::Debug> {
//     fn addr(&self) -> &Addr;

//     fn execute(
//         &self,
//         app: &mut MockApp,
//         msg: &E,
//         sender: Addr,
//         funds: &[Coin],
//     ) -> AnyResult<AppResponse> {
//         app.execute_contract(sender, self.addr().clone(), &msg, funds)
//     }

//     fn query<T: DeserializeOwned>(&self, app: &MockApp, msg: Q) -> T {
//         app.wrap()
//             .query_wasm_smart::<T>(self.addr().clone(), &msg)
//             .unwrap()
//     }
// }

// pub trait MockADO<E: Serialize + fmt::Debug, Q: Serialize + fmt::Debug>:
//     MockContract<E, Q>
// {
// }

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
