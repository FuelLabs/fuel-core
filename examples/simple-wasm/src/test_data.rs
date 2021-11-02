// NOTE: to be replaced by ABI gen when it's done....
use fuel_indexer::types::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct SomeEvent {
    pub id: ID,
    pub account: Address,
}

#[derive(Serialize, Deserialize)]
pub struct AnotherEvent {
    pub id: ID,
    pub hash: Bytes32,
    pub sub_event: SomeEvent,
}
