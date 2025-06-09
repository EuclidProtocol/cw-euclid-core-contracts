use cosmwasm_schema::{export_schema_with_title, schema_for, write_api};
use forwarding::msgs::{
    duality::{ExecuteMsg, InstantiateMsg, QueryMsg},
    euclid_receive::DualityEuclidReceiveHook,
};
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

    export_schema_with_title(
        &schema_for!(DualityEuclidReceiveHook),
        &out_dir,
        "euclid-receive",
    );
}
