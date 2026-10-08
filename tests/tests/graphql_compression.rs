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

#[tokio::test]
async fn fuel_client_requests_and_decodes_gzip_responses() {
    use tokio::io::{
        AsyncReadExt,
        AsyncWriteExt,
    };

    // Given
    let node = FuelService::new_node(Config::local_node()).await.unwrap();
    let node_addr = node.bound_address;
    let relay = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay_addr = relay.local_addr().unwrap();
    let observed = tokio::spawn(async move {
        let (mut client_side, _) = relay.accept().await.unwrap();
        let mut node_side = tokio::net::TcpStream::connect(node_addr).await.unwrap();
        let mut buf = vec![0u8; 64 * 1024];
        let mut request = Vec::new();
        while !request.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = client_side.read(&mut buf).await.unwrap();
            request.extend_from_slice(&buf[..n]);
        }
        node_side.write_all(&request).await.unwrap();
        let (mut client_read, mut client_write) = client_side.into_split();
        let (mut node_read, mut node_write) = node_side.into_split();
        tokio::spawn(async move {
            let _ = tokio::io::copy(&mut client_read, &mut node_write).await;
        });
        let mut response = Vec::new();
        while !response.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = node_read.read(&mut buf).await.unwrap();
            response.extend_from_slice(&buf[..n]);
        }
        client_write.write_all(&response).await.unwrap();
        tokio::spawn(async move {
            let _ = tokio::io::copy(&mut node_read, &mut client_write).await;
        });
        (
            String::from_utf8_lossy(&request).to_ascii_lowercase(),
            String::from_utf8_lossy(&response).to_ascii_lowercase(),
        )
    });
    let client = FuelClient::new(format!("http://{relay_addr}")).unwrap();

    // When
    let chain_info = client.chain_info().await.unwrap();

    // Then
    let (request, response) = observed.await.unwrap();
    assert!(request.contains("accept-encoding: gzip"), "{request}");
    assert!(response.contains("content-encoding: gzip"), "{response}");
    assert_eq!(chain_info.latest_block.header.height, 0);
}
