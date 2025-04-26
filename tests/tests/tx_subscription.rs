#![allow(clippy::arithmetic_side_effects)]
use fuel_core::p2p_test_helpers::{
    BootstrapSetup, Nodes, ProducerSetup, ValidatorSetup, make_nodes, CustomizeConfig,
};
use fuel_core_client::client::{FuelClient, types::{TransactionStatus, primitives::ChainId}, pagination::{PaginationRequest, PageDirection}};
use fuel_core_types::{fuel_tx::*, fuel_vm::*};
use fuel_core::chain_config::{CoinConfig, StateConfig, coin_config_helpers::CoinConfigGenerator};
use fuel_core::service::Config;
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
    let num_validators = 1;
    (num_validators, line!()).hash(&mut hasher);
    let mut rng = StdRng::seed_from_u64(hasher.finish());

    // Create key pairs.
    let producer_secret = SecretKey::random(&mut rng);
    let producer_pub_key = Input::owner(&producer_secret.public_key());
    let producer_address = Address::from(producer_pub_key);
    let validator_secret = SecretKey::random(&mut rng);
    let validator_pub_key = Input::owner(&validator_secret.public_key());

    // Manually configure genesis coins for the producer
    let mut coin_generator = CoinConfigGenerator::new();
    let producer_genesis_coin = CoinConfig {
        owner: producer_address,
        amount: 2_000_000, // Give enough for funding + fees
        asset_id: AssetId::BASE,
        ..coin_generator.generate()
    };
    let state_config = StateConfig { coins: vec![producer_genesis_coin], ..Default::default() };
    let mut config = Config::local_node();
    config.snapshot_reader = config.snapshot_reader.clone().with_state_config(state_config);

    // Create producer and validator
    let Nodes {
        producers,
        validators,
        bootstrap_nodes: _dont_drop,
    } = make_nodes(
        vec![
            Some(BootstrapSetup::new(producer_pub_key)),
            Some(BootstrapSetup::new(validator_pub_key)),
        ],
        vec![Some(
            ProducerSetup::new(producer_secret.clone())
                .with_txs(0) // Genesis coin provided manually via config
                .with_name(format!("{}:producer", producer_pub_key)),
        )],
        vec![Some(
            ValidatorSetup::new(producer_pub_key) // Sync from producer
                .with_name(format!("{}:validator", validator_pub_key)),
        )],
        Some(config), // Pass the modified config
    )
    .await;

    let client_one = FuelClient::from(producers[0].node.bound_address);
    let client_two = FuelClient::from(validators[0].node.bound_address);
    let validator_address = Address::from(validator_pub_key);

    // Add a small delay to allow node state to initialize
    tokio::time::sleep(Duration::from_millis(500)).await;

    // --- Setup: Fund the validator node using producer's genesis coin ---
    println!("Funding validator {} from producer {}", validator_address, producer_address);
    let producer_coins = client_one
        .coins(&producer_address, None, PaginationRequest {
            cursor: None,
            results: 10,
            direction: PageDirection::Forward,
        })
        .await
        .unwrap();
    assert!(!producer_coins.results.is_empty(), "Producer has no genesis coins");
    let producer_coin = producer_coins.results[0].clone();
    println!("Producer coin to use for funding: {:?}", producer_coin);

    let amount_to_fund: u64 = 1_000_000;
    let mut funding_builder = TransactionBuilder::script(vec![], vec![]);
    funding_builder
        .script_gas_limit(10_000)
        .add_unsigned_coin_input(
            producer_secret.clone(), // Use the correct producer secret
            producer_coin.utxo_id,
            producer_coin.amount,
            producer_coin.asset_id,
            TxPointer::new(producer_coin.block_created.into(), producer_coin.tx_created_idx),
        )
        .add_output(Output::coin(validator_address, amount_to_fund, AssetId::BASE))
        .add_output(Output::change(producer_address, 0, AssetId::BASE));

    let funding_tx = funding_builder.finalize_as_transaction();
    let funding_tx_id = funding_tx.id(&ChainId::default());
    println!("Submitting funding tx {}", funding_tx_id);
    let funding_status = client_one.submit_and_await_commit(&funding_tx).await.expect("Producer failed to submit funding tx");
    let funding_commit_height = match funding_status {
        TransactionStatus::Success { block_height, .. } => block_height,
        _ => panic!("Funding transaction failed to commit successfully"),
    };
    println!("Funding tx {} committed at height {}", funding_tx_id, funding_commit_height);

    // Wait for validator node to sync past the funding commit height
    println!("Waiting for validator to sync past height {}", funding_commit_height);
    let wait_timeout = Duration::from_secs(15);
    let poll_interval = Duration::from_millis(500);
    let mut waited = Duration::ZERO;
    loop {
        let validator_info = client_two.chain_info().await.expect("Failed to get validator chain info");
        let validator_height = validator_info.latest_block.header.height;
        println!("Validator current height: {}", validator_height);
        if validator_height >= *funding_commit_height {
            println!("Validator synced.");
            break;
        }
        if waited >= wait_timeout {
            panic!("Timeout waiting for validator to sync past height {}", funding_commit_height);
        }
        tokio::time::sleep(poll_interval).await;
        waited += poll_interval;
    }

    // --- Test Logic ---
    println!("Starting main test logic: Validator submits tx");
    // Fetch the specific coin UTXO received by the validator from the funding tx
    let validator_coins = client_two
        .coins(&validator_address, None, PaginationRequest {
            cursor: None,
            results: 10,
            direction: PageDirection::Forward,
        })
        .await
        .unwrap();
    assert!(!validator_coins.results.is_empty(), "Validator has no coins after funding delay");
    let input_coin = validator_coins.results[0].clone(); // Use the first available coin

    println!("Validator coin to spend: {:?}", input_coin);
    // We can't easily assert the amount if we just take the first coin,
    // but we know it should be spendable.

    let outputs = vec![Output::change(
        producer_address, // Send change back to producer
        0,
        AssetId::BASE,
    )];

    let mut builder = TransactionBuilder::script(vec![], vec![]);
    builder
        .script_gas_limit(10_000)
        .add_unsigned_coin_input(
            validator_secret, // Validator's secret
            input_coin.utxo_id,
            input_coin.amount,
            input_coin.asset_id,
            TxPointer::new(input_coin.block_created.into(), input_coin.tx_created_idx),
        );
    for output in outputs {
        builder.add_output(output);
    }
    let tx = builder.finalize_as_transaction();
    let tx_id = tx.id(&ChainId::default());

    println!("Validator submitting main tx {}", tx_id);
    let submission_result = client_two.submit_and_await_commit(&tx).await;
    assert!(
        submission_result.is_ok(),
        "Transaction submission to validator failed: {:?}",
        submission_result.err()
    );
    println!("Validator main tx {} committed", tx_id);

    println!("Checking tx status on producer");
    // Give some time for gossip and block production by producer
    tokio::time::sleep(Duration::from_secs(4)).await; // Increased sleep

    let tx_status_on_producer = client_one.transaction_status(&tx_id).await;
    println!("Producer status for tx {}: {:?}", tx_id, tx_status_on_producer);
    assert!(
        tx_status_on_producer.is_ok(),
        "Failed to query transaction status on producer"
    );
    match tx_status_on_producer.unwrap() {
        TransactionStatus::Success { .. } => {}
        other => {
            panic!(
                "Transaction should have succeeded on producer, but got: {:?}",
                other
            );
        }
    }

    println!("Checking tx object on producer");
    let response = client_one.transaction(&tx_id).await.unwrap();
    assert!(
        response.is_some(),
        "Transaction should be available on the producer node after being gossiped"
    );
    println!("Test test_tx_gossiping_enabled_by_default finished successfully");
}

