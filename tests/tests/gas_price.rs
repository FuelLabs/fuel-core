#![allow(non_snake_case)]
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
async fn latest_gas_price__should_be_static() {
    // given
    let node_config = Config::local_node();
    let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // when
    let LatestGasPrice { gas_price, .. } = client.latest_gas_price().await.unwrap();

    // then
    let expected = node_config.static_gas_price;
    let actual = gas_price;
    assert_eq!(expected, actual)
}

#[tokio::test]
async fn estimate_gas_price__should_be_static() {
    // given
    let node_config = Config::local_node();
    let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // when
    let arbitrary_horizon = 10;

    let EstimateGasPrice { gas_price } =
        client.estimate_gas_price(arbitrary_horizon).await.unwrap();

    // then
    let expected = node_config.static_gas_price;
    let actual = u64::from(gas_price);
    assert_eq!(expected, actual);
}
