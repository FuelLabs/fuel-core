#![no_std]
extern crate alloc;
use fuel_indexer_derive::{graphql_schema, handler};
use fuels_abigen_macro::wasm_abigen;

graphql_schema!("demo_namespace", "schema/demo_schema.graphql");
wasm_abigen!(no_name, "examples/indexer-demo/contracts/indexer_demo.json");

#[handler]
fn function_one(event: MyEvent) {
    let MyEvent { contract, rega, regb } = event;

    let t1 = Thing1 {
        id: rega,
        account: Address::from(contract),
    };

    let t2 = Thing2 {
        id: regb,
        hash: Bytes32::from(contract),
    };

    t1.save();
    t2.save();
}
