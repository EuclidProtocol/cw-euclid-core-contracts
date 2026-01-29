use std::env::current_dir;

use cosmwasm_schema::write_api;
use euclid::msgs::meta_transaction::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    out_dir.push("raw");
    write_api! {
        instantiate: InstantiateMsg,
        execute: ExecuteMsg,
        query: QueryMsg,
    }
}
