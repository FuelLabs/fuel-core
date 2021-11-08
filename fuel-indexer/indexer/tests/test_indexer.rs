use fuel_indexer::types::*;
use fuel_wasm_executor::{IndexExecutor, Manifest, SchemaManager};
use serde::{Deserialize, Serialize};
use serde_json;

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

const DATABASE_URL: &'static str = "postgres://postgres:my-secret@127.0.0.1:5432";
const GRAPHQL_SCHEMA: &'static str = include_str!("./test_data/schema.graphql");
const MANIFEST: &'static str = include_str!("./test_data/manifest.yaml");
const WASM_BYTES: &'static [u8] = include_bytes!("./test_data/simple_wasm.wasm");

#[cfg(feature = "postgres")]
#[test]
fn test_indexer() {
    let manifest: Manifest = serde_yaml::from_str(MANIFEST).expect("Bad manifest file.");

    let schema_manager =
        SchemaManager::new(DATABASE_URL.to_string()).expect("Schema manager failed");

    schema_manager
        .new_schema(&manifest.namespace, GRAPHQL_SCHEMA)
        .expect("Could not create new schema");

    let test_events = manifest.test_events.clone();

    let instance = IndexExecutor::new(DATABASE_URL.to_string(), manifest, WASM_BYTES)
        .expect("Error creating IndexExecutor");

    for event in test_events {
        if event.trigger == "an_event_name" {
            let evt: SomeEvent = serde_json::from_str(&event.payload).expect("Bad payload value");
            instance
                .trigger_event("an_event_name", serialize(&evt))
                .expect("Indexing failed");
        } else if event.trigger == "another_event_name" {
            let evt: AnotherEvent =
                serde_json::from_str(&event.payload).expect("Bad payload value");
            instance
                .trigger_event("another_event_name", serialize(&evt))
                .expect("Indexing failed");
        } else {
            println!("NO handler for {}", event.trigger);
        }
    }
}
