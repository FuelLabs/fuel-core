use super::*;
use crate::{
    chain_config::{
        BlobConfig,
        ContractConfig,
        StateConfig,
    },
    database::Database,
    service::{
        Config as NodeConfig,
        FuelService,
    },
};
use fuel_core_types::fuel_types::{
    BlobId,
    ContractId,
};
use tokio::io::{
    AsyncReadExt,
    AsyncWriteExt,
};

async fn request(
    address: SocketAddr,
    method: &str,
    parameters: serde_json::Value,
) -> (String, serde_json::Value) {
    let (path, body) = if method == "GET" {
        let mut query = url::form_urlencoded::Serializer::new(String::new());
        for (name, value) in parameters.as_object().unwrap() {
            query.append_pair(
                name,
                &value
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| value.to_string()),
            );
        }
        (format!("/v1/graphql?{}", query.finish()), String::new())
    } else {
        ("/v1/graphql".to_string(), parameters.to_string())
    };
    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    stream.write_all(format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len(),
    ).as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let response = String::from_utf8(bytes).unwrap();
    let (headers, body) = response.split_once("\r\n\r\n").unwrap();
    (
        headers.to_ascii_lowercase(),
        serde_json::from_str(body).unwrap(),
    )
}

#[tokio::test]
async fn cache_control_http_historical_and_mutable_queries() {
    let blob_id = BlobId::new([1; 32]);
    let contract_id = ContractId::new([2; 32]);
    let config = NodeConfig::local_node_with_state_config(StateConfig {
        blobs: vec![BlobConfig {
            blob_id,
            payload: vec![1, 2, 3],
        }],
        contracts: vec![ContractConfig {
            contract_id,
            code: vec![4, 5, 6],
            ..Default::default()
        }],
        ..StateConfig::local_testnet()
    });
    let service = FuelService::from_database(Database::default(), config)
        .await
        .unwrap();
    let address = service.bound_address;

    for method in ["GET", "POST"] {
        for query in [
            "{ __typename queryType: __typename block(height: \"0\") { __typename objectType: __typename id header { __typename height txIdCommitment } consensus { __typename unionType: __typename ... on Genesis { __typename chainConfigHash } } transactionIds } }".to_string(),
            format!("{{ __typename queryType: __typename blob(id: \"{blob_id:#x}\") {{ __typename id bytecode }} }}"),
            format!("{{ __typename queryType: __typename contract(id: \"{contract_id:#x}\") {{ __typename id bytecode }} }}"),
        ] {
            let (headers, body) = request(address, method, json!({"query": query})).await;
            assert!(headers.starts_with("http/1.1 200"), "{headers}");
            assert!(headers.contains("cache-control: public, max-age=60, must-revalidate"), "{query}: {headers} {body}");
            assert!(body.get("errors").is_none(), "{body}");
            assert_eq!(body["data"]["__typename"], "Query");
            assert_eq!(body["data"]["queryType"], "Query");
            if let Some(block) = body["data"].get("block") {
                assert_eq!(block["__typename"], "Block");
                assert_eq!(block["objectType"], "Block");
                assert_eq!(block["header"]["__typename"], "Header");
                assert_eq!(block["consensus"]["__typename"], "Genesis");
                assert_eq!(block["consensus"]["unionType"], "Genesis");
            } else if let Some(blob) = body["data"].get("blob") {
                assert_eq!(blob["__typename"], "Blob");
            } else {
                assert_eq!(body["data"]["contract"]["__typename"], "Contract");
            }
            assert!(body["extensions"]["current_fuel_block_height"].is_string() || body["extensions"]["current_fuel_block_height"].is_number(), "{body}");
        }
        for query in [
            "{ block(height: \"0\") { id } health }",
            "{ __typename block(height: \"0\") { __typename id } health }",
            "{ block(height: \"0\") { __typename id } __schema { queryType { name } } }",
            "{ block(height: \"0\") { transactions { id } } }",
            "{ block(height: \"999\") { id } }",
            "{ block { id } }",
            "{ block(height: \"0\") { id } chain { latestBlock { height } } }",
            "{ block(height: \"0\") { id } blocks(first: 1) { edges { node { id } } } }",
            "{ blob(id: \"0x0000000000000000000000000000000000000000000000000000000000000000\") { id } }",
            "{ contract(id: \"0x0000000000000000000000000000000000000000000000000000000000000000\") { id } }",
        ] {
            let (headers, _) = request(address, method, json!({"query": query})).await;
            assert!(
                headers.contains("cache-control: no-store"),
                "{query}: {headers}"
            );
        }

        let (headers, body) = request(address, method, json!({
            "query": "query Historical($height: U32!) { historical: block(height: $height) { ...Fields } } fragment Fields on Block { header { height txIdCommitment } } mutation Unused { produceBlocks(blocksToProduce: \"1\") }",
            "operationName": "Historical",
            "variables": {"height": "0"},
        })).await;
        assert!(
            headers.contains("cache-control: public, max-age=60, must-revalidate"),
            "{headers} {body}"
        );
        assert_eq!(body["data"]["historical"]["header"]["height"], "0");

        if method == "GET" {
            let (headers, body) = request(
                address,
                method,
                json!({
                    "query": "{ block(height: \"0\") { id } }",
                    "operationName": "",
                    "variables": "",
                    "extensions": "",
                }),
            )
            .await;
            assert!(
                headers.contains("cache-control: public, max-age=60, must-revalidate"),
                "{headers} {body}"
            );
            assert!(body.get("errors").is_none(), "{body}");
        }

        let (headers, body) = request(
            address,
            method,
            json!({
                "query": "{ block(height: \"0\") { id } }",
                "extensions": {"required_fuel_block_height": 0},
            }),
        )
        .await;
        assert!(headers.contains("cache-control: no-store"), "{headers}");
        assert!(body.get("errors").is_none(), "{body}");

        let (headers, body) = request(
            address,
            method,
            json!({
                "query": "{ block(height: \"0\") { id } }",
                "extensions": {"required_fuel_block_height": 999999},
            }),
        )
        .await;
        assert!(headers.contains("cache-control: no-store"), "{headers}");
        assert!(body.get("errors").is_some(), "{body}");
    }
    service.send_stop_signal_and_await_shutdown().await.unwrap();
}

