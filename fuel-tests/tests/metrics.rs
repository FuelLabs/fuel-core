use fuel_core::service::{Config, DbType, FuelService};
use fuel_core_interfaces::common::fuel_tx::{Address, AssetId};
use fuel_gql_client::client::FuelClient;
use tempfile::TempDir;

#[tokio::test]
async fn test_database_metrics() {
    let mut config = Config::local_node();
    let tmp_dir = TempDir::new().unwrap();
    config.database_type = DbType::RocksDb;
    config.database_path = tmp_dir.path().to_path_buf();
    // setup server & client
    let srv = FuelService::new_node(config).await.unwrap();

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

    let resp = reqwest::get(format!("http://{}/metrics", srv.bound_address))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    let categories = resp.split('\n').collect::<Vec<&str>>();

    srv.stop().await;

    assert_eq!(categories.len(), 13);
    assert_eq!(
        categories[0],
        "# HELP Bytes_Read The number of bytes read from the database"
    );
    assert_eq!(categories[1], "# TYPE Bytes_Read counter");
    assert_eq!(
        categories[3],
        "# HELP Bytes_Written The number of bytes written to the database"
    );
    assert_eq!(categories[4], "# TYPE Bytes_Written counter");

    // Next Check the actual measured values and ensure they are correct
    // So this test when run alone will return consistent values, however when run with all the
    // other tests at the same time the measured metrics are inconsistent. I fixed the port issue
    // (with the metrics service binding to the correct port) so I am wondering if it's tests
    // stopping and starting leading to the wrong /metrics being checked
    assert!(
        categories[5].split(' ').collect::<Vec<&str>>()[1]
            .to_string()
            .parse::<i64>()
            .unwrap()
            >= 1
    );
    assert!(
        categories[8].split(' ').collect::<Vec<&str>>()[1]
            .to_string()
            .parse::<i64>()
            .unwrap()
            >= 1
    );
    assert!(
        categories[11].split(' ').collect::<Vec<&str>>()[1]
            .to_string()
            .parse::<i64>()
            .unwrap()
            >= 1
    );
}
