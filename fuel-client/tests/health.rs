use fuel_client::client::FuelClient;
use fuel_core::service::{configure, run_in_background};

#[tokio::test]
async fn health() {
    let srv = run_in_background(configure(Default::default())).await;
    let client = FuelClient::from(srv);

    let health = client.health().await.unwrap();
    assert!(health);
}
