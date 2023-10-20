use fuel_core::p2p_test_helpers::{
    make_nodes,
    BootstrapSetup,
    Nodes,
    ProducerSetup,
    ValidatorSetup,
};
use fuel_core_client::client::FuelClient;
use fuel_core_types::{
    fuel_tx::*,
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

#[tokio::test(flavor = "multi_thread")]
async fn test_tx_gossiping() {
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

    // Create a producer for each key pair and a set of validators that share
    // the same key pair.
    let Nodes {
        producers,
        validators,
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
    )
    .await;

    let client_one = FuelClient::from(producers[0].node.bound_address);
    let client_two = FuelClient::from(validators[0].node.bound_address);

    let (tx_id, _) = producers[0]
        .insert_txs()
        .await
        .into_iter()
        .next()
        .expect("Producer is initialized with one transaction");

    let _ = client_one.await_transaction_commit(&tx_id).await.unwrap();

    let mut client_two_subscription = client_two
        .subscribe_transaction_status(&tx_id)
        .await
        .expect("Should be able to subscribe for events");
    tokio::time::timeout(Duration::from_secs(10), client_two_subscription.next())
        .await
        .expect("Should await transaction notification in time");

    let response = client_two.transaction(&tx_id).await.unwrap();
    assert!(response.is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tx_gossiping_reserved_nodes_invalid_txs() {
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

    // Create a producer for each key pair and a set of validators that share
    // the same key pair.
    let Nodes {
        producers,
        validators,
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
                Some(
                    ValidatorSetup::new(*pub_key)
                        .with_name(format!("{pub_key}:{i}"))
                        // Validator wants to send invalid transactions.
                        .disable_utxo_validation(),
                )
            })
        }),
    )
    .await;

    let authority = &producers[0];
    let sentry = &validators[0];

    // Time for nodes to connect to each other.
    tokio::time::sleep(Duration::from_secs(1)).await;

    use rand::Rng;

    for _ in 0..1000 {
        let invalid_tx = TransactionBuilder::script(vec![], vec![])
            .add_unsigned_coin_input(
                rng.gen(),
                rng.gen(),
                rng.gen(),
                rng.gen(),
                Default::default(),
                Default::default(),
            )
            .finalize()
            .into();

        sentry.node.submit(invalid_tx).await.expect(
            "Should accept invalid transaction because `utxo_validation = false`.",
        );
    }

    let valid_transaction = authority.test_transactions()[0].clone();
    let _ = tokio::time::timeout(
        Duration::from_secs(5),
        sentry.node.submit_and_await_commit(valid_transaction.clone()),
    )
    .await
    .expect_err("The authority node shouldn't accept a valid transaction after decreasing sentries reputation.");

    // While submitting valid transaction to authority node should include it into the block.
    let status = authority
        .node
        .submit_and_await_commit(valid_transaction)
        .await
        .expect("Should submit valid transaction without any errors.");

    assert!(matches!(
        status,
        fuel_core::schema::tx::types::TransactionStatus::Success(_)
    ));
}
