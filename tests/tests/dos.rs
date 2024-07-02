#![allow(warnings)]

use fuel_core::service::{
    Config,
    FuelService,
    ServiceTrait,
};
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

#[tokio::test]
async fn complex_queries__10_blocks__works() {
    let query = r#"
        query {
          blocks(first: 10) {
            nodes {
              transactions {
                id
              }
            }
          }
        }
    "#;

    let node = FuelService::new_node(Config::local_node()).await.unwrap();
    let url = format!("http://{}/v1/graphql", node.bound_address);

    let result = send_graph_ql_query(&url, query).await;
    assert!(result.contains("transactions"));
}

#[tokio::test]
async fn complex_queries__50_block__query_to_complex() {
    let query = r#"
        query {
          blocks(first: 50) {
            nodes {
              transactions {
                id
              }
            }
          }
        }
    "#;

    let node = FuelService::new_node(Config::local_node()).await.unwrap();
    let url = format!("http://{}/v1/graphql", node.bound_address);

    let result = send_graph_ql_query(&url, query).await;
    assert!(result.contains("Query is too complex."));
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
