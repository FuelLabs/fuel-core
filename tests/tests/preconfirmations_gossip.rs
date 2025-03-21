use std::{
    hash::{
        DefaultHasher,
        Hash,
        Hasher,
    },
    time::Duration,
};

use fuel_core::{
    p2p_test_helpers::{
        make_nodes,
        BootstrapSetup,
        Nodes,
        ProducerSetup,
        ValidatorSetup,
    },
    service::Config,
};
use fuel_core_client::client::{
    types::TransactionStatus,
    FuelClient,
};
use fuel_core_poa::Trigger;
use fuel_core_types::{
    fuel_asm::{
        op,
        RegId,
    },
    fuel_tx::{
        Address,
        AssetId,
        Input,
        Output,
        Receipt,
        TransactionBuilder,
        TxPointer,
        UniqueIdentifier,
    },
    fuel_types::BlockHeight,
    fuel_vm::SecretKey,
};
use futures::StreamExt;
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};

fn config_with_preconfirmations(block_production_period: Duration) -> Config {
    let mut config = Config::local_node();

    config.p2p.as_mut().unwrap().subscribe_to_pre_confirmations = true;
    config.txpool.pending_pool_tx_ttl = Duration::from_secs(1);
    config
        .pre_confirmation_signature_service
        .echo_delegation_interval = Duration::from_millis(100);
    config.block_production = Trigger::Open {
        period: block_production_period,
    };

    config
}

