use cosmwasm_schema::{export_schema_with_title, schema_for, write_api};
use forwarding::msgs::{
    cw20::Cw20HookMsg,
    euclid_receive::OsmosisEuclidReceiveHook,
    osmosis::{ExecuteMsg, InstantiateMsg, QueryMsg},
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

    export_schema_with_title(&schema_for!(Cw20HookMsg), &out_dir, "cw20receive");
    export_schema_with_title(
        &schema_for!(OsmosisEuclidReceiveHook),
        &out_dir,
        "euclid-receive",
    );
}
