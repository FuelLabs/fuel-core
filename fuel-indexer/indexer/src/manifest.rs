use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Manifest {
    pub namespace: String,
    pub graphql_schema: String,
    pub wasm_module: String,
    pub start_block: Option<u64>,
    pub handlers: Vec<Handler>,
    pub test_events: Vec<Event>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Event {
    pub trigger: String,
    pub payload: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Handler {
    pub event: String,
    pub handler: String,
}