#[tokio::test(flavor = "multi_thread")]
async fn preconfirmation__propagate_p2p_after_successful_execution() {
    let mut rng = rand::thread_rng();
    let address = Address::new([0; 32]);
    let block_production_period = Duration::from_secs(8);
    let gas_limit = 1_000_000;
    let amount = 10;

    // Given
    let script = [
        op::addi(0x10, RegId::ZERO, 0xca),
        op::addi(0x11, RegId::ZERO, 0xba),
        op::log(0x10, 0x11, RegId::ZERO, RegId::ZERO),
        op::ret(RegId::ONE),
    ];
    let script: Vec<u8> = script
        .iter()
        .flat_map(|op| u32::from(*op).to_be_bytes())
        .collect();

    // Given
    let tx = TransactionBuilder::script(script, vec![])
        .script_gas_limit(gas_limit)
        .add_unsigned_coin_input(
            SecretKey::random(&mut rng),
            rng.gen(),
            amount,
            AssetId::default(),
            Default::default(),
        )
        .add_output(Output::change(address, 0, AssetId::default()))
        .finalize_as_transaction();
    let tx_id = tx.id(&Default::default());
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
        producers: _producers,
        validators,
        bootstrap_nodes: _dont_drop,
    } = make_nodes(
        pub_keys
            .iter()
            .map(|pub_key| Some(BootstrapSetup::new(*pub_key))),
        secrets.clone().into_iter().enumerate().map(|(i, secret)| {
            Some(
                ProducerSetup::new(secret)
                    .with_name(format!("{}:producer", pub_keys[i]))
                    .utxo_validation(false),
            )
        }),
        pub_keys.iter().flat_map(|pub_key| {
            (0..num_validators).map(move |i| {
                Some(
                    ValidatorSetup::new(*pub_key)
                        .with_name(format!("{pub_key}:{i}"))
                        .utxo_validation(false),
                )
            })
        }),
        Some(config_with_preconfirmations(block_production_period)),
    )
    .await;

    let sentry = &validators[0];

    // Sleep to let time for exchange preconfirmations delegate public keys
    tokio::time::sleep(Duration::from_secs(1)).await;

    // When
    let client_sentry = FuelClient::from(sentry.node.bound_address);
    let mut tx_statuses_subscriber = client_sentry
        .submit_and_await_status(&tx)
        .await
        .expect("Should be able to subscribe for events");

    // Then
    assert!(matches!(
        tx_statuses_subscriber.next().await.unwrap().unwrap(),
        TransactionStatus::Submitted { .. }
    ));

    let status =
        tokio::time::timeout(Duration::from_secs(1), tx_statuses_subscriber.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    if let TransactionStatus::PreconfirmationSuccess {
        tx_pointer,
        total_fee,
        total_gas: _,
        transaction_id,
        receipts,
        resolved_outputs,
    } = status.clone()
    {
        // Then
        assert_eq!(tx_pointer, TxPointer::new(BlockHeight::new(1), 1));
        assert_eq!(total_fee, 0);
        assert_eq!(transaction_id, tx_id);
        let receipts = receipts.unwrap();
        assert_eq!(receipts.len(), 3);
        assert!(matches!(receipts[0],
            Receipt::Log {
                ra, rb, ..
            } if ra == 0xca && rb == 0xba));

        assert!(matches!(receipts[1],
            Receipt::Return {
                val, ..
            } if val == 1));
        let outputs = resolved_outputs.unwrap();
        assert_eq!(outputs.len(), 1);
        assert_eq!(
            outputs[0],
            Output::Change {
                to: address,
                amount,
                asset_id: AssetId::default()
            }
        );
    } else {
        panic!("Expected preconfirmation status, got {status:?}");
    }
    assert!(matches!(
        tx_statuses_subscriber.next().await.unwrap().unwrap(),
        TransactionStatus::Success { .. }
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn preconfirmation__propagate_p2p_after_failed_execution() {
    let mut rng = rand::thread_rng();
    let address = Address::new([0; 32]);
    let block_production_period = Duration::from_secs(8);
    let gas_limit = 1_000_000;
    let amount = 10;

    let script = [
        op::addi(0x10, RegId::ZERO, 0xca),
        op::addi(0x11, RegId::ZERO, 0xba),
        op::log(0x10, 0x11, RegId::ZERO, RegId::ZERO),
        op::rvrt(RegId::ONE),
        op::ret(RegId::ONE),
    ];
    let script: Vec<u8> = script
        .iter()
        .flat_map(|op| u32::from(*op).to_be_bytes())
        .collect();

    // Given
    let tx = TransactionBuilder::script(script, vec![])
        .script_gas_limit(gas_limit)
        .add_unsigned_coin_input(
            SecretKey::random(&mut rng),
            rng.gen(),
            amount,
            AssetId::default(),
            Default::default(),
        )
        .add_output(Output::change(address, 0, AssetId::default()))
        .finalize_as_transaction();
    let tx_id = tx.id(&Default::default());
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
        producers: _producers,
        validators,
        bootstrap_nodes: _dont_drop,
    } = make_nodes(
        pub_keys
            .iter()
            .map(|pub_key| Some(BootstrapSetup::new(*pub_key))),
        secrets.clone().into_iter().enumerate().map(|(i, secret)| {
            Some(
                ProducerSetup::new(secret)
                    .with_name(format!("{}:producer", pub_keys[i]))
                    .utxo_validation(false),
            )
        }),
        pub_keys.iter().flat_map(|pub_key| {
            (0..num_validators).map(move |i| {
                Some(
                    ValidatorSetup::new(*pub_key)
                        .with_name(format!("{pub_key}:{i}"))
                        .utxo_validation(false),
                )
            })
        }),
        Some(config_with_preconfirmations(block_production_period)),
    )
    .await;

    let sentry = &validators[0];

    // Sleep to let time for exchange preconfirmations delegate public keys
    tokio::time::sleep(Duration::from_secs(1)).await;

    // When
    let client_sentry = FuelClient::from(sentry.node.bound_address);
    let mut tx_statuses_subscriber = client_sentry
        .submit_and_await_status(&tx)
        .await
        .expect("Should be able to subscribe for events");

    assert!(matches!(
        tx_statuses_subscriber.next().await.unwrap().unwrap(),
        TransactionStatus::Submitted { .. }
    ));

    let status =
        tokio::time::timeout(Duration::from_secs(1), tx_statuses_subscriber.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();

    if let TransactionStatus::PreconfirmationFailure {
        tx_pointer,
        total_fee,
        total_gas: _,
        transaction_id,
        receipts,
        resolved_outputs,
        reason: _,
    } = status
    {
        // Then
        assert_eq!(tx_pointer, TxPointer::new(BlockHeight::new(1), 1));
        assert_eq!(total_fee, 0);
        assert_eq!(transaction_id, tx_id);
        let receipts = receipts.unwrap();
        assert_eq!(receipts.len(), 3);
        assert!(matches!(receipts[0],
            Receipt::Log {
                ra, rb, ..
            } if ra == 0xca && rb == 0xba));

        assert!(matches!(receipts[1],
            Receipt::Revert {
                ra, ..
            } if ra == 1));
        let outputs = resolved_outputs.unwrap();
        assert_eq!(outputs.len(), 1);
        assert_eq!(
            outputs[0],
            Output::Change {
                to: address,
                amount,
                asset_id: AssetId::default()
            }
        );
    } else {
        panic!("Expected preconfirmation status");
    }

    assert!(matches!(
        tx_statuses_subscriber.next().await.unwrap().unwrap(),
        TransactionStatus::Failure { .. }
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn preconfirmation__propagate_p2p_after_squeezed_out_on_producer() {
    let mut rng = rand::thread_rng();

    let block_production_period = Duration::from_secs(8);
    let gas_limit = 1_000_000;
    let tx = TransactionBuilder::script(
        vec![op::ret(RegId::ONE)].into_iter().collect(),
        vec![],
    )
    .script_gas_limit(gas_limit)
    .add_unsigned_coin_input(
        SecretKey::random(&mut rng),
        rng.gen(),
        10,
        Default::default(),
        Default::default(),
    )
    .add_output(Output::Change {
        to: Default::default(),
        amount: 0,
        asset_id: Default::default(),
    })
    .finalize_as_transaction();

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

    // Given
    // Disable UTXO validation in TxPool so that the transaction is squeezed out by
    // block production
    let Nodes {
        producers: _producers,
        validators,
        bootstrap_nodes: _dont_drop,
    } = make_nodes(
        pub_keys
            .iter()
            .map(|pub_key| Some(BootstrapSetup::new(*pub_key))),
        secrets.clone().into_iter().enumerate().map(|(i, secret)| {
            Some(
                ProducerSetup::new(secret)
                    .with_name(format!("{}:producer", pub_keys[i]))
                    .utxo_validation(true),
            )
        }),
        pub_keys.iter().flat_map(|pub_key| {
            (0..num_validators).map(move |i| {
                Some(
                    ValidatorSetup::new(*pub_key)
                        .with_name(format!("{pub_key}:{i}"))
                        .utxo_validation(false),
                )
            })
        }),
        Some(config_with_preconfirmations(block_production_period)),
    )
    .await;

    let sentry = &validators[0];

    // Sleep to let time for exchange preconfirmations delegate public keys
    tokio::time::sleep(Duration::from_secs(1)).await;

    // When
    let client_sentry = FuelClient::from(sentry.node.bound_address);
    let mut tx_statuses_subscriber = client_sentry
        .submit_and_await_status(&tx)
        .await
        .expect("Should be able to subscribe for events");

    assert!(matches!(
        tx_statuses_subscriber.next().await.unwrap().unwrap(),
        TransactionStatus::Submitted { .. }
    ));
    tracing::info!("Submitted");

    assert!(matches!(
        tx_statuses_subscriber.next().await.unwrap().unwrap(),
        TransactionStatus::SqueezedOut { .. }
    ));
    tracing::info!("SqueezedOut");
}
