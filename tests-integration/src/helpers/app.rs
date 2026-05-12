#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{Addr, BlockInfo, Coin, Empty, Uint128, Uint256};
use cw_multi_test::{AppBuilder, AppResponse, BankSudo, BasicApp, Contract, Executor, SudoMsg};
use serde::{de::DeserializeOwned, Serialize};

/// Thin wrapper around `cw_multi_test::BasicApp` providing the same conveniences
/// that `cw_orch::mock::MockBase` used to provide.
pub struct EuclidApp {
    inner: BasicApp,
    chain_id: String,
    sender: Addr,
}

impl EuclidApp {
    pub fn new(chain_id: &str, sender_name: &str) -> Self {
        let inner = AppBuilder::new().build(|router, api, storage| {
            router
                .bank
                .init_balance(storage, &api.addr_make("nobody"), vec![])
                .unwrap();
        });

        let sender = inner.api().addr_make(sender_name);

        let mut app = Self {
            inner,
            chain_id: chain_id.to_string(),
            sender,
        };
        app.inner.update_block(|b| {
            b.chain_id = chain_id.to_string();
        });
        app
    }

    pub fn sender(&self) -> Addr {
        self.sender.clone()
    }

    pub fn chain_id(&self) -> &str {
        &self.chain_id
    }

    pub fn addr_make(&self, name: &str) -> Addr {
        self.inner.api().addr_make(name)
    }

    pub fn store_code(&mut self, contract: Box<dyn Contract<Empty>>) -> u64 {
        self.inner.store_code(contract)
    }

    pub fn instantiate<M: Serialize>(
        &mut self,
        code_id: u64,
        sender: &Addr,
        msg: &M,
        funds: &[Coin],
        label: &str,
    ) -> Addr {
        self.inner
            .instantiate_contract(code_id, sender.clone(), msg, funds, label, None)
            .unwrap()
    }

    pub fn execute<M: Serialize + std::fmt::Debug>(
        &mut self,
        sender: &Addr,
        addr: &Addr,
        msg: &M,
        funds: &[Coin],
    ) -> AppResponse {
        self.inner
            .execute_contract(sender.clone(), addr.clone(), msg, funds)
            .unwrap()
    }

    pub fn try_execute<M: Serialize + std::fmt::Debug>(
        &mut self,
        sender: &Addr,
        addr: &Addr,
        msg: &M,
        funds: &[Coin],
    ) -> Result<AppResponse, anyhow::Error> {
        self.inner
            .execute_contract(sender.clone(), addr.clone(), msg, funds)
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    pub fn execute_err<M: Serialize + std::fmt::Debug>(
        &mut self,
        sender: &Addr,
        addr: &Addr,
        msg: &M,
        funds: &[Coin],
    ) -> anyhow::Error {
        self.inner
            .execute_contract(sender.clone(), addr.clone(), msg, funds)
            .map_err(|e| anyhow::anyhow!("{}", e))
            .unwrap_err()
    }

    pub fn query<M: Serialize, R: DeserializeOwned>(&self, addr: &Addr, msg: &M) -> R {
        self.inner.wrap().query_wasm_smart(addr, msg).unwrap()
    }

    pub fn try_query<M: Serialize, R: DeserializeOwned>(
        &self,
        addr: &Addr,
        msg: &M,
    ) -> Result<R, anyhow::Error> {
        self.inner
            .wrap()
            .query_wasm_smart(addr, msg)
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    pub fn set_balance(&mut self, addr: &Addr, coins: Vec<Coin>) {
        self.inner
            .init_modules(|router, _api, storage| router.bank.init_balance(storage, addr, coins))
            .unwrap();
    }

    pub fn add_balance(&mut self, addr: &Addr, coins: Vec<Coin>) {
        self.inner
            .sudo(SudoMsg::Bank(BankSudo::Mint {
                to_address: addr.to_string(),
                amount: coins,
            }))
            .unwrap();
    }

    pub fn query_balance(&self, addr: &Addr, denom: &str) -> Uint256 {
        self.inner.wrap().query_balance(addr, denom).unwrap().amount
    }

    pub fn block_info(&self) -> BlockInfo {
        self.inner.block_info()
    }

    pub fn app(&self) -> &BasicApp {
        &self.inner
    }

    pub fn app_mut(&mut self) -> &mut BasicApp {
        &mut self.inner
    }
}
