#![no_std]
extern crate alloc;
use alloc::{vec, vec::Vec};

use fuel_indexer::types::*;
use fuel_indexer_derive::{handler, graphql_schema};

graphql_schema!("test_namespace", "schema/schema.graphql");


#[handler(filters = [])]
fn function_one(event: SomeEvent) {
    let SomeEvent { id, account } = event;

    let t1 = Thing1 {
        ident: id,
        account,
    };
    t1.save();
}


#[handler(filters = [])]
fn function_two(event: AnotherEvent) {
    let AnotherEvent { id, hash, .. } = event;

    let Thing1 { account, .. } = Thing1::load(id)
        .expect("No object with that ID");

    let t2 = Thing2 {
        ident: id,
        account,
        hash,
    };

    t2.save();
}
