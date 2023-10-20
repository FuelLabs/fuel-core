use fuel_core::p2p_test_helpers::{
    make_nodes,
    BootstrapSetup,
    Nodes,
    ProducerSetup,
    ValidatorSetup,
};
use fuel_core_client::client::FuelClient;
use fuel_core_types::{
    fuel_tx::{
        input::Input,
        UniqueIdentifier,
    },
    fuel_vm::*,
};
use rand::{
    rngs::StdRng,
    SeedableRng,
};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{
        Hash,
        Hasher,
    },
    time::Duration,
};

// This test is set up in such a way that the transaction is not committed
// as we've disabled the block production. This is to test that the peer
// will request this transaction from the other peer upon connection.
#[tokio::test(flavor = "multi_thread")]
async fn test_tx_request() {
    // Create a random seed based on the test parameters.
    let mut hasher = DefaultHasher::new();
    let num_txs = 1;
    let num_validators = 1;
    let num_partitions = 1;
    (num_txs, num_validators, num_partitions, line!()).hash(&mut hasher);
    let mut rng = StdRng::seed_from_u64(hasher.finish());

    // Create a set of key pairs.
    let secrets: Vec<_> = (0..1).map(|_| SecretKey::random(&mut rng)).collect();
    let pub_keys: Vec<_> = secrets
        .clone()
        .into_iter()
        .map(|secret| Input::owner(&secret.public_key()))
        .collect();

    // Creates a simple node config with no block production.
    // This is used for these tests because we don't want the transactions to be
    // included in a block. We want them to be included in the mempool and check
    // that new connected nodes sync the mempool.
    let disable_block_production = true;
    // Create a producer for each key pair and a set of validators that share
    // the same key pair.
    let Nodes {
        mut producers,
        mut validators,
        bootstrap_nodes: _dont_drop,
    } = make_nodes(
        pub_keys
            .iter()
            .map(|pub_key| Some(BootstrapSetup::new(*pub_key))),
        secrets.clone().into_iter().enumerate().map(|(i, secret)| {
            Some(
                ProducerSetup::new(secret)
                    .with_txs(num_txs)
                    .with_name(format!("{}:producer", pub_keys[i])),
            )
        }),
        pub_keys.iter().flat_map(|pub_key| {
            (0..num_validators).map(move |i| {
                Some(ValidatorSetup::new(*pub_key).with_name(format!("{pub_key}:{i}")))
            })
        }),
        disable_block_production,
    )
    .await;

    let first_node = &mut validators[0];
    let second_node = &mut producers[0];

    let client_one = FuelClient::from(first_node.node.bound_address);

    // Temporarily shut down the second node.
    second_node.shutdown().await;

    // Insert transactions into the mempool of the first node.
    first_node.test_txs = second_node.test_txs.clone();
    let _tx = first_node.insert_txs().await;

    // Sleep for 2 seconds
    tokio::time::sleep(Duration::from_secs(2)).await;

    second_node.start().await;

    // Wait for nodes to share their transaction pool.
    tokio::time::sleep(Duration::from_secs(2)).await;

    let client_two = FuelClient::from(second_node.node.bound_address);

    // Produce a new block with the participating nodes.
    // It's expected that they will:
    // 1. Connect to each other;
    // 2. Share the transaction;
    // 3. Produce a block with the transaction;
    let _ = client_one.produce_blocks(1, None).await.unwrap();
    let _ = client_two.produce_blocks(1, None).await.unwrap();

    let chain_id = first_node.config.chain_conf.consensus_parameters.chain_id();

    let request = fuel_core_client::client::pagination::PaginationRequest {
        cursor: None,
        results: 10,
        direction: fuel_core_client::client::pagination::PageDirection::Forward,
    };

    let first_node_txs = client_two.transactions(request.clone()).await.unwrap();
    let second_node_txs = client_one.transactions(request).await.unwrap();

    let first_node_tx = first_node_txs
        .results
        .first()
        .expect("txs should have at least one result")
        .transaction
        .id(&chain_id);
    let second_node_tx = second_node_txs
        .results
        .first()
        .expect("txs should have at least one result")
        .transaction
        .id(&chain_id);

    assert_eq!(first_node_tx, second_node_tx);
}

// Similar to the previous test, but this time we're testing
// sending enough requests that it won't fit into a single batch.
#[tokio::test(flavor = "multi_thread")]
async fn test_tx_request_multiple_batches() {
    // Create a random seed based on the test parameters.
    let mut hasher = DefaultHasher::new();
    let num_txs = 200;
    let num_validators = 1;
    let num_partitions = 1;
    (num_txs, num_validators, num_partitions, line!()).hash(&mut hasher);
    let mut rng = StdRng::seed_from_u64(hasher.finish());

    // Create a set of key pairs.
    let secrets: Vec<_> = (0..1).map(|_| SecretKey::random(&mut rng)).collect();
    let pub_keys: Vec<_> = secrets
        .clone()
        .into_iter()
        .map(|secret| Input::owner(&secret.public_key()))
        .collect();

    // Creates a simple node config with no block production.
    // This is used for these tests because we don't want the transactions to be
    // included in a block. We want them to be included in the mempool and check
    // that new connected nodes sync the mempool.
    let disable_block_production = true;
    // Create a producer for each key pair and a set of validators that share
    // the same key pair.
    let Nodes {
        mut producers,
        mut validators,
        bootstrap_nodes: _dont_drop,
    } = make_nodes(
        pub_keys
            .iter()
            .map(|pub_key| Some(BootstrapSetup::new(*pub_key))),
        secrets.clone().into_iter().enumerate().map(|(i, secret)| {
            Some(
                ProducerSetup::new(secret)
                    .with_txs(num_txs)
                    .with_name(format!("{}:producer", pub_keys[i])),
            )
        }),
        pub_keys.iter().flat_map(|pub_key| {
            (0..num_validators).map(move |i| {
                Some(ValidatorSetup::new(*pub_key).with_name(format!("{pub_key}:{i}")))
            })
        }),
        disable_block_production,
    )
    .await;

    let first_node = &mut validators[0];
    let second_node = &mut producers[0];

    let client_one = FuelClient::from(first_node.node.bound_address);

    // Temporarily shut down the second node.
    second_node.shutdown().await;

    // Insert transactions into the mempool of the first node.
    first_node.test_txs = second_node.test_txs.clone();
    let _tx = first_node.insert_txs().await;

    // Sleep for 2 seconds
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Start second node
    second_node.start().await;

    // Wait for nodes to share their transaction pool.
    tokio::time::sleep(Duration::from_secs(5)).await;

    let client_two = FuelClient::from(second_node.node.bound_address);

    // Produce a new block with the participating nodes.
    // It's expected that they will:
    // 1. Connect to each other;
    // 2. Share the transaction;
    // 3. Produce a block with the transaction;
    let _ = client_one.produce_blocks(1, None).await.unwrap();
    let _ = client_two.produce_blocks(1, None).await.unwrap();

    let request = fuel_core_client::client::pagination::PaginationRequest {
        cursor: None,
        results: 1000,
        direction: fuel_core_client::client::pagination::PageDirection::Forward,
    };

    let first_node_txs = client_one.transactions(request.clone()).await.unwrap();
    let second_node_txs = client_two.transactions(request).await.unwrap();

    assert_eq!(first_node_txs.results.len(), second_node_txs.results.len());
}
