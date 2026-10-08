use fuel_core::service::{
    Config,
    FuelService,
};
use fuel_core_client::client::FuelClient;
use fuel_core_poa::Trigger;
use reqwest::header::{
    ACCEPT_ENCODING,
    CONTENT_ENCODING,
    CONTENT_TYPE,
};
use std::{
    io::Read,
    time::Duration,
};

const SCHEMA_QUERY: &str = r#"
    query {
        __schema {
            types {
                name
                description
                fields {
                    name
                    description
                }
            }
        }
    }
"#;

fn query_body(query: &str) -> serde_json::Value {
    serde_json::json!({ "query": query })
}

fn plain_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .no_zstd()
        .build()
        .unwrap()
}

#[tokio::test]
async fn graphql_response_is_gzip_compressed_when_client_accepts_gzip() {
    // Given
    let node = FuelService::new_node(Config::local_node()).await.unwrap();
    let url = format!("http://{}/v1/graphql", node.bound_address);

    // When
    let response = plain_http_client()
        .post(&url)
        .header(ACCEPT_ENCODING, "gzip")
        .json(&query_body(SCHEMA_QUERY))
        .send()
        .await
        .unwrap();

    // Then
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers().get(CONTENT_ENCODING).unwrap(),
        "gzip",
        "response should be gzip encoded"
    );
    let compressed = response.bytes().await.unwrap();
    let mut decoded = Vec::new();
    flate2::read::GzDecoder::new(compressed.as_ref())
        .read_to_end(&mut decoded)
        .unwrap();
    assert!(compressed.len() < decoded.len());
    let json: serde_json::Value = serde_json::from_slice(&decoded).unwrap();
    assert!(json["data"]["__schema"]["types"].is_array());
}

#[tokio::test]
async fn graphql_response_is_not_compressed_without_accept_encoding() {
    // Given
    let node = FuelService::new_node(Config::local_node()).await.unwrap();
    let url = format!("http://{}/v1/graphql", node.bound_address);

    // When
    let response = plain_http_client()
        .post(&url)
        .json(&query_body(SCHEMA_QUERY))
        .send()
        .await
        .unwrap();

    // Then
    assert_eq!(response.status(), 200);
    assert!(response.headers().get(CONTENT_ENCODING).is_none());
    let json: serde_json::Value = response.json().await.unwrap();
    assert!(json["data"]["__schema"]["types"].is_array());
}

#[tokio::test]
async fn graphql_subscription_is_not_compressed_and_streams_events() {
    // Given
    let mut config = Config::local_node();
    config.block_production = Trigger::Instant;
    config.debug = true;
    let node = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(node.bound_address);
    let url = format!("http://{}/v1/graphql-sub", node.bound_address);
    let mut response = plain_http_client()
        .post(&url)
        .header(ACCEPT_ENCODING, "gzip")
        .json(&query_body("subscription { alpha__new_blocks }"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers().get(CONTENT_TYPE).unwrap(),
        "text/event-stream"
    );
    assert!(response.headers().get(CONTENT_ENCODING).is_none());
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // When
    client.produce_blocks(1, None).await.unwrap();

    // Then
    let event = tokio::time::timeout(Duration::from_secs(10), async {
        let mut received = String::new();
        while let Some(chunk) = response.chunk().await.unwrap() {
            received.push_str(std::str::from_utf8(&chunk).unwrap());
            if received.contains("alpha__new_blocks") {
                return received;
            }
        }
        panic!("subscription stream ended before the block event");
    })
    .await
    .expect("block event should arrive without buffering");
    assert!(event.starts_with("data:"));
}
