use fuel_core::{
    database::Database,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_client::client::FuelClient;
// use fuel_core_types::fuel_tx::Transaction;

use fuel_core::combined_database::CombinedDatabase;
use fuel_core_types::fuel_tx::Transaction;
use tracing_test::traced_test;

#[tokio::test]
async fn health() {
    let srv = FuelService::from_database(Database::default(), Config::local_node())
        .await
        .unwrap();
    let client = FuelClient::from(srv.bound_address);

    let health = client.health().await.unwrap();
    assert!(health);
}

#[cfg(feature = "default")]
#[tokio::test]
async fn can_restart_node() {
    use tempfile::TempDir;
    let tmp_dir = TempDir::new().unwrap();

    // start node once
    {
        use fuel_core::service::ServiceTrait;
        let database = Database::open_rocksdb(tmp_dir.path(), None).unwrap();
        let first_startup = FuelService::from_database(database, Config::local_node())
            .await
            .unwrap();
        first_startup.stop_and_await().await.unwrap();
    }

    {
        let database = Database::open_rocksdb(tmp_dir.path(), None).unwrap();
        let _second_startup = FuelService::from_database(database, Config::local_node())
            .await
            .unwrap();
    }
}

#[traced_test]
#[tokio::test]
async fn can_restart_node_with_data() {
    use fuel_core::service::ServiceTrait;
    use tempfile::TempDir;

    let tmp_dir = TempDir::new().unwrap();

    {
        let database = CombinedDatabase::open(tmp_dir.path(), 10 * 1024 * 1024).unwrap();
        let service = FuelService::from_combined_database(database, Config::local_node())
            .await
            .unwrap();
        let client = FuelClient::from(service.bound_address);
        client.health().await.unwrap();

        let tx = Transaction::default_test_tx();
        client.submit_and_await_commit(&tx).await.unwrap();
        let tx = Transaction::default_test_tx();
        client.submit_and_await_commit(&tx).await.unwrap();
        let tx = Transaction::default_test_tx();
        client.submit_and_await_commit(&tx).await.unwrap();
        let tx = Transaction::default_test_tx();
        client.submit_and_await_commit(&tx).await.unwrap();

        service.stop_and_await().await.unwrap();
    }

    {
        let database = CombinedDatabase::open(tmp_dir.path(), 10 * 1024 * 1024).unwrap();
        let service = FuelService::from_combined_database(database, Config::local_node())
            .await
            .unwrap();
        let client = FuelClient::from(service.bound_address);
        client.health().await.unwrap();
    }
}
