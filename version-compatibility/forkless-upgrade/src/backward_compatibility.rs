use crate::tests_helper::{
    default_multiaddr,
    GenesisFuelCoreDriver,
    LatestFuelCoreDriver,
    IGNITION_SNAPSHOT,
    POA_SECRET_KEY,
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

#[tokio::test]
async fn latest_binary_is_backward_compatible_and_can_load_testnet_config() {
    // When
    let latest_node = LatestFuelCoreDriver::spawn(&[
        "--debug",
        "--poa-instant",
        "true",
        "--snapshot",
        IGNITION_SNAPSHOT,
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
        IGNITION_SNAPSHOT,
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
        IGNITION_SNAPSHOT,
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
        let _ = tokio::time::timeout(Duration::from_secs(120), imported_blocks.next())
            .await
            .expect(format!("Timed out waiting for block import {i}").as_str())
            .expect(format!("Failed to import block {i}").as_str());
    }
}
