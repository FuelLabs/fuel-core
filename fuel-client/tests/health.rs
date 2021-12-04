use fuel_client::client::FuelClient;
use fuel_core::service::{Config, FuelService};

#[tokio::test]
async fn health() {
    let srv = FuelService::new_node(Config::local_node()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let health = client.health().await.unwrap();
    assert!(health);
}
