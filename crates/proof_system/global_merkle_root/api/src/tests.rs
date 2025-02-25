#![allow(non_snake_case)]

use std::{
    collections::BTreeMap,
    net::SocketAddr,
};

use async_graphql::{
    EmptyMutation,
    EmptySubscription,
};
use fuel_core_services::Service;
use fuel_core_types::{
    fuel_tx::Bytes32,
    fuel_types::BlockHeight,
};

use crate::{
    ports::GetStateRoot,
    schema::{
        Query,
        Schema,
    },
    service,
};

#[tokio::test]
async fn schema__can_execute_valid_state_root_query() {
    // Given
    let block_height = 1337;
    let mut storage = TestStorage::default();

    storage.insert_zeroed_root_at(block_height.into());

    let query = Query::new(storage);
    let schema = Schema::build(query, EmptyMutation, EmptySubscription).finish();
    let request = "{ stateRoot(height: \"$\") }".replace("$", &block_height.to_string());

    // When
    let response = schema.execute(request).await;

    // Then
    assert!(response.errors.is_empty());
    assert_eq!(
        response.data.to_string(),
        r#"{stateRoot: "0x0000000000000000000000000000000000000000000000000000000000000000"}"#
    );
}

#[tokio::test]
async fn schema__returns_error_on_invalid_state_root_query() {
    // Given
    let block_height = 1337;
    let mut storage = TestStorage::default();

    storage.insert_zeroed_root_at(block_height.into());

    let query = Query::new(storage);
    let schema = Schema::build(query, EmptyMutation, EmptySubscription).finish();
    let request = "{ stateGroot(eight: \"$\") }".replace("$", &block_height.to_string());

    // When
    let response = schema.execute(request).await;

    // Then
    assert_eq!(response.errors.len(), 1);
    assert_eq!(response.data.to_string(), "null");
}

#[tokio::test]
async fn service__can_serve_api() {
    // Given
    let block_height = 1337;
    let mut storage = TestStorage::default();

    storage.insert_zeroed_root_at(block_height.into());

    let network_address = "127.0.0.1:0";

    let service = service::new_service(storage, network_address).unwrap();
    let client = TestClient::new(service.shared.bound_address);

    // When
    service.start_and_await().await.unwrap();
    let response = client.get_root_response_at(block_height).await;

    // Then
    assert_eq!(
        response,
        r#"{"data":{"stateRoot":"0x0000000000000000000000000000000000000000000000000000000000000000"}}"#
    );
}

#[derive(Default)]
struct TestStorage {
    state_roots: BTreeMap<BlockHeight, Bytes32>,
}

impl TestStorage {
    fn insert_zeroed_root_at(&mut self, height: BlockHeight) {
        self.state_roots.insert(height, Bytes32::zeroed());
    }
}

impl GetStateRoot for TestStorage {
    fn state_root_at(
        &self,
        height: BlockHeight,
    ) -> Result<Option<Bytes32>, fuel_core_storage::Error> {
        Ok(self.state_roots.get(&height).copied())
    }
}

struct TestClient {
    address: SocketAddr,
    client: reqwest::Client,
}

impl TestClient {
    fn new(address: SocketAddr) -> Self {
        Self {
            address,
            client: reqwest::Client::new(),
        }
    }

    async fn get_root_response_at(&self, block_height: u32) -> String {
        let url = format!("http://{}/graphql", self.address);
        let resp = self
            .client
            .post(url)
            .header("Content-Type", "application/json")
            .body(
                r#"{"query":"{stateRoot(height: \"$\")}"}"#
                    .replace("$", &block_height.to_string()),
            )
            .send()
            .await
            .unwrap();

        resp.text().await.unwrap()
    }
}
