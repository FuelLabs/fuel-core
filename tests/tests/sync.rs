#![allow(unexpected_cfgs)] // for cfg(coverage)

use fuel_core::p2p_test_helpers::*;
use fuel_core_types::{
    fuel_crypto::SecretKey,
    fuel_tx::Input,
};
use rand::{
    rngs::StdRng,
    SeedableRng,
};
use std::{
    collections::{
        hash_map::DefaultHasher,
        HashMap,
    },
    hash::{
        Hash,
        Hasher,
    },
};
use test_case::test_case;

#[tokio::test(flavor = "multi_thread")]
async fn test_producer_getting_own_blocks_back() {
    let mut rng = StdRng::seed_from_u64(line!() as u64);

    // Create a producer and a validator that share the same key pair.
    let secret = SecretKey::random(&mut rng);
    let pub_key = Input::owner(&secret.public_key());
    let Nodes {
        mut producers,
        mut validators,
        bootstrap_nodes: _dont_drop,
    } = make_nodes(
        [Some(BootstrapSetup::new(pub_key))],
        [Some(
            ProducerSetup::new(secret).with_txs(1).with_name("Alice"),
        )],
        [Some(ValidatorSetup::new(pub_key).with_name("Bob"))],
        None,
    )
    .await;

    let mut producer = producers.pop().unwrap();
    let mut validator = validators.pop().unwrap();

    // Shut down the validator.
    validator.shutdown().await;

    // Insert the transactions into the tx pool.
    let expected = producer.insert_txs().await;

    // Wait up to 10 seconds for the producer to commit their own blocks.
    producer.consistency_10s(&expected).await;

    // Start the validator.
    validator.start().await;

    // Wait up to 10 seconds for the validator to sync with the producer.
    validator.consistency_10s(&expected).await;
}

#[test_case(1; "partition with 1 tx")]
#[test_case(10; "partition with 10 txs")]
#[test_case(100; "partition with 100 txs")]
#[tokio::test(flavor = "multi_thread")]
async fn test_partition_single(num_txs: usize) {
    // Create a random seed based on the test parameters.
    let mut hasher = DefaultHasher::new();
    (num_txs, line!()).hash(&mut hasher);
    let mut rng = StdRng::seed_from_u64(hasher.finish());

    // Create a producer and two validators that share the same key pair.
    let secret = SecretKey::random(&mut rng);
    let pub_key = Input::owner(&secret.public_key());
    let Nodes {
        mut producers,
        validators,
        bootstrap_nodes: _dont_drop,
    } = make_nodes(
        [Some(BootstrapSetup::new(pub_key))],
        [Some(
            ProducerSetup::new(secret)
                .with_txs(num_txs)
                .with_name("Alice"),
        )],
        [
            Some(ValidatorSetup::new(pub_key).with_name("Bob")),
            Some(ValidatorSetup::new(pub_key).with_name("Carol")),
        ],
        None,
    )
    .await;

    // Convert to named nodes.
    let mut validators: NamedNodes = validators.into();

    let mut producer = producers.pop().unwrap();

    // Shutdown Carol.
    validators["Carol"].shutdown().await;

    // Insert the transactions into the tx pool.
    let expected = producer.insert_txs().await;

    // Wait up to 10 seconds for the producer to commit their own blocks.
    producer.consistency_10s(&expected).await;

    // Wait up to 20 seconds for Bob to sync with the producer.
    validators["Bob"].consistency_20s(&expected).await;

    // Shutdown the producer.
    producer.shutdown().await;

    // Start Carol.
    validators["Carol"].start().await;

    // Wait up to 20 seconds for Carol to sync with Bob.
    validators["Carol"].consistency_20s(&expected).await;
}

