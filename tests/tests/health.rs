use fuel_core::service::{
    Config,
    FuelService,
};
use fuel_core_client::client::FuelClient;

#[tokio::test]
async fn health() {
    let srv = FuelService::new_node(Config::local_node()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let health = client.health().await.unwrap();
    assert!(health);
}
