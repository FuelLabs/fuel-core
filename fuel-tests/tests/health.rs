use fuel_core::{config::Config, service::FuelService};
use fuel_gql_client::client::FuelClient;

#[tokio::test]
async fn health() {
    let srv = FuelService::new_node(Config::local_node()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let health = client.health().await.unwrap();
    assert!(health);
}
