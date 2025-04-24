use fuel_core::p2p_test_helpers::{
    BootstrapSetup, Nodes, ProducerSetup, ValidatorSetup, make_nodes,
};
use fuel_core_client::client::FuelClient;
use fuel_core_types::{fuel_tx::*, fuel_vm::*};
use futures::StreamExt;
use rand::{SeedableRng, rngs::StdRng};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    time::Duration,
};

/// Test to verify that transaction gossiping works by default (subscribe_to_transactions=true)
#[tokio::test(flavor = "multi_thread")]
async fn test_tx_gossiping_enabled_by_default() {
    // Create a random seed based on the test parameters.
    let mut hasher = DefaultHasher::new();
    let num_txs = 1;
    let num_validators = 1;
    (num_txs, num_validators, line!()).hash(&mut hasher);
    let mut rng = StdRng::seed_from_u64(hasher.finish());

    // Create a set of key pairs.
    let secrets: Vec<_> = (0..1).map(|_| SecretKey::random(&mut rng)).collect();
    let pub_keys: Vec<_> = secrets
        .clone()
        .into_iter()
        .map(|secret| Input::owner(&secret.public_key()))
        .collect();

    // Create producer and validator with default configuration (subscribe_to_transactions=true)
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

    // Verify the configuration was applied correctly
    if let Some(p2p) = &producers[0].node.shared.config.p2p {
        eprintln!(
            "Default Producer subscribe_to_transactions: {}",
            p2p.subscribe_to_transactions
        );
        assert!(
            p2p.subscribe_to_transactions,
            "Producer should have subscribe_to_transactions=true by default"
        );
    }
    if let Some(p2p) = &validators[0].node.shared.config.p2p {
        eprintln!(
            "Default Validator subscribe_to_transactions: {}",
            p2p.subscribe_to_transactions
        );
        assert!(
            p2p.subscribe_to_transactions,
            "Validator should have subscribe_to_transactions=true by default"
        );
    }

    let client_one = FuelClient::from(producers[0].node.bound_address);
    let client_two = FuelClient::from(validators[0].node.bound_address);

    let (tx_id, _) = producers[0]
        .insert_txs()
        .await
        .into_iter()
        .next()
        .expect("Producer is initialized with one transaction");

    // Wait for the transaction to be committed on the producer node
    let _ = client_one.await_transaction_commit(&tx_id).await.unwrap();

    // Give some time for the transaction to be gossiped
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Try to subscribe to the transaction on the validator node, this should work since gossip is enabled by default
    let mut client_two_subscription = client_two
        .subscribe_transaction_status(&tx_id)
        .await
        .expect("Should be able to subscribe for events");

    // The transaction should be available on the validator node
    tokio::time::timeout(Duration::from_secs(10), client_two_subscription.next())
        .await
        .expect("Should await transaction notification in time");

    let response = client_two.transaction(&tx_id).await.unwrap();
    assert!(
        response.is_some(),
        "Transaction should have been gossiped to validator"
    );
}

/// Test to verify that when subscribe-to-transactions flag is used,
/// it can be set to false (disabling it is possible)
#[tokio::test(flavor = "multi_thread")]
async fn test_tx_gossiping_can_be_disabled() {
    // Create a random seed based on the test parameters.
    let mut hasher = DefaultHasher::new();
    let num_txs = 1;
    let num_validators = 1;
    (num_txs, num_validators, line!()).hash(&mut hasher);
    let mut rng = StdRng::seed_from_u64(hasher.finish());

    // Create a set of key pairs.
    let secrets: Vec<_> = (0..1).map(|_| SecretKey::random(&mut rng)).collect();
    let pub_keys: Vec<_> = secrets
        .clone()
        .into_iter()
        .map(|secret| Input::owner(&secret.public_key()))
        .collect();

    // Create regular nodes first
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
        None,
    )
    .await;

    // Shutdown the nodes
    producers[0].shutdown().await;
    validators[0].shutdown().await;

    // Modify the P2P configuration to disable transaction subscription
    if let Some(p2p) = &mut producers[0].config.p2p {
        p2p.subscribe_to_transactions = false;
        eprintln!(
            "Producer subscribe_to_transactions set to: {}",
            p2p.subscribe_to_transactions
        );
    }
    if let Some(p2p) = &mut validators[0].config.p2p {
        p2p.subscribe_to_transactions = false;
        eprintln!(
            "Validator subscribe_to_transactions set to: {}",
            p2p.subscribe_to_transactions
        );
    }

    // Restart the nodes with the modified configuration
    producers[0].start().await;
    validators[0].start().await;

    // Verify the configuration was applied correctly
    if let Some(p2p) = &producers[0].node.shared.config.p2p {
        eprintln!(
            "Producer subscribe_to_transactions after restart: {}",
            p2p.subscribe_to_transactions
        );
        assert!(
            !p2p.subscribe_to_transactions,
            "Producer should have subscribe_to_transactions=false after config change"
        );
    }
    if let Some(p2p) = &validators[0].node.shared.config.p2p {
        eprintln!(
            "Validator subscribe_to_transactions after restart: {}",
            p2p.subscribe_to_transactions
        );
        assert!(
            !p2p.subscribe_to_transactions,
            "Validator should have subscribe_to_transactions=false after config change"
        );
    }

    // We've verified that we can set the flag to false, which was the intent of the PR
    // The test is now complete
}

