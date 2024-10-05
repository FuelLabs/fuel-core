use crate::tests_helper::{
    default_multiaddr,
    GenesisFuelCoreDriver,
    LatestFuelCoreDriver,
    Version36FuelCoreDriver,
    IGNITION_TESTNET_SNAPSHOT,
    POA_SECRET_KEY,
    V36_TESTNET_SNAPSHOT,
};
use latest_fuel_core_type::{
    fuel_tx::Transaction,
    services::{
        block_producer::Components,
        executor::{
            Error as ExecutorError,
            TransactionValidityError,
        },
    },
};
use libp2p::{
    futures::StreamExt,
    identity::{
        secp256k1::Keypair as SecpKeypair,
        Keypair,
    },
    PeerId,
};
use std::time::Duration;

const BLOCK_INCLUSION_TIMEOUT: Duration = Duration::from_secs(360);

#[tokio::test]
async fn latest_binary_is_backward_compatible_and_can_load_testnet_config() {
    // When
    let latest_node = LatestFuelCoreDriver::spawn(&[
        "--debug",
        "--poa-instant",
        "true",
        "--snapshot",
        IGNITION_TESTNET_SNAPSHOT,
        // We need to set the native executor version to 1 to be
        // sure it is not zero to force the usage of the WASM executor
        "--native-executor-version",
        "1",
    ])
    .await;

    // Then
    let latest_node = latest_node.expect("Failed to spawn latest node");
    assert!(latest_node.node.state().started())
}

#[tokio::test]
async fn latest_binary_is_backward_compatible_and_follows_blocks_created_by_genesis_binary(
) {
    // Given
    let genesis_keypair = SecpKeypair::generate();
    let hexed_secret = hex::encode(genesis_keypair.secret().to_bytes());
    let genesis_port = "30333";
    let genesis_node = GenesisFuelCoreDriver::spawn(&[
        "--service-name",
        "GenesisProducer",
        "--debug",
        "--poa-instant",
        "true",
        "--consensus-key",
        POA_SECRET_KEY,
        "--snapshot",
        IGNITION_TESTNET_SNAPSHOT,
        "--enable-p2p",
        "--keypair",
        hexed_secret.as_str(),
        "--peering-port",
        genesis_port,
    ])
    .await
    .unwrap();
    let public_key = Keypair::from(genesis_keypair).public();
    let genesis_peer_id = PeerId::from_public_key(&public_key);
    let genesis_multiaddr = default_multiaddr(genesis_port, genesis_peer_id);

    // Starting node that uses latest fuel core.
    // It will connect to the genesis node and sync blocks.
    let latest_keypair = SecpKeypair::generate();
    let hexed_secret = hex::encode(latest_keypair.secret().to_bytes());
    let latest_node = LatestFuelCoreDriver::spawn(&[
        "--service-name",
        "LatestValidator",
        "--debug",
        "--poa-instant",
        "false",
        "--snapshot",
        IGNITION_TESTNET_SNAPSHOT,
        "--enable-p2p",
        "--keypair",
        hexed_secret.as_str(),
        "--reserved-nodes",
        genesis_multiaddr.as_str(),
        "--peering-port",
        "0",
    ])
    .await
    .unwrap();
    let mut imported_blocks = latest_node.node.shared.block_importer.events();

    // When
    const BLOCKS_TO_PRODUCE: u32 = 10;
    genesis_node
        .client
        .produce_blocks(BLOCKS_TO_PRODUCE, None)
        .await
        .unwrap();

    // Then
    for i in 0..BLOCKS_TO_PRODUCE {
        let _ = tokio::time::timeout(BLOCK_INCLUSION_TIMEOUT, imported_blocks.next())
            .await
            .expect(format!("Timed out waiting for block import {i}").as_str())
            .expect(format!("Failed to import block {i}").as_str());
    }
}

