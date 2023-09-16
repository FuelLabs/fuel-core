use fuel_core::{
    chain_config::{
        CoinConfig,
        StateConfig,
    },
    service::{
        Config,
        FuelService,
    },
};
use fuel_core::{
    p2p_test_helpers::{
        make_nodes, BootstrapSetup, Nodes, ProducerSetup, ValidatorSetup,
    },
};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};
use fuel_core_client::client::FuelClient;
use fuel_core_poa::Trigger;
use fuel_core_types::{
    fuel_tx::{
        field::*,
        input::coin::{
            CoinPredicate,
            CoinSigned,
        },
        *,
    },
    fuel_vm::*,
};
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};
use std::time::Duration;


// This test is set up in such a way that the transaction is not committed
// as we've disabled the block production. This is to test that the peer
// will request this transaction from the other peer upon connection.
#[tokio::test(flavor = "multi_thread")]
async fn test_tx_request() {
    use futures::StreamExt;
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

    let _client_one = FuelClient::from(validators[0].node.bound_address);

    // Temporarily shut down the second node.
     producers[0].shutdown().await;



    // Insert transactions into the mempool of the first node.
    validators[0].test_txs = producers[0].test_txs.clone();
    let (tx_id, _) = validators[0]
        .insert_txs()
        .await
        .into_iter()
        .next()
        .expect("Validator is initialized with one transaction");

    println!("tx_id = {:?}", tx_id);

    // Sleep for 2 seconds
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Start second node
     producers[0].start().await;

    let client_two = FuelClient::from(producers[0].node.bound_address);

    let wait_time = Duration::from_secs(10);
    tokio::time::sleep(wait_time).await;
}