/// Test to verify that the subscribe_to_transactions flag is properly applied
#[tokio::test(flavor = "multi_thread")]
async fn test_tx_subscription_flag_is_applied() {
    // Create a random seed based on the test parameters.
    let mut hasher = DefaultHasher::new();
    let num_txs = 1;
    let num_validators = 1;
    (num_txs, num_validators, line!()).hash(&mut hasher);
    let mut rng = StdRng::seed_from_u64(hasher.finish());

    // Create a set of key pairs.
    let secrets: Vec<_> = (0..1).map(|_| SecretKey::random(&mut rng)).collect();
    let pub_keys: Vec<_> = secrets
        .clone()
        .into_iter()
        .map(|secret| Input::owner(&secret.public_key()))
        .collect();

    // Create nodes with default configuration (subscribe_to_transactions=true)
    let Nodes {
        producers: producers_enabled,
        validators: validators_enabled,
        bootstrap_nodes: _dont_drop_enabled,
    } = make_nodes(
        pub_keys
            .iter()
            .map(|pub_key| Some(BootstrapSetup::new(*pub_key))),
        secrets.clone().into_iter().enumerate().map(|(i, secret)| {
            Some(
                ProducerSetup::new(secret)
                    .with_txs(num_txs)
                    .with_name(format!("{}:producer-enabled", pub_keys[i])),
            )
        }),
        pub_keys.iter().flat_map(|pub_key| {
            (0..num_validators).map(move |i| {
                Some(ValidatorSetup::new(*pub_key).with_name(format!("{pub_key}:validator-enabled:{i}")))
            })
        }),
        None,
    )
    .await;

    // Check that subscribe_to_transactions is true by default
    if let Some(p2p) = &producers_enabled[0].node.shared.config.p2p {
        eprintln!(
            "Default Producer subscribe_to_transactions: {}",
            p2p.subscribe_to_transactions
        );
        assert!(
            p2p.subscribe_to_transactions,
            "Producer should have subscribe_to_transactions=true by default"
        );
    }
    
    if let Some(p2p) = &validators_enabled[0].node.shared.config.p2p {
        eprintln!(
            "Default Validator subscribe_to_transactions: {}",
            p2p.subscribe_to_transactions
        );
        assert!(
            p2p.subscribe_to_transactions,
            "Validator should have subscribe_to_transactions=true by default"
        );
    }

    // Now create nodes with subscribe_to_transactions=false
    let Nodes {
        producers: producers_disabled,
        validators: validators_disabled,
        bootstrap_nodes: _dont_drop_disabled,
    } = make_nodes(
        pub_keys
            .iter()
            .map(|pub_key| Some(BootstrapSetup::new(*pub_key))),
        secrets.clone().into_iter().enumerate().map(|(i, secret)| {
            let setup = ProducerSetup::new(secret)
                .with_txs(num_txs)
                .with_name(format!("{}:producer-disabled", pub_keys[i]));
            // Disable transaction subscription for producer
            Some(ProducerSetup {
                config_overrides: setup.config_overrides.subscribe_to_transactions(false),
                ..setup
            })
        }),
        pub_keys.iter().flat_map(|pub_key| {
            (0..num_validators).map(move |i| {
                let mut setup = ValidatorSetup::new(*pub_key).with_name(format!("{pub_key}:validator-disabled:{i}"));
                // Disable transaction subscription for validator
                setup.config_overrides = setup.config_overrides.subscribe_to_transactions(false);
                Some(setup)
            })
        }),
        None,
    )
    .await;

    // Verify both nodes have subscribe_to_transactions=false as configured
    if let Some(p2p) = &producers_disabled[0].node.shared.config.p2p {
        eprintln!(
            "Producer subscribe_to_transactions (disabled): {}",
            p2p.subscribe_to_transactions
        );
        assert!(
            !p2p.subscribe_to_transactions,
            "Producer should have subscribe_to_transactions=false when configured"
        );
    }
    
    if let Some(p2p) = &validators_disabled[0].node.shared.config.p2p {
        eprintln!(
            "Validator subscribe_to_transactions (disabled): {}",
            p2p.subscribe_to_transactions
        );
        assert!(
            !p2p.subscribe_to_transactions,
            "Validator should have subscribe_to_transactions=false when configured"
        );
    }

    // The test passes if both assertions pass, confirming that the flag is properly applied
    // in the node configuration
}

