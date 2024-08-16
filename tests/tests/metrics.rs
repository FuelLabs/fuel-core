use fuel_core::service::{
    Config,
    DbType,
    FuelService,
};
use fuel_core_client::client::FuelClient;
use fuel_core_types::{
    fuel_asm::*,
    fuel_tx::*,
};
use tempfile::TempDir;
use test_helpers::send_graph_ql_query;

#[tokio::test]
async fn test_metrics_endpoint() {
    let mut config = Config::local_node();
    let tmp_dir = TempDir::new().unwrap();

    config.combined_db_config.database_path = tmp_dir.path().to_path_buf();
    config.combined_db_config.database_type = DbType::RocksDb;
    // setup server & client
    let srv = FuelService::new_node(config).await.unwrap();

    let client = FuelClient::from(srv.bound_address);
    let owner = Address::default();
    let asset_id = AssetId::new([1u8; 32]);
    // Should generate some database reads
    client.balance(&owner, Some(&asset_id)).await.unwrap();

    let script = [
        op::addi(0x10, RegId::ZERO, 0xca),
        op::addi(0x11, RegId::ZERO, 0xba),
        op::log(0x10, 0x11, RegId::ZERO, RegId::ZERO),
        op::ret(RegId::ONE),
    ];
    let script: Vec<u8> = script
        .iter()
        .flat_map(|op| u32::from(*op).to_be_bytes())
        .collect();

    client
        .submit_and_await_commit(
            &TransactionBuilder::script(script, vec![])
                .script_gas_limit(1000000)
                .add_random_fee_input()
                .finalize_as_transaction(),
        )
        .await
        .unwrap();

    let resp = reqwest::get(format!("http://{}/v1/metrics", srv.bound_address))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    let categories = resp.split('\n').collect::<Vec<&str>>();

    srv.send_stop_signal_and_await_shutdown().await.unwrap();

    // Gt check exists because testing can be weird with multiple instances running
    assert!(categories.len() >= 16);
}

#[tokio::test]
async fn metrics_include_real_query_name_instead_of_alias() {
    let node = FuelService::new_node(Config::local_node()).await.unwrap();
    let url = format!("http://{}/v1/graphql", node.bound_address);

    // Given
    const ALIAS: &str = "bbbbblooooocks";
    let query = r#"
        query {
          bbbbblooooocks: blocks(first: 1) {
            nodes {
              transactions {
                id
              }
            }
          }
        }
    "#;

    // When
    send_graph_ql_query(&url, query).await;

    // Then
    let resp = reqwest::get(format!("http://{}/v1/metrics", node.bound_address))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(!resp.contains(ALIAS))
}
