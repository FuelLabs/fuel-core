#![allow(non_snake_case)]

use std::time::Instant;

use fuel_core::service::{
    Config,
    FuelService,
};
use fuel_core_client::client::FuelClient;
use fuel_core_types::blockchain::header::LATEST_STATE_TRANSITION_VERSION;
use test_helpers::send_graph_ql_query;

#[tokio::test]
async fn complex_queries__recursion() {
    let query = r#"
        query {
          chain {
            latestBlock {
              transactions {
                status {
                  ... on SuccessStatus {
                    block {
                      transactions {
                        status {
                          ... on SuccessStatus {
                            block {
                              transactions {
                                status {
                                  ... on SuccessStatus {
                                    block {
                                      transactions {
                                        status {
                                          ... on SuccessStatus {
                                            block {
                                              transactions {
                                                status {
                                                  ... on SuccessStatus {
                                                    block {
                                                      transactions {
                                                        status {
                                                          ... on SuccessStatus {
                                                            block {
                                                              transactions {
                                                                id
                                                              }
                                                            }
                                                          }
                                                        }
                                                      }
                                                    }
                                                  }
                                                }
                                              }
                                            }
                                          }
                                        }
                                      }
                                    }
                                  }
                                }
                              }
                            }
                          }
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
    "#;

    let mut config = Config::local_node();
    config.graphql_config.max_queries_complexity = usize::MAX;
    let node = FuelService::new_node(config).await.unwrap();
    let url = format!("http://{}/v1/graphql", node.bound_address);

    let result = send_graph_ql_query(&url, query).await;
    assert!(result.contains("The recursion depth of the query cannot be greater than"));
}

const FULL_BLOCK_QUERY: &str = r#"
    query {
      blocks(first: $NUMBER_OF_BLOCKS) {
        edges {
          cursor
          node {
            id
            header {
              id
              daHeight
              consensusParametersVersion
              stateTransitionBytecodeVersion
              transactionsCount
              messageReceiptCount
              transactionsRoot
              messageOutboxRoot
              eventInboxRoot
              height
              prevRoot
              time
              applicationHash
            }
            consensus {
              ... on Genesis {
                chainConfigHash
                coinsRoot
                contractsRoot
                messagesRoot
                transactionsRoot
              }
              ... on PoAConsensus {
                signature
              }
            }
            transactions {
              rawPayload
              status {
                ... on SubmittedStatus {
                  time
                }
                ... on SuccessStatus {
                  transactionId
                  blockHeight
                  time
                  programState {
                    returnType
                    data
                  }
                  receipts {
                    param1
                    param2
                    amount
                    assetId
                    gas
                    digest
                    id
                    is
                    pc
                    ptr
                    ra
                    rb
                    rc
                    rd
                    reason
                    receiptType
                    to
                    toAddress
                    val
                    len
                    result
                    gasUsed
                    data
                    sender
                    recipient
                    nonce
                    contractId
                    subId
                  }
                  totalGas
                  totalFee
                }
                ... on SqueezedOutStatus {
                  reason
                }
                ... on FailureStatus {
                  transactionId
                  blockHeight
                  time
                  reason
                  programState {
                    returnType
                    data
                  }
                  receipts {
                    param1
                    param2
                    amount
                    assetId
                    gas
                    digest
                    id
                    is
                    pc
                    ptr
                    ra
                    rb
                    rc
                    rd
                    reason
                    receiptType
                    to
                    toAddress
                    val
                    len
                    result
                    gasUsed
                    data
                    sender
                    recipient
                    nonce
                    contractId
                    subId
                  }
                  totalGas
                  totalFee
                }
              }
            }
          }
        }
        pageInfo {
          endCursor
          hasNextPage
          hasPreviousPage
          startCursor
        }
      }
    }
"#;

#[tokio::test]
async fn complex_queries__40_full_blocks__works() {
    let query = FULL_BLOCK_QUERY.to_string();
    let query = query.replace("$NUMBER_OF_BLOCKS", "40");

    let node = FuelService::new_node(Config::local_node()).await.unwrap();
    let url = format!("http://{}/v1/graphql", node.bound_address);

    let result = send_graph_ql_query(&url, query.as_str()).await;
    assert!(result.contains("transactions"));
}

#[tokio::test]
async fn complex_queries__41_full_block__query_too_complex() {
    let query = FULL_BLOCK_QUERY.to_string();
    let query = query.replace("$NUMBER_OF_BLOCKS", "41");

    let node = FuelService::new_node(Config::local_node()).await.unwrap();
    let url = format!("http://{}/v1/graphql", node.bound_address);

    let result = send_graph_ql_query(&url, query.as_str()).await;
    assert!(result.contains("Query is too complex."));
}

#[tokio::test]
async fn complex_queries__increased_block_cost__query_too_complex() {
    let query = FULL_BLOCK_QUERY.to_string();
    let query = query.replace("$NUMBER_OF_BLOCKS", "1");

    let mut config = Config::local_node();
    config.graphql_config.costs.block_header *= 41;
    config.graphql_config.costs.block_transactions *= 41;
    config.graphql_config.costs.block_header *= 41;

    let node = FuelService::new_node(config).await.unwrap();
    let url = format!("http://{}/v1/graphql", node.bound_address);

    let result = send_graph_ql_query(&url, query.as_str()).await;
    assert!(result.contains("Query is too complex."));
}

#[tokio::test]
async fn complex_queries__100_block_headers__works() {
    let query = r#"
        query {
          blocks(first: 100) {
            edges {
              cursor
              node {
                id
                header {
                  id
                  daHeight
                  consensusParametersVersion
                  stateTransitionBytecodeVersion
                  transactionsCount
                  messageReceiptCount
                  transactionsRoot
                  messageOutboxRoot
                  eventInboxRoot
                  height
                  prevRoot
                  time
                  applicationHash
                }
                consensus {
                  ... on Genesis {
                    chainConfigHash
                    coinsRoot
                    contractsRoot
                    messagesRoot
                    transactionsRoot
                  }
                  ... on PoAConsensus {
                    signature
                  }
                }
              }
            }
            pageInfo {
              endCursor
              hasNextPage
              hasPreviousPage
              startCursor
            }
            pageInfo {
              endCursor
              hasNextPage
              hasPreviousPage
              startCursor
            }
          }
        }
    "#;

    let node = FuelService::new_node(Config::local_node()).await.unwrap();
    let url = format!("http://{}/v1/graphql", node.bound_address);

    let result = send_graph_ql_query(&url, query).await;
    dbg!(&result);
    assert!(result.contains("transactions"));
}

#[tokio::test]
async fn body_limit_prevents_from_huge_queries() {
    // Given
    let node = FuelService::new_node(Config::local_node()).await.unwrap();
    let url = format!("http://{}/v1/graphql", node.bound_address);
    let client = reqwest::Client::new();

    // When
    let response = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("Content-Length", "33554432")
        .body(vec![123; 32 * 1024 * 1024])
        .send()
        .await;

    // Then
    let result = response.unwrap();
    assert_eq!(result.status(), 413);
}

#[tokio::test]
async fn complex_queries__1_state_transition_bytecode__works() {
    let version = LATEST_STATE_TRANSITION_VERSION;
    let query = r#"
    query {
      stateTransitionBytecodeByVersion(version: $VERSION) {
        bytecode {
          uploadedSubsectionsNumber
          bytecode
          completed
        }
      }
    }"#
    .to_string();
    let query = query.replace("$VERSION", version.to_string().as_str());

    let node = FuelService::new_node(Config::local_node()).await.unwrap();
    let url = format!("http://{}/v1/graphql", node.bound_address);

    let result = send_graph_ql_query(&url, query.as_str()).await;
    assert!(result.contains("bytecode"), "{}", result);
}

#[tokio::test]
async fn complex_queries__2_state_transition_bytecode__query_to_complex() {
    let version = LATEST_STATE_TRANSITION_VERSION;
    let query = r#"
    query {
      stateTransitionBytecodeByVersion(version: $VERSION) {
        bytecode {
          uploadedSubsectionsNumber
          bytecode
          completed
        }
      }
      stateTransitionBytecodeByVersion(version: $VERSION) {
        bytecode {
          uploadedSubsectionsNumber
          bytecode
          completed
        }
      }
    }"#
    .to_string();
    let query = query.replace("$VERSION", version.to_string().as_str());

    let node = FuelService::new_node(Config::local_node()).await.unwrap();
    let url = format!("http://{}/v1/graphql", node.bound_address);

    let result = send_graph_ql_query(&url, query.as_str()).await;
    assert!(result.contains("Query is too complex."));
}

#[tokio::test]
async fn concurrency_limit_0_prevents_any_throttled_queries() {
    // Given
    let mut config = Config::local_node();
    config.graphql_config.max_concurrent_queries = 0;
    config.graphql_config.api_request_timeout = std::time::Duration::from_millis(100);

    let node = FuelService::new_node(config).await.unwrap();
    let url = format!("http://{}/v1/graphql", node.bound_address);
    let client = reqwest::Client::new();

    // When
    let response = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("Content-Length", "10_000")
        .body(vec![42; 10_000])
        .send()
        .await;

    // Then
    let result = response.unwrap();
    assert_eq!(result.status(), 408);
}

#[tokio::test]
async fn concurrency_limit_0_allows_unthrottled_queries() {
    // Given
    let mut config = Config::local_node();
    // perhaps this shouldn't be called `graphql_config` but `api_config` or something
    config.graphql_config.max_concurrent_queries = 0;

    let node = FuelService::new_node(config).await.unwrap();
    let url = format!("http://{}/v1/health", node.bound_address);
    let client = reqwest::Client::new();

    // When
    let response = client.get(url).send().await;

    // Then
    let result = response.unwrap();
    assert_eq!(result.status(), 200);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn concurrency_limit_1_prevents_concurrent_queries() {
    // Given
    const NUM_OF_BLOCKS: u32 = 40;
    let num_samples = 100;

    let mut config = Config::local_node();
    config.graphql_config.max_concurrent_queries = 1;

    let query = FULL_BLOCK_QUERY.to_string();
    let query = query.replace("$NUMBER_OF_BLOCKS", NUM_OF_BLOCKS.to_string().as_str());

    let node = FuelService::new_node(config).await.unwrap();
    let url = format!("http://{}/v1/graphql", node.bound_address);
    let client = FuelClient::new(url.clone()).unwrap();
    client.produce_blocks(NUM_OF_BLOCKS, None).await.unwrap();

    let (tx, mut rx) = tokio::sync::mpsc::channel(100);

    // When

    // Measure the average request time for sequential queries
    let mut avg_request_time = 0;
    for _ in 0..num_samples {
        let now = Instant::now();
        let _ = send_graph_ql_query(&url, &query).await;
        avg_request_time += now.elapsed().as_nanos() / num_samples;
    }

    // Measure the average request time for concurrent queries
    for _ in 0..num_samples {
        let tx = tx.clone();
        let url = url.clone();
        let query = query.clone();

        tokio::spawn(async move {
            let now = Instant::now();
            send_graph_ql_query(&url, &query).await;
            let _ = tx.send(now.elapsed().as_nanos()).await;
        });
    }

    let mut avg_concurrent_request_time = 0;
    for _ in 0..num_samples {
        let request_time = rx.recv().await.unwrap();
        avg_concurrent_request_time += request_time / num_samples;
    }

    // Then

    // In an idealized model we should see c = s * n / 2
    // where
    //   c = average concurrent request time
    //   s = single request time
    //   n = number of request
    //   2 = the first even natural non-zero number ;)
    //
    // However, since this is inherently flaky we divide by 4 instead of 2 to have some margin,
    // while still maintaining our ability to assert a large deviation between the two measurements.
    assert!(avg_concurrent_request_time > avg_request_time * num_samples / 4);
}

#[tokio::test]
async fn recursion_in_queries_is_no_allowed__blocks() {
    let query = r#"
    query {
      blocks(last: 1) {
        edges {
          cursor
          node {
            id
            transactions {
              status {
                ... on SuccessStatus {
                  block {
                    height
                    transactions {
                      id
                    }
                  }
                }
                ... on FailureStatus {
                  block {
                    height
                    transactions {
                      id
                    }
                  }
                }
              }
            }
          }
        }
      }
    }"#;

    let node = FuelService::new_node(Config::local_node()).await.unwrap();
    let url = format!("http://{}/v1/graphql", node.bound_address);

    let result = send_graph_ql_query(&url, query).await;
    assert!(result.contains("Recursion detected"), "{:?}", result);
}

#[tokio::test]
async fn recursion_in_queries_is_no_allowed__transactions() {
    let query = r#"
    query {
      transactions {
        edges {
          cursor
          node {
            status {
              ... on SuccessStatus {
                block {
                  transactions {
                    id
                  }
                }
              }
            }
          }
        }
      }
    }"#;

    let node = FuelService::new_node(Config::local_node()).await.unwrap();
    let url = format!("http://{}/v1/graphql", node.bound_address);

    let result = send_graph_ql_query(&url, query).await;
    assert!(result.contains("Recursion detected"), "{:?}", result);
}

#[tokio::test]
async fn schema_is_retrievable() {
    let query = r#"
    query IntrospectionQuery {
      __schema {
        queryType {
          name
        }
        mutationType {
          name
        }
        subscriptionType {
          name
        }
        types {
          ...FullType
        }
        directives {
          name
          description
          locations
          args {
            ...InputValue
          }
        }
      }
    }

    fragment FullType on __Type {
      kind
      name
      description
      fields(includeDeprecated: true) {
        name
        description
        args {
          ...InputValue
        }
        type {
          ...TypeRef
        }
        isDeprecated
        deprecationReason
      }
      inputFields {
        ...InputValue
      }
      interfaces {
        ...TypeRef
      }
      enumValues(includeDeprecated: true) {
        name
        description
        isDeprecated
        deprecationReason
      }
      possibleTypes {
        ...TypeRef
      }
    }

    fragment InputValue on __InputValue {
      name
      description
      type {
        ...TypeRef
      }
      defaultValue
    }

    fragment TypeRef on __Type {
      kind
      name
      ofType {
        kind
        name
        ofType {
          kind
          name
          ofType {
            kind
            name
            ofType {
              kind
              name
              ofType {
                kind
                name
                ofType {
                  kind
                  name
                  ofType {
                    kind
                    name
                  }
                }
              }
            }
          }
        }
      }
    }"#;

    let node = FuelService::new_node(Config::local_node()).await.unwrap();
    let url = format!("http://{}/v1/graphql", node.bound_address);

    let result = send_graph_ql_query(&url, query).await;
    assert!(result.contains("__schema"), "{:?}", result);
}
