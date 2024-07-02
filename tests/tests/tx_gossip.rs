#![allow(unexpected_cfgs)] // for cfg(coverage)

use fuel_core::p2p_test_helpers::{
    make_nodes,
    BootstrapSetup,
    BootstrapType,
    Nodes,
    ProducerSetup,
    ValidatorSetup,
};
use fuel_core_client::client::FuelClient;
use fuel_core_types::{
    fuel_tx::{
        input::{
            coin::CoinSigned,
            Empty,
        },
        *,
    },
    fuel_vm::*,
    services::block_importer::SharedImportResult,
};
use futures::StreamExt;
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
        None,
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

const NUMBER_OF_INVALID_TXS: usize = 100;

async fn test_tx_gossiping_invalid_txs(
    bootstrap_type: BootstrapType,
) -> anyhow::Result<SharedImportResult> {
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
                    .with_name(format!("{}:producer", pub_keys[i]))
                    .bootstrap_type(bootstrap_type),
            )
        }),
        pub_keys.iter().flat_map(|pub_key| {
            (0..num_validators).map(move |i| {
                Some(
                    ValidatorSetup::new(*pub_key)
                        .with_name(format!("{pub_key}:{i}"))
                        .bootstrap_type(bootstrap_type)
                        // Validator wants to send invalid transactions.
                        .utxo_validation(false),
                )
            })
        }),
        None,
    )
    .await;

    let authority = &producers[0];
    let sentry = &validators[0];

    // Time for nodes to connect to each other.
    tokio::time::sleep(Duration::from_secs(2)).await;

    use rand::Rng;

    for _ in 0..NUMBER_OF_INVALID_TXS {
        let invalid_tx = TransactionBuilder::script(vec![], vec![])
            .add_input(Input::CoinSigned(CoinSigned {
                utxo_id: rng.gen(),
                owner: rng.gen(),
                amount: 0,
                asset_id: rng.gen(),
                tx_pointer: Default::default(),
                witness_index: 0,
                predicate_gas_used: Empty::new(),
                predicate: Empty::new(),
                predicate_data: Empty::new(),
            }))
            .add_witness(Witness::default())
            .finalize()
            .into();

        sentry.node.submit(invalid_tx).await.expect(
            "Should accept invalid transaction because `utxo_validation = false`.",
        );
    }

    // Give some time to receive all invalid transactions.
    tokio::time::sleep(Duration::from_secs(5)).await;

    let mut authority_blocks =
        authority.node.shared.block_importer.events_shared_result();

    // Submit a valid transaction from banned sentry to an authority node.
    let valid_transaction = authority.test_transactions()[0].clone();
    sentry
        .node
        .submit(valid_transaction.clone())
        .await
        .expect("Transaction is valid");
    let block = tokio::time::timeout(Duration::from_secs(5), authority_blocks.next())
        .await?
        .unwrap();
    Ok(block)
}

#[tokio::test(flavor = "multi_thread")]
#[cfg(not(coverage))]
async fn test_tx_gossiping_reserved_nodes_invalid_txs() {
    // Test verifies that gossiping of invalid transactions from reserved
    // nodes doesn't decrease its reputation.
    // After gossiping `NUMBER_OF_INVALID_TXS` transactions,
    // we will gossip one valid transaction, and it should be included.
    let result = test_tx_gossiping_invalid_txs(BootstrapType::ReservedNodes)
        .await
        .expect("Should be able to import block");

    assert_eq!(result.tx_status.len(), 2);
    assert!(matches!(
        result.tx_status[0].result,
        fuel_core_types::services::executor::TransactionExecutionResult::Success { .. }
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tx_gossiping_non_reserved_nodes_invalid_txs() {
    // This test is opposite to the `test_tx_gossiping_reserved_nodes_invalid_txs`.
    // It verifies that gossiping about invalid transactions from
    // non-reserved peers disconnects them.
    //
    // The test sends `NUMBER_OF_INVALID_TXS` invalid transactions,
    // and verifies that sending a valid one will be ignored.
    let result = test_tx_gossiping_invalid_txs(BootstrapType::BootstrapNodes).await;

    assert!(result.is_err());
}
