use std::env::current_dir;

use cosmwasm_schema::{export_schema_with_title, schema_for, write_api};

use euclid::msgs::factory::{cw20::FactoryCw20HookMsg, ExecuteMsg, InstantiateMsg, QueryMsg};

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    out_dir.push("raw");
    write_api! {
        instantiate: InstantiateMsg,
        execute: ExecuteMsg,
        query: QueryMsg,
    }

    export_schema_with_title(&schema_for!(FactoryCw20HookMsg), &out_dir, "cw20receive");
}
