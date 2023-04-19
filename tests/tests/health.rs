use fuel_core::{
    database::Database,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_client::client::FuelClient;

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
        let database = Database::open(tmp_dir.path(), None).unwrap();
        let first_startup = FuelService::from_database(database, Config::local_node())
            .await
            .unwrap();
        first_startup.stop_and_await().await.unwrap();
    }

    {
        let database = Database::open(tmp_dir.path(), None).unwrap();
        let _second_startup = FuelService::from_database(database, Config::local_node())
            .await
            .unwrap();
    }
}
