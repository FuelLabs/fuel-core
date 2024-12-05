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
    let client = FuelClient::from(srv.bound_address);

    // Issue a request with wrong precondition
    let error = client
        .balance_with_required_block_header(&owner, Some(&asset_id), 100)
        .await
        .unwrap_err();

    let error_str = format!("{:?}", error);
    assert_eq!(
        error_str,
        "Custom { kind: Other, error: ErrorResponse(412, \"\") }"
    );

    // Meet precondition on server side
    client.produce_blocks(100, None).await.unwrap();

    // Issue request again
    let result = client
        .balance_with_required_block_header(&owner, Some(&asset_id), 100)
        .await;

    assert!(result.is_ok());
}
