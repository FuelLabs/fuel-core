#[cfg(feature = "postgres")]
mod tests {
    use chrono::{TimeZone, Utc};
    use fuel_gql_client::client::{FuelClient, PageDirection, PaginationRequest};
    use fuel_core::database::Database;
    use fuel_core::{
        model::fuel_block::FuelBlock,
        schema::scalars::HexString256,
        service::{Config, FuelService},
    };
    use fuel_indexer::types::*;
    use fuel_storage::Storage;
    use fuel_vm::{consts::*, prelude::*};
    use fuel_wasm_executor::{IndexExecutor, Manifest, SchemaManager};
    use itertools::Itertools;
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
                let evt: SomeEvent =
                    serde_json::from_str(&event.payload).expect("Bad payload value");
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


    #[tokio::test]
    async fn test_blocks() {
        let script = vec![
            Opcode::ADDI(0x10, REG_ZERO, 0xca),
            Opcode::ADDI(0x11, REG_ZERO, 0xba),
            Opcode::LOG(0x10, 0x11, REG_ZERO, REG_ZERO),
            Opcode::RET(REG_ONE),
        ]
        .iter()
        .copied()
        .collect::<Vec<u8>>();

        let gas_price = 0;
        let gas_limit = 1_000_000;
        let maturity = 0;
        let transaction = fuel_tx::Transaction::script(
            gas_price,
            gas_limit,
            maturity,
            script,
            vec![],
            vec![],
            vec![],
            vec![],
        );
        let id = transaction.id();


        let srv = FuelService::new_node(Config::local_node()).await.unwrap();
        let client = FuelClient::from(srv.bound_address);
        // submit tx
        let result = client.submit(&transaction).await;


        // run test
        let blocks = client
            .blocks(PaginationRequest {
                cursor: None,
                results: 5,
                direction: PageDirection::Backward,
            })
            .await
            .unwrap();

        for block in blocks.results {
            for trans in block.transactions {
                let tx = fuel_tx::Transaction::try_from(trans).expect("Bad transaction");

                if let Some(receipts) = trans.receipts {
                    for receipt in receipts {
                        let rec = fuel_tx::Receipt::try_from(receipt).expect("Bad receipt");
                        println!("Rekpt {:?}", rec);
                    }
                }
            }
        }
        assert!(false);
    }
}
