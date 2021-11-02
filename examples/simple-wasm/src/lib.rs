#![no_std]
use fuel_indexer_derive::{graphql_schema, handler};
mod test_data;
use test_data::*;

graphql_schema!("test_namespace", "schema/schema.graphql");

#[handler]
fn function_one(event: SomeEvent) {
    let SomeEvent { id, account } = event;

    let t1 = Thing1 { id, account };
    t1.save();
}

#[handler]
fn function_two(event: AnotherEvent) {
    let AnotherEvent { id, hash, .. } = event;

    let Thing1 { account, .. } = Thing1::load(id).expect("No object with that ID");

    let t2 = Thing2 { id, account, hash };

    t2.save();
}
