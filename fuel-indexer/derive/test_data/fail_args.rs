#![no_std]
use fuel_indexer_derive::handler;

#[handler(SomeArg)]
fn function_one(event: SomeEvent) {
    let SomeEvent { id, account } = event;

    let t1 = Thing1 { id, account };
    t1.save();
}
