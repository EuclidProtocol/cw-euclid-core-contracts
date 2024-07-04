use crate::mock::MockApp;
pub use anyhow::Result as AnyResult;
use core::fmt;
use cosmwasm_std::{Addr, Coin};
use cw_multi_test::{AppResponse, Executor};
use serde::{de::DeserializeOwned, Serialize};
pub type ExecuteResult = AnyResult<AppResponse>;

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

#[macro_export]
macro_rules! mock_eucl {
    ($t:ident, $e:ident, $q:ident) => {
        impl MockContract<$e, $q> for $t {
            fn addr(&self) -> &Addr {
                &self.0
            }
        }

        impl From<Addr> for $t {
            fn from(addr: Addr) -> Self {
                Self(addr)
            }
        }

        impl MockADO<$e, $q> for $t {}
    };
}
