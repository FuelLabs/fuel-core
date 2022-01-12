extern crate alloc;

include!("./test_data/abi_code.rs");

#[cfg(feature = "postgres")]
mod tests {
    use crate::{AnotherEvent, SomeEvent};
    use fuel_wasm_executor::{IndexExecutor, Manifest, SchemaManager};
    use fuels_core::abi_encoder::ABIEncoder;

    const DATABASE_URL: &'static str = "postgres://postgres:my-secret@127.0.0.1:5432";
    const GRAPHQL_SCHEMA: &'static str = include_str!("./test_data/schema.graphql");
    const MANIFEST: &'static str = include_str!("./test_data/manifest.yaml");
    const WASM_BYTES: &'static [u8] = include_bytes!("./test_data/simple_wasm.wasm");

    #[test]
    fn test_indexer() {
        let manifest: Manifest = serde_yaml::from_str(MANIFEST).expect("Bad manifest file.");

        let schema_manager =
            SchemaManager::new(DATABASE_URL.to_string()).expect("Schema manager failed");

        schema_manager
            .new_schema(&manifest.namespace, GRAPHQL_SCHEMA)
            .expect("Could not create new schema");

        let instance = IndexExecutor::new(DATABASE_URL.to_string(), manifest, WASM_BYTES)
            .expect("Error creating IndexExecutor");

        let mystruct = SomeEvent {
            id: 4,
            account: [0x43; 32],
        }
        .into_token();

        let myotherstruct = AnotherEvent {
            id: 4,
            hash: [0x21; 32],
            bar: false,
        }
        .into_token();

        let struct1 = ABIEncoder::new()
            .encode(&[mystruct])
            .expect("Failed encoding struct1");
        let struct2 = ABIEncoder::new()
            .encode(&[myotherstruct])
            .expect("Failed encoding struct1");

        instance
            .trigger_event("an_event_name", struct1)
            .expect("Indexing failed");
        instance
            .trigger_event("another_event_name", struct2)
            .expect("Indexing failed");
    }
}
