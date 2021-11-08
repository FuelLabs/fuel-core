#![no_std]
use fuel_indexer_derive::handler;

#[handler]
fn function_one(self, event: SomeEvent) {
    let SomeEvent { id, account } = event;

    let t1 = Thing1 { id, account };
    t1.save();
}
