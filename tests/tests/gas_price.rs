use fuel_core::service::{
    Config,
    FuelService,
};
use fuel_core_client::client::{
    schema::gas_price::EstimateGasPrice,
    types::gas_price::LatestGasPrice,
    FuelClient,
};

#[tokio::test]
async fn latest_gas_price() {
    let node_config = Config::local_node();
    let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let LatestGasPrice { gas_price, .. } = client.latest_gas_price().await.unwrap();
    assert_eq!(gas_price, node_config.txpool.min_gas_price);
}

#[tokio::test]
async fn estimate_gas_price() {
    let node_config = Config::local_node();
    let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let arbitrary_horizon = 10;

    let EstimateGasPrice { gas_price } =
        client.estimate_gas_price(arbitrary_horizon).await.unwrap();
    assert_eq!(u64::from(gas_price), node_config.txpool.min_gas_price);
}
