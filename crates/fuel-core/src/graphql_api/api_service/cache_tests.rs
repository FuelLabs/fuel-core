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
use fuel_core_storage::{
    StorageAsRef,
    tables::Transactions,
};
use fuel_core_types::{
    fuel_tx::{
        Transaction,
        UniqueIdentifier,
    },
    fuel_types::{
        BlobId,
        ContractId,
        canonical::Serialize,
    },
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
async fn cache_control_http_committed_transaction_payloads() {
    let mut config = NodeConfig::local_node();
    config.block_production = fuel_core_poa::Trigger::Never;
    let chain_id = config.chain_id();
    let database = Database::default();
    let service = FuelService::from_database(database.clone(), config)
        .await
        .unwrap();
    let address = service.bound_address;

    let transaction = Transaction::default_test_tx();
    let tx_id = transaction.id(&chain_id);
    let pending_payload =
        crate::schema::scalars::HexString(transaction.to_bytes()).to_string();
    let by_id = format!("{{ transaction(id: \"{tx_id:#x}\") {{ id rawPayload }} }}");
    let (headers, body) = request(
        address,
        "POST",
        json!({
            "query": format!("mutation {{ submit(tx: \"{pending_payload}\") {{ id }} }}"),
        }),
    )
    .await;
    assert!(body.get("errors").is_none(), "{body}");
    assert_eq!(body["data"]["submit"]["id"], format!("{tx_id:#x}"));
    assert!(headers.contains("cache-control: no-store"), "{headers}");

    // A pool transaction's execution fields can still change under the same ID.
    for method in ["GET", "POST"] {
        let (headers, body) = request(address, method, json!({"query": by_id})).await;
        assert!(body.get("errors").is_none(), "{body}");
        assert!(headers.contains("cache-control: no-store"), "{headers}");
        assert_eq!(body["data"]["transaction"]["rawPayload"], pending_payload);
    }

    let (_, body) = request(
        address,
        "POST",
        json!({
            "query": "mutation { produceBlocks(blocksToProduce: \"1\") }",
        }),
    )
    .await;
    assert!(body.get("errors").is_none(), "{body}");
    assert_eq!(body["data"]["produceBlocks"], "1");

    let stored = database
        .storage::<Transactions>()
        .get(&tx_id)
        .unwrap()
        .unwrap()
        .into_owned();
    assert_eq!(stored.id(&chain_id), tx_id);
    let committed_payload =
        crate::schema::scalars::HexString(stored.to_bytes()).to_string();
    assert_ne!(pending_payload, committed_payload);

    for method in ["GET", "POST"] {
        let (headers, body) = request(address, method, json!({
            "query": "query Payloads($height: U32!) { block(height: $height) { transactionIds transactions { ...Payload } } } fragment Payload on Transaction { __typename id rawPayload }",
            "operationName": "Payloads",
            "variables": {"height": "1"},
        })).await;
        assert!(body.get("errors").is_none(), "{body}");
        assert!(
            headers.contains("cache-control: public, max-age=60, must-revalidate"),
            "{headers} {body}"
        );
        let block = &body["data"]["block"];
        let ids = block["transactionIds"].as_array().unwrap();
        let transactions = block["transactions"].as_array().unwrap();
        assert!(!transactions.is_empty());
        assert_eq!(transactions.len(), ids.len());
        for (transaction, id) in transactions.iter().zip(ids) {
            assert_eq!(transaction["__typename"], "Transaction");
            assert_eq!(&transaction["id"], id);
            let id = id.as_str().unwrap().parse().unwrap();
            let stored = database
                .storage::<Transactions>()
                .get(&id)
                .unwrap()
                .unwrap()
                .into_owned();
            assert_eq!(
                transaction["rawPayload"],
                crate::schema::scalars::HexString(stored.to_bytes()).to_string()
            );
        }

        // Direct lookups keep their conservative policy even after commitment.
        let (headers, body) = request(address, method, json!({"query": by_id})).await;
        assert!(body.get("errors").is_none(), "{body}");
        assert!(headers.contains("cache-control: no-store"), "{headers}");
        assert_eq!(body["data"]["transaction"]["rawPayload"], committed_payload);

        for query in [
            "{ block(height: \"1\") { transactions { id rawPayload status { __typename } } } }",
            "{ block(height: \"1\") { transactions { id rawPayload inputAssetIds } } }",
            "{ block(height: \"1\") { transactions { id rawPayload policies { tip } } } }",
            "{ block(height: \"1\") { transactions { id rawPayload } } health }",
            "{ transactions(first: 1) { edges { node { id rawPayload } } } }",
        ] {
            let (headers, body) = request(address, method, json!({"query": query})).await;
            assert!(body.get("errors").is_none(), "{query}: {body}");
            assert!(
                headers.contains("cache-control: no-store"),
                "{query}: {headers}"
            );
        }

        let (headers, body) = request(
            address,
            method,
            json!({
                "query": "{ block(height: \"999\") { transactions { id rawPayload } } }",
            }),
        )
        .await;
        assert!(body.get("errors").is_none(), "{body}");
        assert!(body["data"]["block"].is_null());
        assert!(headers.contains("cache-control: no-store"), "{headers}");

        let (headers, body) = request(
            address,
            method,
            json!({
                "query": "{ block(height: \"1\") { transactions { id rawPayload } } }",
                "extensions": {"required_fuel_block_height": 1},
            }),
        )
        .await;
        assert!(body.get("errors").is_none(), "{body}");
        assert!(headers.contains("cache-control: no-store"), "{headers}");
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