#[test_case(1, 3, 3; "partition with 1 tx 3 validators 3 partitions")]
#[test_case(10, 3, 3; "partition with 10 txs 3 validators 3 partitions")]
#[test_case(100, 3, 3; "partition with 100 txs 3 validators 3 partitions")]
#[test_case(1, 8, 4; "partition with 1 tx 8 validators 4 partitions")]
#[test_case(10, 8, 4; "partition with 10 txs 8 validators 4 partitions")]
#[test_case(100, 8, 4; "partition with 100 txs 8 validators 4 partitions")]
#[tokio::test(flavor = "multi_thread")]
#[cfg(not(coverage))] // This test is too slow for coverage
async fn test_partitions_larger_groups(
    num_txs: usize,
    num_validators: usize,
    num_partitions: usize,
) {
    use itertools::Itertools;
    use std::collections::VecDeque;

    // Create a random seed based on the test parameters.
    let mut hasher = DefaultHasher::new();
    (num_txs, num_validators, num_partitions, line!()).hash(&mut hasher);
    let mut rng = StdRng::seed_from_u64(hasher.finish());

    // Create a producer and a set of validators that share the same key pair.
    let secret = SecretKey::random(&mut rng);
    let pub_key = Input::owner(&secret.public_key());
    let Nodes {
        mut producers,
        mut validators,
        bootstrap_nodes: _dont_drop,
    } = make_nodes(
        [Some(BootstrapSetup::new(pub_key))],
        [Some(
            ProducerSetup::new(secret)
                .with_txs(num_txs)
                .with_name(format!("{pub_key}:producer")),
        )],
        (0..num_validators).map(|i| {
            Some(ValidatorSetup::new(pub_key).with_name(format!("{pub_key}:{i}")))
        }),
        None,
    )
    .await;

    let mut producer = producers.pop().unwrap();

    // Get the number of validators per partition.
    let group_size = num_validators / num_partitions;
    assert_eq!(num_validators % num_partitions, 0);

    // Shutdown the validators.
    for v in &mut validators {
        v.shutdown().await;
    }

    // Insert the transactions into the tx pool.
    let expected = producer.insert_txs().await;
    producer.consistency_20s(&expected).await;

    // The overlap between two groups.
    let mut overlap: VecDeque<Vec<Node>> = VecDeque::with_capacity(2);

    // Partition the validators into groups.
    let groups = validators
        .into_iter()
        .chunks(group_size)
        .into_iter()
        .map(|chunk| chunk.collect::<Vec<_>>())
        .collect::<Vec<_>>();

    // The producer is the first overlap.
    overlap.push_back(vec![producer]);

    // For each group, start the group, wait for it to sync with the overlapping
    // group, and shutdown the overlapping group.
    for mut validators in groups {
        assert_eq!(overlap.len(), 1);

        // Start this group.
        for v in &mut validators {
            v.start().await;
        }

        // Wait up to 10 seconds validators to sync with the overlapping group.
        for v in &mut validators {
            v.consistency_20s(&expected).await;
        }

        // Shutdown the overlapping group.
        let last_group = overlap.pop_front().unwrap();
        for mut v in last_group {
            v.shutdown().await;
        }

        // The current group is the next overlap.
        overlap.push_back(validators);
    }
}

#[tokio::test(flavor = "multi_thread")]
#[cfg(not(coverage))] // This test is too slow for coverage
async fn test_multiple_producers_different_keys() {
    use itertools::Itertools;

    // Create a random seed based on the test parameters.
    let mut hasher = DefaultHasher::new();
    let num_txs = 10;
    let num_validators = 6;
    let num_partitions = 3;
    (num_txs, num_validators, num_partitions, line!()).hash(&mut hasher);
    let mut rng = StdRng::seed_from_u64(hasher.finish());

    // Create a set of key pairs.
    let secrets: Vec<_> = (0..num_partitions)
        .map(|_| SecretKey::random(&mut rng))
        .collect();
    let pub_keys: Vec<_> = secrets
        .clone()
        .into_iter()
        .map(|secret| Input::owner(&secret.public_key()))
        .collect();

    // Create a producer for each key pair and a set of validators that share
    // the same key pair.
    let Nodes {
        mut producers,
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

    // Get the number of validators per key pair.
    let group_size = num_validators;

    // Insert the transactions into the tx pool
    // and gather the expect transactions for each group.
    let mut expected = Vec::with_capacity(num_partitions);
    for p in &mut producers {
        expected.push(p.insert_txs().await);
    }

    // Wait producers to produce all blocks.
    for (expected, producer) in expected.iter().zip(producers.iter_mut()) {
        producer.consistency_10s(expected).await;
    }

    // Partition the validators into groups.
    let groups = validators
        .into_iter()
        .chunks(group_size)
        .into_iter()
        .map(|chunk| chunk.collect::<Vec<_>>())
        .collect::<Vec<_>>();

    // For each group, start the group and wait for it to sync with the
    // producer with the same key pair.
    for (expected, mut validators) in expected.iter().zip(groups) {
        assert_eq!(group_size, validators.len());
        for v in &mut validators {
            v.consistency_20s(expected).await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "This test might not make any sense, since we probably don't want to support PoA producers sharing their private key"]
async fn test_multiple_producers_same_key() {
    let mut hasher = DefaultHasher::new();
    let num_txs = 10;
    let num_validators = 6;
    let num_producers = 3;
    (num_txs, num_validators, num_producers, line!()).hash(&mut hasher);
    let mut rng = StdRng::seed_from_u64(hasher.finish());

    let secret = SecretKey::random(&mut rng);
    let pub_key = Input::owner(&secret.public_key());
    let Nodes {
        mut producers,
        mut validators,
        bootstrap_nodes: _dont_drop,
    } = make_nodes(
        std::iter::repeat(Some(BootstrapSetup::new(pub_key))).take(num_producers),
        std::iter::repeat(Some(ProducerSetup::new(secret))).take(num_producers),
        std::iter::repeat(Some(ValidatorSetup::new(pub_key))).take(num_validators),
        None,
    )
    .await;

    let mut expected = HashMap::new();
    for p in &mut producers {
        expected.extend(p.insert_txs().await);
    }

    for v in &mut validators {
        v.consistency_10s(&expected).await;
    }
}
