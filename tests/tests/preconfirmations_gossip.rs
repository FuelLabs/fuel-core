use std::hash::{
    DefaultHasher,
    Hash,
    Hasher,
};

use fuel_core::p2p_test_helpers::{
    make_nodes,
    BootstrapSetup,
    Nodes,
    ProducerSetup,
    ValidatorSetup,
};
use fuel_core_client::client::{
    types::TransactionStatus,
    FuelClient,
};
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
    SeedableRng,
};

#[tokio::test(flavor = "multi_thread")]
async fn preconfirmation__propagate_p2p_after_execution() {
    let address = Address::new([0; 32]);
    let gas_limit = 1_000_000;
    let maturity = Default::default();

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
        .maturity(maturity)
        .add_fee_input()
        .add_output(Output::variable(address, 0, AssetId::default()))
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
        producers: _dont_drop1,
        validators,
        bootstrap_nodes: _dont_drop2,
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
        None,
    )
    .await;

    let sentry = &validators[0];
    let authority = &validators[0];

    // When
    let client_sentry = FuelClient::from(sentry.node.bound_address);
    sentry
        .node
        .submit(tx)
        .await
        .expect("Should accept invalid transaction because `utxo_validation = false`.");
    let mut tx_statuses_subscriber = client_sentry
        .subscribe_transaction_status(&tx_id)
        .await
        .expect("Should be able to subscribe for events");
    authority
        .node
        .shared
        .poa_adapter
        .manually_produce_blocks(
            None,
            fuel_core_poa::service::Mode::Blocks {
                number_of_blocks: 1,
            },
        )
        .await
        .unwrap();

    // Then
    assert!(matches!(
        tx_statuses_subscriber.next().await.unwrap().unwrap(),
        TransactionStatus::Submitted { .. }
    ));
    if let TransactionStatus::PreconfirmationSuccess {
        tx_pointer,
        total_fee,
        total_gas,
        transaction_id,
        receipts,
        resolved_outputs,
    } = tx_statuses_subscriber.next().await.unwrap().unwrap()
    {
        // Then
        assert_eq!(tx_pointer, TxPointer::new(BlockHeight::new(1), 0));
        assert_eq!(total_fee, 0);
        assert_eq!(total_gas, 4450);
        assert_eq!(transaction_id, tx_id);
        let receipts = receipts.unwrap();
        assert_eq!(receipts.len(), 2);
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
            Output::Coin {
                to: address,
                amount: 2,
                asset_id: AssetId::default()
            }
        );
    } else {
        panic!("Expected preconfirmation status");
    }
    assert!(matches!(
        tx_statuses_subscriber.next().await.unwrap().unwrap(),
        TransactionStatus::Success { .. }
    ));
}

#[tokio::test]
async fn preconfirmation__propagate_p2p_after_failed_execution() {
    let address = Address::new([0; 32]);
    let gas_limit = 1_000_000;
    let maturity = Default::default();

    // Given
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
        .maturity(maturity)
        .add_fee_input()
        .add_output(Output::variable(address, 0, AssetId::default()))
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
        producers: _dont_drop1,
        validators,
        bootstrap_nodes: _dont_drop2,
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
        None,
    )
    .await;

    let sentry = &validators[0];
    let authority = &validators[0];

    // When
    let client_sentry = FuelClient::from(sentry.node.bound_address);
    sentry
        .node
        .submit(tx)
        .await
        .expect("Should accept invalid transaction because `utxo_validation = false`.");
    let mut tx_statuses_subscriber = client_sentry
        .subscribe_transaction_status(&tx_id)
        .await
        .expect("Should be able to subscribe for events");
    authority
        .node
        .shared
        .poa_adapter
        .manually_produce_blocks(
            None,
            fuel_core_poa::service::Mode::Blocks {
                number_of_blocks: 1,
            },
        )
        .await
        .unwrap();

    // Then
    assert!(matches!(
        tx_statuses_subscriber.next().await.unwrap().unwrap(),
        TransactionStatus::Submitted { .. }
    ));
    if let TransactionStatus::PreconfirmationFailure {
        tx_pointer,
        total_fee,
        total_gas,
        transaction_id,
        receipts,
        resolved_outputs,
        reason: _,
    } = tx_statuses_subscriber.next().await.unwrap().unwrap()
    {
        // Then
        assert_eq!(tx_pointer, TxPointer::new(BlockHeight::new(1), 0));
        assert_eq!(total_fee, 0);
        assert_eq!(total_gas, 4450);
        assert_eq!(transaction_id, tx_id);
        let receipts = receipts.unwrap();
        assert_eq!(receipts.len(), 2);
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
            Output::Coin {
                to: address,
                amount: 2,
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

#[tokio::test]
async fn preconfirmation__propagate_p2p_after_squeezed_out() {
    let address = Address::new([0; 32]);
    let gas_limit = 1_000_000;
    let maturity = Default::default();

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
        .maturity(maturity)
        .add_fee_input()
        .add_output(Output::variable(address, 0, AssetId::default()))
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
        producers: _dont_drop1,
        validators,
        bootstrap_nodes: _dont_drop2,
    } = make_nodes(
        pub_keys
            .iter()
            .map(|pub_key| Some(BootstrapSetup::new(*pub_key))),
        secrets.clone().into_iter().enumerate().map(|(i, secret)| {
            Some(
                ProducerSetup::new(secret).with_name(format!("{}:producer", pub_keys[i])),
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
        None,
    )
    .await;

    let sentry = &validators[0];
    let authority = &validators[0];

    // When
    let client_sentry = FuelClient::from(sentry.node.bound_address);
    sentry
        .node
        .submit(tx)
        .await
        .expect("Should accept invalid transaction because `utxo_validation = false`.");
    let mut tx_statuses_subscriber = client_sentry
        .subscribe_transaction_status(&tx_id)
        .await
        .expect("Should be able to subscribe for events");
    authority
        .node
        .shared
        .poa_adapter
        .manually_produce_blocks(
            None,
            fuel_core_poa::service::Mode::Blocks {
                number_of_blocks: 1,
            },
        )
        .await
        .unwrap();

    // Then
    assert!(matches!(
        tx_statuses_subscriber.next().await.unwrap().unwrap(),
        TransactionStatus::Submitted { .. }
    ));
    if let TransactionStatus::PreconfirmationSqueezedOut {
        reason,
        transaction_id,
    } = tx_statuses_subscriber.next().await.unwrap().unwrap()
    {
        assert_eq!(transaction_id, tx_id);
        assert_eq!(reason, "Transaction has been skipped during block insertion: \
        The specified coin(0xc49d65de61cf04588a764b557d25cc6c6b4bc0d7429227e2a21e61c213b3a3e28212) doesn't exist".to_string())
    } else {
        panic!("Expected preconfirmation status");
    }
    assert!(matches!(
        tx_statuses_subscriber.next().await.unwrap().unwrap(),
        TransactionStatus::SqueezedOut { .. }
    ));
}
