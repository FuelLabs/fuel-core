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

#[tokio::test]
async fn balance_with_block_height_header() {
    let owner = Address::default();
    let asset_id = AssetId::BASE;

    // setup config
    let state_config = StateConfig::default();
    let config = Config::local_node_with_state_config(state_config);

    // setup server & client
    let srv = FuelService::new_node(config).await.unwrap();
    let mut client: FuelClient = FuelClient::from(srv.bound_address);

    client
        .set_header("REQUIRED_FUEL_BLOCK_HEIGHT", "100")
        .unwrap();

    // Issue a request with wrong precondition
    let error = client.balance(&owner, Some(&asset_id)).await.unwrap_err();

    let error_str = format!("{:?}", error);
    assert_eq!(
        error_str,
        "Custom { kind: Other, error: ErrorResponse(412, \"\\\"Required fuel block height is too far in the future\\\"\") }"
    );

    // Disable HEADER, otherwise requests fail with status code 412
    client.remove_header("REQUIRED_FUEL_BLOCK_HEIGHT");
    // Meet precondition on server side
    client.produce_blocks(100, None).await.unwrap();

    // Set the header and issue request again
    client
        .set_header("REQUIRED_FUEL_BLOCK_HEIGHT", "100")
        .unwrap();
    let result = client.balance(&owner, Some(&asset_id)).await;

    assert!(result.is_ok());
}