#[tokio::test]
async fn cache_control_get_rejects_mutations_before_execution() {
    let service =
        FuelService::from_database(Database::default(), NodeConfig::local_node())
            .await
            .unwrap();
    let address = service.bound_address;
    let (headers, body) = request(address, "GET", json!({
        "query": "query Read { block(height: \"0\") { id } } mutation Write { produceBlocks(blocksToProduce: \"1\") }",
        "operationName": "Write",
    })).await;
    assert!(headers.starts_with("http/1.1 405"), "{headers} {body}");
    assert!(headers.contains("allow: post"), "{headers}");
    assert!(headers.contains("cache-control: no-store"), "{headers}");

    for parameters in [
        json!({"query": "query A { health } mutation B { produceBlocks(blocksToProduce: \"1\") }"}),
        json!({"query": "mutation B { produceBlocks(blocksToProduce: \"1\") }", "operationName": "Unknown"}),
        json!({"query": "{ health }", "variables": "bad JSON"}),
        json!({"query": "{"}),
        json!({"query": "subscription { statusChange(id: \"0x00\") { __typename } }"}),
    ] {
        let (headers, body) = request(address, "GET", parameters).await;
        assert!(headers.contains("cache-control: no-store"), "{headers}");
        assert!(body.get("errors").is_some(), "{body}");
    }

    let (_, body) = request(
        address,
        "POST",
        json!({"query": "{ chain { latestBlock { height } } }"}),
    )
    .await;
    assert_eq!(body["data"]["chain"]["latestBlock"]["height"], "0");
    let (headers, body) = request(
        address,
        "POST",
        json!({"query": "mutation { produceBlocks(blocksToProduce: \"1\") }"}),
    )
    .await;
    assert!(headers.contains("cache-control: no-store"), "{headers}");
    assert_eq!(body["data"]["produceBlocks"], "1", "{body}");
    service.send_stop_signal_and_await_shutdown().await.unwrap();
}

#[test]
fn cache_control_http_preserves_private_scope_and_shorter_lifetime() {
    let response = Response::new(async_graphql::value!({"value": 1})).cache_control(
        async_graphql::CacheControl {
            public: false,
            max_age: 30,
        },
    );
    let response = graphql_response(response);
    assert_eq!(
        response.headers()[CACHE_CONTROL],
        "private, max-age=30, must-revalidate"
    );
}