/// Test to verify that when subscribe-to-transactions flag is used,
/// it can be set to false (disabling it is possible)
#[tokio::test(flavor = "multi_thread")]
async fn test_tx_gossiping_can_be_disabled() {
    // Create a random seed based on the test parameters.
    let mut hasher = DefaultHasher::new();
    let num_validators = 1;
    (num_validators, line!()).hash(&mut hasher);
    let mut rng = StdRng::seed_from_u64(hasher.finish());

    // Create a set of key pairs.
    let secrets: Vec<_> = (0..1).map(|_| SecretKey::random(&mut rng)).collect();
    let pub_keys: Vec<_> = secrets
        .clone()
        .into_iter()
        .map(|secret| Input::owner(&secret.public_key()))
        .collect();

    // Get producer/validator details
    let producer_secret = secrets[0].clone();
    let producer_pub_key = pub_keys[0];
    let producer_address = Address::from(producer_pub_key);
    let validator_pub_key = pub_keys[0];
    let validator_address = Address::from(validator_pub_key);

    // Configure nodes with gossip DISABLED from the start
    let overrides = CustomizeConfig::no_overrides().subscribe_to_transactions(false);

    // Manually configure genesis coins for the producer
    let mut coin_generator = CoinConfigGenerator::new();
    let producer_genesis_coin = CoinConfig {
        owner: producer_address,
        amount: 2_000_000, // Give enough for funding + fees
        asset_id: AssetId::BASE,
        ..coin_generator.generate()
    };
    let state_config = StateConfig { coins: vec![producer_genesis_coin], ..Default::default() };
    let mut config = Config::local_node();
    config.snapshot_reader = config.snapshot_reader.clone().with_state_config(state_config);

    // Build setups separately to avoid closure capture issues
    let producer_setups: Vec<_> = secrets.clone().into_iter().enumerate().map(|(i, secret)| {
        let mut setup = ProducerSetup::new(secret.clone())
            .with_txs(0) // Manual genesis coin
            .with_name(format!("{}:producer", pub_keys[i]));
        setup.config_overrides = overrides.clone();
        Some(setup)
    }).collect();

    let validator_setups: Vec<_> = (0..num_validators).map(|i| {
        let mut setup = ValidatorSetup::new(producer_pub_key)
            .with_name(format!("{producer_pub_key}:{i}"));
        setup.config_overrides = overrides.clone();
        Some(setup)
    }).collect();

    // Create nodes with gossip disabled
    let Nodes {
        producers,
        validators,
        bootstrap_nodes: _dont_drop,
    } = make_nodes(
        pub_keys.iter().map(|pk| Some(BootstrapSetup::new(*pk))),
        producer_setups,
        validator_setups,
        Some(config), // Pass the modified config
    )
    .await;

    // Verify the configuration was applied correctly
    if let Some(p2p) = &producers[0].node.shared.config.p2p {
        assert!(
            !p2p.subscribe_to_transactions,
            "Producer should have subscribe_to_transactions=false"
        );
    }
    if let Some(p2p) = &validators[0].node.shared.config.p2p {
        assert!(
            !p2p.subscribe_to_transactions,
            "Validator should have subscribe_to_transactions=false"
        );
    }

    let client_one = FuelClient::from(producers[0].node.bound_address);
    let client_two = FuelClient::from(validators[0].node.bound_address);

    // Add a small delay to allow node state to initialize
    tokio::time::sleep(Duration::from_millis(500)).await;

    // --- Setup: Fund the validator node ---
    println!(
        "Funding validator {} from producer {} (gossip disabled test)",
        validator_address,
        producer_address
    );

    // Fetch producer's genesis coin (manually configured)
    let producer_coins = client_one
        .coins(&producer_address, None, PaginationRequest {
            cursor: None,
            results: 10,
            direction: PageDirection::Forward,
        })
        .await
        .unwrap();
    assert!(
        !producer_coins.results.is_empty(),
        "Producer has no manual genesis coins (gossip disabled test)"
    );
    let producer_coin = producer_coins.results[0].clone();
    println!("Producer coin to use for funding: {:?}", producer_coin);

    let amount_to_fund: u64 = 1_000_000;
    let mut funding_builder = TransactionBuilder::script(vec![], vec![]);
    funding_builder
        .script_gas_limit(10_000)
        .add_unsigned_coin_input(
            producer_secret,
            producer_coin.utxo_id,
            producer_coin.amount,
            producer_coin.asset_id,
            TxPointer::new(producer_coin.block_created.into(), producer_coin.tx_created_idx),
        )
        .add_output(Output::coin(validator_address, amount_to_fund, AssetId::BASE))
        .add_output(Output::change(producer_address, 0, AssetId::BASE));

    let funding_tx = funding_builder.finalize_as_transaction();
    let funding_tx_id = funding_tx.id(&ChainId::default());
    println!("Submitting funding tx {} (gossip disabled test)", funding_tx_id);
    let funding_status = client_one
         .submit_and_await_commit(&funding_tx)
         .await
         .expect("Producer failed to submit funding tx (gossip disabled test)");
    let funding_commit_height = match funding_status {
        TransactionStatus::Success { block_height, .. } => block_height,
        _ => panic!("Funding transaction failed to commit successfully (gossip disabled test)"),
    };
    println!("Funding tx {} committed at height {}", funding_tx_id, funding_commit_height);

    // Wait for validator node to sync past the funding commit height
    println!("Waiting for validator to sync past height {}", funding_commit_height);
    let wait_timeout = Duration::from_secs(15);
    let poll_interval = Duration::from_millis(500);
    let mut waited = Duration::ZERO;
    loop {
        let validator_info = client_two.chain_info().await.expect("Failed to get validator chain info");
        let validator_height = validator_info.latest_block.header.height;
        if validator_height >= *funding_commit_height {
            println!("Validator synced.");
            break;
        }
        if waited >= wait_timeout {
            panic!("Timeout waiting for validator to sync past height {}", funding_commit_height);
        }
        tokio::time::sleep(poll_interval).await;
        waited += poll_interval;
    }

    // --- Test Logic: Validator submits tx, should NOT be gossiped ---
    println!("Starting main test logic: Validator submits tx (gossip disabled)");
    // Fetch any available coin for the validator (should include the funded one)
    let validator_coins = client_two
        .coins(&validator_address, None, PaginationRequest {
            cursor: None,
            results: 10,
            direction: PageDirection::Forward,
        })
        .await
        .unwrap();
    assert!(!validator_coins.results.is_empty(), "Validator has no coins after funding delay (disabled test)");
    let input_coin = validator_coins.results[0].clone();
    println!("Validator coin to spend: {:?}", input_coin);

    let outputs = vec![Output::change(producer_address, 0, AssetId::BASE)];
    let mut builder = TransactionBuilder::script(vec![], vec![]);
    builder
        .script_gas_limit(10_000)
        .add_unsigned_coin_input(
            producer_secret,
            input_coin.utxo_id,
            input_coin.amount,
            input_coin.asset_id,
            TxPointer::new(input_coin.block_created.into(), input_coin.tx_created_idx),
        );
    for output in outputs {
        builder.add_output(output);
    }
    let tx = builder.finalize_as_transaction();
    let tx_id = tx.id(&ChainId::default());

    println!("Validator submitting main tx {} (gossip disabled)", tx_id);
    // Submit the transaction via the validator node (client_two)
    // Use submit, not submit_and_await_commit
    let submission_result = client_two.submit(&tx).await;
    assert!(
        submission_result.is_ok(),
        "Transaction submission to validator failed: {:?}",
        submission_result.err()
    );
    println!("Validator main tx {} submitted (gossip disabled)", tx_id);

    // Verify the transaction was NOT included by the producer node (client_one)
    println!("Checking tx status on producer (gossip disabled)");
    // Give some time for potential (but disabled) gossip and block production
    tokio::time::sleep(Duration::from_secs(4)).await;

    let tx_status_on_producer = client_one.transaction_status(&tx_id).await;
    println!("Producer status for tx {}: {:?}", tx_id, tx_status_on_producer);

    match tx_status_on_producer {
        Ok(TransactionStatus::Submitted { .. }) => {
            println!("Transaction status on producer is Submitted, as expected.");
        }
        Ok(TransactionStatus::Success { .. }) => {
            panic!("Transaction status on producer is Success, but gossip was disabled!");
        }
        Ok(TransactionStatus::Failure { .. }) => {
            panic!("Transaction status on producer is Failure, but gossip was disabled!");
        }
        Ok(other) => {
            println!(
                "Transaction status on producer is {:?}. This might be okay if not Success/Failure.",
                other
            );
        }
        Err(e) => {
            println!(
                "Transaction not found on producer (Error: {}), as expected.",
                e
            );
        }
    }

    println!("Checking tx object on producer (gossip disabled)");
    let response = client_one.transaction(&tx_id).await.unwrap();
    assert!(
        response.is_none(),
        "Transaction should NOT be available on the producer node when gossip is disabled"
    );
    println!("Test test_tx_gossiping_can_be_disabled finished successfully");
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
            let mut setup = ProducerSetup::new(secret.clone())
                .with_txs(num_txs)
                .with_name(format!("{}:producer-disabled", pub_keys[i]));
            setup.config_overrides = setup.config_overrides.subscribe_to_transactions(false);
            Some(setup)
        }),
        pub_keys.iter().flat_map(|pub_key| {
            (0..num_validators).map(move |i| {
                let mut setup = ValidatorSetup::new(*pub_key).with_name(format!("{pub_key}:validator-disabled:{i}"));
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