#[tokio::test]
async fn latest_binary_is_backward_compatible_and_follows_blocks_created_by_v36_binary() {
    // Given
    let v36_keypair = SecpKeypair::generate();
    let hexed_secret = hex::encode(v36_keypair.secret().to_bytes());
    let v36_port = "30333";
    let v36_node = Version36FuelCoreDriver::spawn(&[
        "--service-name",
        "V36Producer",
        "--debug",
        "--poa-instant",
        "true",
        "--consensus-key",
        POA_SECRET_KEY,
        "--snapshot",
        V36_TESTNET_SNAPSHOT,
        "--enable-p2p",
        "--keypair",
        hexed_secret.as_str(),
        "--peering-port",
        v36_port,
    ])
    .await
    .unwrap();
    let public_key = Keypair::from(v36_keypair).public();
    let v36_peer_id = PeerId::from_public_key(&public_key);
    let v36_multiaddr = default_multiaddr(v36_port, v36_peer_id);

    // Starting node that uses latest fuel core.
    // It will connect to the v36 node and sync blocks.
    let latest_keypair = SecpKeypair::generate();
    let hexed_secret = hex::encode(latest_keypair.secret().to_bytes());
    let latest_node = LatestFuelCoreDriver::spawn(&[
        "--service-name",
        "LatestValidator",
        "--debug",
        "--poa-instant",
        "false",
        "--snapshot",
        V36_TESTNET_SNAPSHOT,
        "--enable-p2p",
        "--keypair",
        hexed_secret.as_str(),
        "--reserved-nodes",
        v36_multiaddr.as_str(),
        "--peering-port",
        "0",
    ])
    .await
    .unwrap();
    let mut imported_blocks = latest_node.node.shared.block_importer.events();

    // When
    const BLOCKS_TO_PRODUCE: u32 = 10;
    v36_node
        .client
        .produce_blocks(BLOCKS_TO_PRODUCE, None)
        .await
        .unwrap();

    // Then
    for i in 0..BLOCKS_TO_PRODUCE {
        let _ = tokio::time::timeout(BLOCK_INCLUSION_TIMEOUT, imported_blocks.next())
            .await
            .expect(format!("Timed out waiting for block import {i}").as_str())
            .expect(format!("Failed to import block {i}").as_str());
    }
}

#[tokio::test]
async fn latest_binary_is_backward_compatible_and_can_deserialize_errors_from_genesis_binary(
) {
    // Given
    let genesis_keypair = SecpKeypair::generate();
    let hexed_secret = hex::encode(genesis_keypair.secret().to_bytes());
    let genesis_port = "30333";
    let node_with_genesis_transition = LatestFuelCoreDriver::spawn(&[
        "--service-name",
        "GenesisProducer",
        "--debug",
        "--poa-instant",
        "true",
        "--consensus-key",
        POA_SECRET_KEY,
        "--snapshot",
        IGNITION_TESTNET_SNAPSHOT,
        "--enable-p2p",
        "--keypair",
        hexed_secret.as_str(),
        "--peering-port",
        genesis_port,
        "--utxo-validation",
    ])
    .await
    .unwrap();

    // When
    let invalid_transaction = Transaction::default_test_tx();
    let mut component: Components<Vec<Transaction>> = Default::default();
    component.header_to_produce.consensus.height = 1u32.into();
    // Use version of the genesis state transition
    component
        .header_to_produce
        .application
        .state_transition_bytecode_version = 0;
    component.transactions_source = vec![invalid_transaction];
    let result = node_with_genesis_transition
        .node
        .shared
        .executor
        .produce_without_commit_from_vector(component);

    // Then
    let result = result.expect("Should dry run without error").into_result();
    assert_eq!(result.skipped_transactions.len(), 1);
    assert!(matches!(
        result.skipped_transactions[0].1,
        ExecutorError::TransactionValidity(TransactionValidityError::CoinDoesNotExist(_))
    ));
}
