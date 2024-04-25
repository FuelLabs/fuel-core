use fuel_core::{
    combined_database::CombinedDatabase,
    database::{
        database_description::relayer::Relayer,
        Database,
    },
    service::{
        Config,
        FuelService,
        ServiceTrait,
    },
    types::fuel_tx::Transaction,
};
use fuel_core_client::client::FuelClient;
use fuel_core_relayer::storage::EventsHistory;
use fuel_core_storage::StorageAsMut;
use fuel_core_types::{
    entities::Message,
    fuel_types::Nonce,
    services::relayer::Event,
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

#[tokio::test]
async fn can_restart_node_with_transactions() {
    let capacity = 1024 * 1024;
    let tmp_dir = tempfile::TempDir::new().unwrap();

    {
        // Given
        let database = CombinedDatabase::open(tmp_dir.path(), capacity).unwrap();
        let service = FuelService::from_combined_database(database, Config::local_node())
            .await
            .unwrap();
        let client = FuelClient::from(service.bound_address);
        client.health().await.unwrap();

        for _ in 0..5 {
            let tx = Transaction::default_test_tx();
            client.submit_and_await_commit(&tx).await.unwrap();
        }

        service.stop_and_await().await.unwrap();
    }

    {
        // When
        let database = CombinedDatabase::open(tmp_dir.path(), capacity).unwrap();
        let service = FuelService::from_combined_database(database, Config::local_node())
            .await
            .unwrap();
        let client = FuelClient::from(service.bound_address);

        // Then
        client.health().await.unwrap();
        service.stop_and_await().await.unwrap();
    }
}

#[tokio::test]
async fn can_restart_node_with_relayer_data() {
    let capacity = 1024 * 1024;
    let tmp_dir = tempfile::TempDir::new().unwrap();

    fn add_message_to_relayer(db: &mut Database<Relayer>, message: Message) {
        let da_height = message.da_height();
        db.storage::<EventsHistory>()
            .insert(&da_height, &[Event::Message(message)])
            .expect("Should insert event");
    }

    {
        // Given
        let mut database = CombinedDatabase::open(tmp_dir.path(), capacity).unwrap();

        let block_da_height = 1000u64;
        let nonce = Nonce::default();
        let mut message = Message::default();
        message.set_da_height(block_da_height.into());
        message.set_nonce(nonce);

        add_message_to_relayer(database.relayer_mut(), message);

        let service = FuelService::from_combined_database(database, Config::local_node())
            .await
            .unwrap();
        let client = FuelClient::from(service.bound_address);
        client.health().await.unwrap();

        for _ in 0..5 {
            let tx = Transaction::default_test_tx();
            client.submit_and_await_commit(&tx).await.unwrap();
        }

        service.stop_and_await().await.unwrap();
    }

    {
        // When
        let database = CombinedDatabase::open(tmp_dir.path(), capacity).unwrap();
        let service = FuelService::from_combined_database(database, Config::local_node())
            .await
            .unwrap();
        let client = FuelClient::from(service.bound_address);

        // Then
        client.health().await.unwrap();
        service.stop_and_await().await.unwrap();
    }
}
