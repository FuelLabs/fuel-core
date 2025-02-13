use fuel_core::{
    chain_config::StateConfig,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_client::client::{
    types::primitives::{
        Address,
        AssetId,
    },
    FuelClient,
};
use fuel_core_types::fuel_tx;

#[tokio::test]
async fn request_with_required_block_height_extension_field_works() {
    let owner = Address::default();
    let asset_id = AssetId::BASE;

    // setup config
    let state_config = StateConfig::default();
    let config = Config::local_node_with_state_config(state_config);

    // setup server & client
    let srv = FuelService::new_node(config).await.unwrap();
    let mut client: FuelClient = FuelClient::from(srv.bound_address);

    client.with_required_fuel_block_height(Some(100u32.into()));
    // Issue a request with wrong precondition
    let error = client.balance(&owner, Some(&asset_id)).await.unwrap_err();

    assert!(
        error
            .to_string()
            .contains("The required block height was not met"),
        "Error: {}",
        error
    );

    // Disable extension metadata, otherwise the request fails
    client.with_required_fuel_block_height(None);

    // Meet precondition on server side
    client.produce_blocks(100, None).await.unwrap();

    // Set the header and issue request again
    client.with_required_fuel_block_height(Some(100u32.into()));
    let result = client.balance(&owner, Some(&asset_id)).await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn current_fuel_block_height_extension_fields_are_present_on_failed_request() {
    // setup config
    let state_config = StateConfig::default();
    let mut config = Config::local_node_with_state_config(state_config);
    // It will cause request to fail.
    config.debug = false;

    // setup server & client
    let srv = FuelService::new_node(config).await.unwrap();
    let mut client: FuelClient = FuelClient::from(srv.bound_address);

    // When
    client.with_required_fuel_block_height(None);
    let result = client.produce_blocks(50, None).await;

    // Then
    assert!(result.is_err());
    assert_eq!(client.required_block_height(), Some(0u32.into()));
}

#[tokio::test]
async fn current_fuel_block_height_header_is_present_on_successful_request() {
    // setup config
    let state_config = StateConfig::default();
    let config = Config::local_node_with_state_config(state_config);

    // setup server & client
    let srv = FuelService::new_node(config).await.unwrap();
    let mut client: FuelClient = FuelClient::from(srv.bound_address);

    // When
    client.with_required_fuel_block_height(None);
    client.produce_blocks(50, None).await.unwrap();

    // Then
    assert_eq!(client.required_block_height(), Some(50u32.into()));
}

#[tokio::test]
async fn current_fuel_block_height_header_is_present_on_successful_committed_transaction()
{
    // setup config
    let state_config = StateConfig::default();
    let config = Config::local_node_with_state_config(state_config);

    // setup server & client
    let srv = FuelService::new_node(config).await.unwrap();
    let mut client: FuelClient = FuelClient::from(srv.bound_address);

    // When
    client.with_required_fuel_block_height(None);
    let tx = fuel_tx::Transaction::default_test_tx();
    client.submit_and_await_commit(&tx).await.unwrap();

    // Then
    assert_eq!(client.required_block_height(), Some(1u32.into()));
}

#[tokio::test]
async fn submitting_transaction_with_future_required_height_return_error() {
    // setup config
    let state_config = StateConfig::default();
    let config = Config::local_node_with_state_config(state_config);

    // setup server & client
    let srv = FuelService::new_node(config).await.unwrap();
    let mut client: FuelClient = FuelClient::from(srv.bound_address);

    // When
    client.with_required_fuel_block_height(Some(100u32.into()));
    let tx = fuel_tx::Transaction::default_test_tx();
    let result = client.submit_and_await_commit(&tx).await;

    // Then
    let error = result.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("The required block height was not met"),
        "Error: {}",
        error
    );
}
