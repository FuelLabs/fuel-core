#![allow(warnings)]

use fuel_core::service::{
    Config,
    FuelService,
    ServiceTrait,
};
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
                  block {
                    height
                  }
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
async fn complex_queries__10_full_blocks__works() {
    let query = FULL_BLOCK_QUERY.to_string();
    let query = query.replace("$NUMBER_OF_BLOCKS", "10");

    let node = FuelService::new_node(Config::local_node()).await.unwrap();
    let url = format!("http://{}/v1/graphql", node.bound_address);

    let result = send_graph_ql_query(&url, query.as_str()).await;
    assert!(result.contains("transactions"));
}

#[tokio::test]
async fn complex_queries__11_full_block__query_to_complex() {
    let query = FULL_BLOCK_QUERY.to_string();
    let query = query.replace("$NUMBER_OF_BLOCKS", "11");

    let node = FuelService::new_node(Config::local_node()).await.unwrap();
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
        .header("Content-Length", "18446744073709551613")
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
