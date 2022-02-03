extern crate alloc;

#[cfg(feature = "postgres")]
mod tests {
    use fuel_core::service::{Config, FuelService};
    use fuel_gql_client::client::FuelClient;
    use fuel_vm::{consts::*, prelude::*};
    use fuel_wasm_executor::{IndexerConfig, IndexerService, Manifest};

    const DATABASE_URL: &'static str = "postgres://postgres:my-secret@127.0.0.1:5432";
    const GRAPHQL_SCHEMA: &'static str = include_str!("./test_data/demo_schema.graphql");
    const MANIFEST: &'static str = include_str!("./test_data/demo_manifest.yaml");
    const WASM_BYTES: &'static [u8] = include_bytes!("./test_data/indexer_demo.wasm");

    fn create_log_transaction(rega: u16, regb: u16) -> Transaction {
        let script = vec![
            Opcode::ADDI(0x10, REG_ZERO, rega),
            Opcode::ADDI(0x11, REG_ZERO, regb),
            Opcode::LOG(0x10, 0x11, REG_ZERO, REG_ZERO),
            Opcode::LOG(0x11, 0x12, REG_ZERO, REG_ZERO),
            Opcode::RET(REG_ONE),
        ]
        .iter()
        .copied()
        .collect::<Vec<u8>>();

        let gas_price = 0;
        let gas_limit = 1_000_000;
        let maturity = 0;
        Transaction::script(
            gas_price,
            gas_limit,
            maturity,
            script,
            vec![],
            vec![],
            vec![],
            vec![],
        )
    }

    #[tokio::test]
    async fn test_blocks() {
        let srv = FuelService::new_node(Config::local_node()).await.unwrap();
        let client = FuelClient::from(srv.bound_address);
        // submit tx
        let _ = client.submit(&create_log_transaction(0xca, 0xba)).await;
        let _ = client.submit(&create_log_transaction(0xfa, 0x4f)).await;
        let _ = client.submit(&create_log_transaction(0x33, 0x11)).await;

        let config = IndexerConfig {
            fuel_node_addr: srv.bound_address,
            database_url: DATABASE_URL.to_string(),
            listen_endpoint: "0.0.0.0:9999".parse().unwrap(),
        };

        let mut indexer_service = IndexerService::new(config).unwrap();

        let manifest: Manifest = serde_yaml::from_str(MANIFEST).expect("Bad yaml file");
        indexer_service
            .add_indexer(manifest, GRAPHQL_SCHEMA, WASM_BYTES, true)
            .expect("Failed to initialize indexer");

        indexer_service.run().await;
    }
}
