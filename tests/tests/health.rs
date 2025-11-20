use fuel_core::{
    combined_database::CombinedDatabase,
    database::Database,
    service::{
        Config,
        FuelService,
    },
    state::rocks_db::DatabaseConfig,
};
use fuel_core_client::client::FuelClient;
use fuel_core_types::fuel_asm::op;
use test_helpers::{
    assemble_tx::AssembleAndRunTx,
    config_with_fee,
    default_signing_wallet,
};

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
        let database = Database::open_rocksdb(
            tmp_dir.path(),
            Default::default(),
            DatabaseConfig::config_for_tests(),
        )
        .unwrap();
        let first_startup = FuelService::from_database(database, Config::local_node())
            .await
            .unwrap();
        first_startup
            .send_stop_signal_and_await_shutdown()
            .await
            .unwrap();
    }

    {
        let database = Database::open_rocksdb(
            tmp_dir.path(),
            Default::default(),
            DatabaseConfig::config_for_tests(),
        )
        .unwrap();
        let _second_startup = FuelService::from_database(database, Config::local_node())
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn can_restart_node_with_transactions() {
    let tmp_dir = tempfile::TempDir::new().unwrap();

    {
        // Given
        let database = CombinedDatabase::open(
            tmp_dir.path(),
            Default::default(),
            DatabaseConfig::config_for_tests(),
        )
        .unwrap();
        let service = FuelService::from_combined_database(database, config_with_fee())
            .await
            .unwrap();
        let client = FuelClient::from(service.bound_address);
        client.health().await.unwrap();

        for _ in 0..5 {
            client
                .run_script(vec![op::ret(1)], vec![], default_signing_wallet())
                .await
                .unwrap();
        }

        service.send_stop_signal_and_await_shutdown().await.unwrap();
    }

    {
        // When
        let database = CombinedDatabase::open(
            tmp_dir.path(),
            Default::default(),
            DatabaseConfig::config_for_tests(),
        )
        .unwrap();
        let service = FuelService::from_combined_database(database, Config::local_node())
            .await
            .unwrap();
        let client = FuelClient::from(service.bound_address);

        // Then
        client.health().await.unwrap();
        service.send_stop_signal_and_await_shutdown().await.unwrap();
    }
}
