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
use reqwest::{
    header::CONTENT_TYPE,
    StatusCode,
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
        "Custom { kind: Other, error: ErrorResponse(412, \"\") }"
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

#[tokio::test]
async fn current_fuel_block_height_header_is_present_on_successful_request() {
    let query = r#"{ "query": "{ contract(id:\"0x7e2becd64cd598da59b4d1064b711661898656c6b1f4918a787156b8965dc83c\") { id bytecode } }", "extensions": {"required_fuel_block_height": 100} }"#;

    // setup config
    let state_config = StateConfig::default();
    let config = Config::local_node_with_state_config(state_config);

    // setup server & client
    let srv = FuelService::new_node(config).await.unwrap();
    let url: reqwest::Url = format!("http://{}/v1/graphql", srv.bound_address)
        .as_str()
        .parse()
        .unwrap();

    let client = reqwest::Client::new();

    let request = client
        .post(url)
        .body(query)
        .header(CONTENT_TYPE, "application/json")
        .build()
        .unwrap();
    let response = client.execute(request).await.unwrap();

    assert!(response.status() == StatusCode::OK);
    let response_body = response.text().await.unwrap();
    println!("{response_body:?}");
}

#[tokio::test]
async fn current_fuel_block_height_header_is_present_on_failed_request() {
    let query = r#"{ "query": "{ contract(id:\"0x7e2becd64cd598da59b4d1064b711661898656c6b1f4918a787156b8965dc83c\") { id bytecode } }" }"#;

    // setup config
    let state_config = StateConfig::default();
    let config = Config::local_node_with_state_config(state_config);

    // setup server & client
    let srv = FuelService::new_node(config).await.unwrap();
    let url: reqwest::Url = format!("http://{}/v1/graphql", srv.bound_address)
        .as_str()
        .parse()
        .unwrap();

    let client = reqwest::Client::new();

    let request = client
        .post(url)
        .body(query)
        .header("REQUIRED_FUEL_BLOCK_HEIGHT", "100")
        .header(CONTENT_TYPE, "application/json")
        .build()
        .unwrap();
    let response = client.execute(request).await.unwrap();

    assert!(response.status() == StatusCode::PRECONDITION_FAILED);
    assert!(response.headers().contains_key("CURRENT_FUEL_BLOCK_HEIGHT"));
}
