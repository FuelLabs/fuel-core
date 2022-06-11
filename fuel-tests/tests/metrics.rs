use fuel_core::service::{Config, FuelService};
use fuel_gql_client::client::FuelClient;
use fuel_tx::AssetId;
use fuel_vm::prelude::Address;

#[tokio::test]
async fn test_database_metrics() {
    let config = Config::local_node();

    // setup server & client
    let srv = FuelService::new_node(config).await.unwrap();

    let metrics_port = srv.metrics_address;
    let client = FuelClient::from(srv.bound_address);
    let owner = Address::default();
    let asset_id = AssetId::new([1u8; 32]);
    // Should generate some database reads
    _ = client
        .balance(
            format!("{:#x}", owner).as_str(),
            Some(format!("{:#x}", asset_id).as_str()),
        )
        .await;

    let resp = reqwest::get(format!("http://{}/metrics", metrics_port))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let categories = resp.split('\n').collect::<Vec<&str>>();

    srv.stop().await;

    // While a bit brittle it also ensures all the other data surrounding the actual metrics is
    // correct, but may be worth decoding more to check actual numbers of each metric
    // Check Sampling of #HELP and #TYP
    assert_eq!(
        categories[0],
        "# HELP Bytes_Read The Number of Bytes Read from the Database"
    );
    assert_eq!(categories[4], "# TYPE Bytes_Written counter");

    // Next Check the actual measured values and ensure they are correct
    assert_eq!(categories[5], "Bytes_Written 846");
    assert_eq!(categories[8], "Reads 1");
    assert_eq!(categories[11], "Writes 11");
}
