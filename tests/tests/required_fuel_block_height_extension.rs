use std::time::Duration;

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
    let mut config = Config::local_node_with_state_config(state_config);

    let required_block_height = 100u32;
    // We want to verify that `required_block_height` block height requirements will be met before
    // transaction is actually inserted into the transaction pool.
    // Since `utxo_validation == true`, `bad_tx` should fail with error that UTXO doesn't exist.
    // So if we receive an error not related to required block height(error about missing UTXO),
    // it means that transaction was inserted before we met the requirement.
    let bad_tx = fuel_tx::Transaction::default_test_tx();
    config.utxo_validation = true;

    // setup server & client
    let srv = FuelService::new_node(config).await.unwrap();
    let mut client: FuelClient = FuelClient::from(srv.bound_address);

    // When
    client.with_required_fuel_block_height(Some(required_block_height.into()));
    let result = client.submit_and_await_commit(&bad_tx).await;

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

#[tokio::test]
async fn request_with_required_block_height_extension_waits_when_within_threshold() {
    let owner = Address::default();
    let asset_id = AssetId::BASE;

    // setup config
    let state_config = StateConfig::default();
    let mut config = Config::local_node_with_state_config(state_config);
    config.graphql_config.required_fuel_block_height_timeout = Duration::from_secs(30);
    config.graphql_config.required_fuel_block_height_tolerance = 5;

    // setup server & client
    let srv = FuelService::new_node(config).await.unwrap();
    let mut client: FuelClient = FuelClient::from(srv.bound_address);

    // Produce enough blocks to be within the tolerance for the node to wait to
    // reach the required fuel block height.
    client.produce_blocks(95, None).await.unwrap();

    let producer = client.clone();

    // Issue a request while the precondition on the required fuel block height is not met.
    let request_task = tokio::spawn(async move {
        client.with_required_fuel_block_height(Some(100u32.into()));
        client.balance(&owner, Some(&asset_id)).await
    });
    // Produce 5 blocks in parallel with the main test, to meet the precondition
    // on required fuel block height.
    let producer_task = tokio::spawn(async move {
        // Sleep to be sure that `request_task` will be processed before this task.
        tokio::time::sleep(Duration::from_secs(5)).await;
        producer.produce_blocks(5, None).await.unwrap();
    });

    let (Ok(result), _) = tokio::join!(request_task, producer_task) else {
        panic!("Request task failed");
    };

    assert!(result.is_ok());
}

#[tokio::test]
async fn request_with_required_block_height_extension_fails_when_timeout_while_within_threshold(
) {
    let owner = Address::default();
    let asset_id = AssetId::BASE;

    // setup config
    let state_config = StateConfig::default();
    let mut config = Config::local_node_with_state_config(state_config);
    config.graphql_config.required_fuel_block_height_timeout = Duration::from_secs(5);
    config.graphql_config.required_fuel_block_height_tolerance = 5;

    // setup server & client
    let srv = FuelService::new_node(config).await.unwrap();
    let mut client: FuelClient = FuelClient::from(srv.bound_address);

    // Produce enough blocks to be within the tolerance for the node to wait to
    // reach the required fuel block height.
    client.produce_blocks(95, None).await.unwrap();

    // Issue a request while the precondition on the required fuel block height is not met.
    client.with_required_fuel_block_height(Some(100u32.into()));
    let result = client.balance(&owner, Some(&asset_id)).await;

    assert!(result.is_err());
}
