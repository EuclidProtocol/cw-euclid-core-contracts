use cosmwasm_schema::{export_schema_with_title, schema_for, write_api};
use euclid::msgs::hook::VoucherReceive;
use euclid::msgs::orderbook_deposits::{ExecuteMsg, InstantiateMsg, QueryMsg};
use std::env::current_dir;

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    out_dir.push("raw");
    write_api! {
        instantiate: InstantiateMsg,
        execute: ExecuteMsg,
        query: QueryMsg,
    }

    export_schema_with_title(&schema_for!(VoucherReceive), &out_dir, "voucherreceive");
}
