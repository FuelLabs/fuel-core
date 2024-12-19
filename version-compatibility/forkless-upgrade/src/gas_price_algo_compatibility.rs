#![allow(unused_imports)]

use crate::tests_helper::{
    default_multiaddr,
    LatestFuelCoreDriver,
    Version36FuelCoreDriver,
    IGNITION_TESTNET_SNAPSHOT,
    POA_SECRET_KEY,
};
use latest_fuel_core_gas_price_service::{
    common::{
        fuel_core_storage_adapter::storage::GasPriceMetadata,
        updater_metadata::UpdaterMetadata,
    },
    ports::GasPriceData,
    v1::metadata::V1Metadata,
};
use latest_fuel_core_storage::{
    transactional::AtomicView,
    StorageAsRef,
};
use libp2p::{
    identity::{
        secp256k1::Keypair as SecpKeypair,
        Keypair,
    },
    PeerId,
};
use std::ops::Deref;

#[tokio::test]
async fn v1_gas_price_metadata_updates_successfully_from_v0() {
    // Given
    let genesis_keypair = SecpKeypair::generate();
    let hexed_secret = hex::encode(genesis_keypair.secret().to_bytes());
    let genesis_port = "30333";
    let starting_gas_price = 987;
    let old_driver = Version36FuelCoreDriver::spawn(&[
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
        "--starting-gas-price",
        starting_gas_price.to_string().as_str(),
    ])
        .await
        .unwrap();
    let public_key = Keypair::from(genesis_keypair).public();
    let genesis_peer_id = PeerId::from_public_key(&public_key);
    let genesis_multiaddr = default_multiaddr(genesis_port, genesis_peer_id);
    let temp_dir = old_driver.kill().await;

    // Starting node that uses latest fuel core.
    // It will connect to the genesis node and sync blocks.
    let latest_keypair = SecpKeypair::generate();
    let hexed_secret = hex::encode(latest_keypair.secret().to_bytes());
    let latest_node = LatestFuelCoreDriver::spawn_with_directory(
        temp_dir,
        &[
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
        ],
    )
        .await
        .unwrap();

    // When
    const BLOCKS_TO_PRODUCE: u32 = 10;
    latest_node
        .client
        .produce_blocks(BLOCKS_TO_PRODUCE, None)
        .await
        .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Then
    let db = &latest_node.node.shared.database;
    let latest_height = db.gas_price().latest_height().unwrap();
    let view = db.gas_price().latest_view().unwrap();
    let metadata = view
        .storage::<GasPriceMetadata>()
        .get(&latest_height.into())
        .unwrap()
        .unwrap()
        .deref()
        .clone();

    assert!(matches!(metadata, UpdaterMetadata::V1(_)));
}
