#![allow(non_snake_case)]

use fuel_core::{
    database::Database,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_client::client::FuelClient;
use reqwest::Url;
use std::{
    net::SocketAddr,
    time::Duration,
};
use tokio::time::timeout;

fn graphql_url(addr: SocketAddr) -> Url {
    let mut url = Url::parse(&format!("http://{}", addr)).unwrap();
    url.set_path("/v1/graphql");
    url
}

fn invalid_graphql_url(port: u16) -> Url {
    let mut url = Url::parse(&format!("http://localhost:{}", port)).unwrap();
    url.set_path("/v1/graphql");
    url
}

#[tokio::test]
async fn client_can_be_created_with_multiple_urls() {
    // Given
    let srv1 = FuelService::from_database(Database::default(), Config::local_node())
        .await
        .unwrap();
    let srv2 = FuelService::from_database(Database::default(), Config::local_node())
        .await
        .unwrap();

    let url1 = graphql_url(srv1.bound_address);
    let url2 = graphql_url(srv2.bound_address);

    // When
    let client = FuelClient::with_urls(vec![url1, url2])
        .expect("Failed to create client with multiple URLs");

    // Then
    let health = client.health().await.unwrap();
    assert!(
        health,
        "Client should be healthy when connected to first URL"
    );
}

#[tokio::test]
async fn client_with_empty_url_list_fails() {
    // Given
    let result = FuelClient::with_urls(vec![]);

    // Then
    assert!(
        result.is_err(),
        "FuelClient creation with an empty list of URL should fail"
    );
}

#[tokio::test]
async fn client_fails_over_to_second_url_when_first_unavailable() {
    // Given
    let srv = FuelService::from_database(Database::default(), Config::local_node())
        .await
        .unwrap();

    // Use an invalid URL as first and valid as second
    let invalid_url = invalid_graphql_url(1);
    let valid_url = graphql_url(srv.bound_address);

    // When
    let client = FuelClient::with_urls(vec![invalid_url, valid_url])
        .expect("Failed to create client");

    // Then
    // This should succeed by failing over to the second URL
    let health = timeout(Duration::from_secs(5), client.health())
        .await
        .expect("Timeout waiting for health check")
        .expect("Health check should succeed after failover");

    assert!(
        health,
        "Client should be healthy after failing over to second URL"
    );
}

#[tokio::test]
async fn client_fails_over_when_first_server_stops() {
    // Given
    let srv1 = FuelService::from_database(Database::default(), Config::local_node())
        .await
        .unwrap();
    let srv2 = FuelService::from_database(Database::default(), Config::local_node())
        .await
        .unwrap();

    let url1 = graphql_url(srv1.bound_address);
    let url2 = graphql_url(srv2.bound_address);

    let client =
        FuelClient::with_urls(vec![url1, url2]).expect("Failed to create client");

    // Verify initial connection works
    assert!(client.health().await.unwrap());

    // When - stop the first server
    srv1.send_stop_signal_and_await_shutdown().await.unwrap();

    // Then - client should still work by failing over to second server
    let health = timeout(Duration::from_secs(5), client.health())
        .await
        .expect("Timeout waiting for health check after failover")
        .expect("Health check should succeed after server shutdown");

    assert!(
        health,
        "Client should fail over to second server when first stops"
    );
}

#[tokio::test]
async fn client_returns_error_when_all_servers_unavailable() {
    // Given - use only invalid URLs
    let invalid_url1 = invalid_graphql_url(1);
    let invalid_url2 = invalid_graphql_url(2);

    let client = FuelClient::with_urls(vec![invalid_url1, invalid_url2])
        .expect("Failed to create client");

    // When/Then - should fail since all servers are unavailable
    let result = timeout(Duration::from_secs(3), client.health()).await;

    assert!(
        result.is_err() || result.unwrap().is_err(),
        "Client should fail when all servers are unavailable"
    );
}

#[tokio::test]
async fn failover_works_with_chain_info_query() {
    // Given
    let srv = FuelService::from_database(Database::default(), Config::local_node())
        .await
        .unwrap();

    let invalid_url = invalid_graphql_url(1);
    let valid_url = graphql_url(srv.bound_address);

    let client = FuelClient::with_urls(vec![invalid_url, valid_url])
        .expect("Failed to create client");

    // When
    let chain_info = timeout(Duration::from_secs(5), client.chain_info())
        .await
        .expect("Timeout")
        .expect("Should get chain info after failover");

    println!("{chain_info:?}");
    // Then
    assert!(
        chain_info.name.len() > 0,
        "Should receive valid chain info after failover"
    );
}

#[tokio::test]
async fn failover_works_with_node_info_query() {
    // Given
    let srv = FuelService::from_database(Database::default(), Config::local_node())
        .await
        .unwrap();

    let invalid_url = invalid_graphql_url(1);
    let valid_url = graphql_url(srv.bound_address);

    let client = FuelClient::with_urls(vec![invalid_url, valid_url])
        .expect("Failed to create client");

    // When
    let node_info = timeout(Duration::from_secs(5), client.node_info())
        .await
        .expect("Timeout")
        .expect("Should get node info after failover");

    // Then
    assert!(
        node_info.node_version.len() > 0,
        "Should receive valid node info after failover"
    );
}

#[tokio::test]
async fn failover_works_with_transaction_queries() {
    // Given
    let srv = FuelService::from_database(Database::default(), Config::local_node())
        .await
        .unwrap();

    let invalid_url = invalid_graphql_url(1);
    let valid_url = graphql_url(srv.bound_address);

    let client = FuelClient::with_urls(vec![invalid_url, valid_url])
        .expect("Failed to create client");

    // When - query transactions (should work even if empty)
    let result = timeout(
        Duration::from_secs(5),
        client.transactions(fuel_core_client::client::pagination::PaginationRequest {
            cursor: None,
            results: 10,
            direction: fuel_core_client::client::pagination::PageDirection::Forward,
        }),
    )
    .await
    .expect("Timeout waiting for transactions query")
    .expect("Transactions query should succeed after failover");

    // Then - should get a valid response (even if empty)
    assert!(
        result.results.len() == 0,
        "Should receive valid (empty) transaction list after failover"
    );
}

#[tokio::test]
async fn failover_persists_across_multiple_requests() {
    // Given
    let srv = FuelService::from_database(Database::default(), Config::local_node())
        .await
        .unwrap();

    let invalid_url = invalid_graphql_url(1);
    let valid_url = graphql_url(srv.bound_address);

    let client = FuelClient::with_urls(vec![invalid_url, valid_url])
        .expect("Failed to create client");

    // When - make multiple requests
    for _ in 0..3 {
        let health = timeout(Duration::from_secs(5), client.health())
            .await
            .expect("Timeout")
            .expect("Health check should succeed");

        assert!(health, "Each request should succeed via failover");
    }

    // Then - verify we can still query other endpoints
    let chain_info = timeout(Duration::from_secs(5), client.chain_info())
        .await
        .expect("Timeout")
        .expect("Chain info should work");

    assert!(chain_info.name.len() > 0);
}

#[tokio::test]
async fn client_tries_all_urls_in_sequence() {
    // Given
    let srv1 = FuelService::from_database(Database::default(), Config::local_node())
        .await
        .unwrap();
    let srv3 = FuelService::from_database(Database::default(), Config::local_node())
        .await
        .unwrap();

    let url1 = graphql_url(srv1.bound_address);
    let invalid_url = invalid_graphql_url(1);
    let url3 = graphql_url(srv3.bound_address);

    // When
    let client = FuelClient::with_urls(vec![url1, invalid_url, url3])
        .expect("Failed to create client");

    // Then - should connect to first server
    assert!(client.health().await.unwrap());

    // When - stop first server
    srv1.send_stop_signal_and_await_shutdown().await.unwrap();

    // Then - should skip invalid middle URL and connect to third
    let health = timeout(Duration::from_secs(5), client.health())
        .await
        .expect("Timeout")
        .expect("Should failover to third server");

    assert!(health);
}

#[tokio::test]
async fn client_sticks_with_working_server_after_failover() {
    // Given
    let srv1 = FuelService::from_database(Database::default(), Config::local_node())
        .await
        .unwrap();
    let srv2 = FuelService::from_database(Database::default(), Config::local_node())
        .await
        .unwrap();

    let url1 = graphql_url(srv1.bound_address);
    let url2 = graphql_url(srv2.bound_address);

    let client =
        FuelClient::with_urls(vec![url1, url2]).expect("Failed to create client");

    // Initial connection to srv1
    assert!(client.health().await.unwrap());

    // When - stop first server to trigger failover
    srv1.send_stop_signal_and_await_shutdown().await.unwrap();

    // Trigger failover
    let _ = timeout(Duration::from_secs(5), client.health())
        .await
        .expect("Timeout")
        .expect("Should failover");

    // Then - multiple subsequent requests should succeed quickly
    // (indicating it's sticking with the working server)
    for _ in 0..5 {
        let start = std::time::Instant::now();
        let health = client.health().await.expect("Health check should work");
        let duration = start.elapsed();

        assert!(health);
        // Subsequent requests should be fast (< 1s) since we're not trying the dead server
        assert!(
            duration < Duration::from_secs(1),
            "Request should be fast when using cached working server, took {:?}",
            duration
        );
    }
}

#[tokio::test]
async fn failover_works_with_block_queries() {
    // Given
    let mut config = Config::local_node();
    config.block_production = fuel_core_poa::Trigger::Instant;

    let srv = FuelService::from_database(Database::default(), config)
        .await
        .unwrap();

    let invalid_url = invalid_graphql_url(1);
    let valid_url = graphql_url(srv.bound_address);

    let client = FuelClient::with_urls(vec![invalid_url, valid_url])
        .expect("Failed to create client");

    // When
    client
        .produce_blocks(1, None)
        .await
        .expect("Should produce block after failover");

    let blocks = timeout(
        Duration::from_secs(5),
        client.blocks(fuel_core_client::client::pagination::PaginationRequest {
            cursor: None,
            results: 10,
            direction: fuel_core_client::client::pagination::PageDirection::Forward,
        }),
    )
    .await
    .expect("Timeout")
    .expect("Should query blocks after failover");

    // Then
    assert!(
        blocks.results.len() > 0,
        "Should receive blocks after failover"
    );
}

#[tokio::test]
async fn failover_works_with_consensus_parameters() {
    // Given
    let srv = FuelService::from_database(Database::default(), Config::local_node())
        .await
        .unwrap();

    let invalid_url = invalid_graphql_url(1);
    let valid_url = graphql_url(srv.bound_address);

    let client = FuelClient::with_urls(vec![invalid_url, valid_url])
        .expect("Failed to create client");

    // When
    let params = timeout(
        Duration::from_secs(5),
        client.consensus_parameters(0.into()),
    )
    .await
    .expect("Timeout")
    .expect("Should get consensus parameters after failover");

    // Then
    assert!(
        params.is_some(),
        "Should receive consensus parameters after failover"
    );
}

#[cfg(not(feature = "only-p2p"))]
mod subscription_tests {
    use super::*;
    use fuel_core_poa::Trigger;
    use futures::StreamExt;

    /// Test failover with block subscription
    #[tokio::test]
    async fn failover_works_with_block_subscription() {
        // Given
        let mut config = Config::local_node();
        config.block_production = Trigger::Instant;

        let srv = FuelService::from_database(Database::default(), config)
            .await
            .unwrap();

        let invalid_url = invalid_graphql_url(1);
        let valid_url = graphql_url(srv.bound_address);

        let client = FuelClient::with_urls(vec![invalid_url, valid_url])
            .expect("Failed to create client");

        // When - subscribe to new blocks
        let mut subscription =
            timeout(Duration::from_secs(5), client.new_blocks_subscription())
                .await
                .expect("Timeout waiting for subscription")
                .expect("Subscription should succeed after failover");

        // Given - Produce a block
        client.produce_blocks(1, None).await.unwrap();

        // Then - should receive the block via subscription
        let block = timeout(Duration::from_secs(5), subscription.next())
            .await
            .expect("Timeout waiting for block")
            .expect("Should receive block")
            .expect("Block should be valid");

        assert_eq!(*block.sealed_block.entity.header().height(), 1u32.into());
    }

    #[tokio::test]
    async fn subscription_fails_over_when_server_stops() {
        // Given
        let srv1 = FuelService::from_database(
            Database::default(),
            Config {
                block_production: Trigger::Instant,
                ..Config::local_node()
            },
        )
        .await
        .unwrap();

        let srv2 = FuelService::from_database(
            Database::default(),
            Config {
                block_production: Trigger::Instant,
                ..Config::local_node()
            },
        )
        .await
        .unwrap();

        let url1 = graphql_url(srv1.bound_address);
        let url2 = graphql_url(srv2.bound_address);

        let client =
            FuelClient::with_urls(vec![url1, url2]).expect("Failed to create client");

        // Verify initial connection works
        assert!(client.health().await.unwrap());

        // When - stop the first server
        srv1.send_stop_signal_and_await_shutdown().await.unwrap();

        // Then - subscription should still work by failing over to second server
        let mut subscription =
            timeout(Duration::from_secs(5), client.new_blocks_subscription())
                .await
                .expect("Timeout")
                .expect("Subscription should succeed after failover");

        // Produce a block on second server
        client.produce_blocks(1, None).await.unwrap();

        // Should receive the block
        let block = timeout(Duration::from_secs(5), subscription.next())
            .await
            .expect("Timeout")
            .expect("Should receive block")
            .expect("Block should be valid");

        assert_eq!(*block.sealed_block.entity.header().height(), 1u32.into());
    }

    #[tokio::test]
    async fn subscription_returns_error_when_all_servers_unavailable() {
        // Given - use only invalid URLs
        let invalid_url1 = invalid_graphql_url(1);
        let invalid_url2 = invalid_graphql_url(2);

        let client = FuelClient::with_urls(vec![invalid_url1, invalid_url2])
            .expect("Failed to create client");

        // When/Then - subscription should fail since all servers are unavailable
        let result =
            timeout(Duration::from_secs(3), client.new_blocks_subscription()).await;

        assert!(
            result.is_err() || result.unwrap().is_err(),
            "Subscription should fail when all servers are unavailable"
        );
    }

    #[tokio::test]
    async fn subscription_failover_persists_across_multiple_subscriptions() {
        // Given
        let mut config = Config::local_node();
        config.block_production = Trigger::Instant;

        let srv = FuelService::from_database(Database::default(), config)
            .await
            .unwrap();

        let invalid_url = invalid_graphql_url(1);
        let valid_url = graphql_url(srv.bound_address);

        let client = FuelClient::with_urls(vec![invalid_url, valid_url])
            .expect("Failed to create client");

        // When - create multiple subscriptions
        for _ in 0..3 {
            let mut subscription =
                timeout(Duration::from_secs(5), client.new_blocks_subscription())
                    .await
                    .expect("Timeout")
                    .expect("Subscription should succeed");

            // Produce a block
            client.produce_blocks(1, None).await.unwrap();

            // Verify we receive it
            let block = timeout(Duration::from_secs(5), subscription.next())
                .await
                .expect("Timeout")
                .expect("Should receive block")
                .expect("Block should be valid");

            assert!(
                *block.sealed_block.entity.header().height() >= 1u32.into(),
                "Should receive blocks via subscription after failover"
            );
        }
    }
}