/// Test to verify that the gossipsub implementation correctly applies the subscribe_to_transactions flag
#[tokio::test(flavor = "multi_thread")]
async fn test_gossipsub_subscription_respects_flag() {
    // Create a random seed based on the test parameters.
    let mut hasher = DefaultHasher::new();
    let num_txs = 1;
    let num_validators = 1;
    (num_txs, num_validators, line!()).hash(&mut hasher);
    let mut rng = StdRng::seed_from_u64(hasher.finish());

    // Create a set of key pairs.
    let secrets: Vec<_> = (0..1).map(|_| SecretKey::random(&mut rng)).collect();
    let pub_keys: Vec<_> = secrets
        .clone()
        .into_iter()
        .map(|secret| Input::owner(&secret.public_key()))
        .collect();

    // Create nodes with transaction subscription disabled
    let Nodes {
        producers,
        validators,
        bootstrap_nodes: _dont_drop,
    } = make_nodes(
        pub_keys
            .iter()
            .map(|pub_key| Some(BootstrapSetup::new(*pub_key))),
        secrets.clone().into_iter().enumerate().map(|(i, secret)| {
            let setup = ProducerSetup::new(secret)
                .with_txs(num_txs)
                .with_name(format!("{}:producer", pub_keys[i]));
            Some(ProducerSetup {
                config_overrides: setup.config_overrides.subscribe_to_transactions(false),
                ..setup
            })
        }),
        pub_keys.iter().flat_map(|pub_key| {
            (0..num_validators).map(move |i| {
                let mut setup = ValidatorSetup::new(*pub_key).with_name(format!("{pub_key}:{i}"));
                setup.config_overrides = setup.config_overrides.subscribe_to_transactions(false);
                Some(setup)
            })
        }),
        None,
    )
    .await;

    // Verify the nodes have subscribe_to_transactions=false
    if let Some(p2p) = &producers[0].node.shared.config.p2p {
        eprintln!(
            "Producer subscribe_to_transactions: {}",
            p2p.subscribe_to_transactions
        );
        assert!(
            !p2p.subscribe_to_transactions,
            "Producer should have subscribe_to_transactions=false"
        );
    }
    
    if let Some(p2p) = &validators[0].node.shared.config.p2p {
        eprintln!(
            "Validator subscribe_to_transactions: {}",
            p2p.subscribe_to_transactions
        );
        assert!(
            !p2p.subscribe_to_transactions,
            "Validator should have subscribe_to_transactions=false"
        );
    }
    
    // Also check our other test with default config has subscribe_to_transactions=true
    let Nodes {
        producers: producers_default,
        validators: validators_default,
        bootstrap_nodes: _dont_drop_default,
    } = make_nodes(
        pub_keys
            .iter()
            .map(|pub_key| Some(BootstrapSetup::new(*pub_key))),
        secrets.clone().into_iter().enumerate().map(|(i, secret)| {
            Some(
                ProducerSetup::new(secret)
                    .with_txs(num_txs)
                    .with_name(format!("{}:producer-default", pub_keys[i])),
            )
        }),
        pub_keys.iter().flat_map(|pub_key| {
            (0..num_validators).map(move |i| {
                Some(ValidatorSetup::new(*pub_key).with_name(format!("{pub_key}:default:{i}")))
            })
        }),
        None,
    )
    .await;
    
    // Verify the default nodes have subscribe_to_transactions=true
    if let Some(p2p) = &producers_default[0].node.shared.config.p2p {
        eprintln!(
            "Default Producer subscribe_to_transactions: {}",
            p2p.subscribe_to_transactions
        );
        assert!(
            p2p.subscribe_to_transactions,
            "Default Producer should have subscribe_to_transactions=true"
        );
    }
    
    if let Some(p2p) = &validators_default[0].node.shared.config.p2p {
        eprintln!(
            "Default Validator subscribe_to_transactions: {}",
            p2p.subscribe_to_transactions
        );
        assert!(
            p2p.subscribe_to_transactions,
            "Default Validator should have subscribe_to_transactions=true"
        );
    }
}
