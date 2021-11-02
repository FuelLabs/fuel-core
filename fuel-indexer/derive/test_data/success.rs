#![no_std]
extern crate alloc;
use core::convert::TryFrom;
use fuel_indexer_derive::handler;
use alloc::vec::Vec;
use fuel_indexer::types::*;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
struct SomeEvent {
    id: u64,
    account: Address,
}

#[handler]
fn function_one(event: SomeEvent) {
    let SomeEvent { id, account } = event;

    assert_eq!(id, 0);
    assert_eq!(account, Address::try_from([0; 32]).expect("failed"));
}

fn main() {
    let s = SomeEvent {
        id: 0,
        account: Address::try_from([0; 32]).expect("failed"),
    };

    let mut bytes = serialize(&s);

    let ptr = bytes.as_mut_ptr();
    let len = bytes.len();
    core::mem::forget(bytes);

    function_one(ptr, len);
}
