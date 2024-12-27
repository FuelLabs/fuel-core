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
        fuel_core_storage_adapter::storage::GasPriceMetadata as NewGasPriceMetadata,
        updater_metadata::UpdaterMetadata as NewUpdaterMetadata,
    },
    ports::GasPriceData as NewGasPriceData,
    v1::metadata::V1Metadata,
};
use latest_fuel_core_storage::{
    transactional::AtomicView as NewAtomicView,
    StorageAsRef as NewStorageAsRef,
};
use libp2p::{
    identity::{
        secp256k1::Keypair as SecpKeypair,
        Keypair,
    },
    PeerId,
};
use std::ops::Deref;
use version_36_fuel_core_gas_price_service::fuel_gas_price_updater::{
    fuel_core_storage_adapter::storage::GasPriceMetadata as OldGasPriceMetadata,
    UpdaterMetadata as OldUpdaterMetadata,
    V0Metadata,
};
use version_36_fuel_core_storage::{
    transactional::{
        AtomicView as OldAtomicView,
        HistoricalView as OldHistoricalView,
    },
    StorageAsRef as OldStorageAsRef,
};

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

    let db = &old_driver.node.shared.database;
    let latest_height = db.gas_price().latest_height().unwrap();
    let view = db.gas_price().latest_view().unwrap();
    let v0_metadata = match OldStorageAsRef::storage::<OldGasPriceMetadata>(&view)
        .get(&latest_height)
        .unwrap()
        .unwrap()
        .deref()
        .clone()
    {
        OldUpdaterMetadata::V0(v0) => v0,
    };

    let public_key = Keypair::from(genesis_keypair).public();
    let genesis_peer_id = PeerId::from_public_key(&public_key);
    let genesis_multiaddr = default_multiaddr(genesis_port, genesis_peer_id);
    drop(view);
    let temp_dir = old_driver.kill().await;

    // Starting node that uses latest fuel core.
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
    const BLOCKS_TO_PRODUCE: u32 = 1;
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
    let v1_metadata = V1Metadata::try_from(
        NewStorageAsRef::storage::<NewGasPriceMetadata>(&view)
            .get(&latest_height.into())
            .unwrap()
            .unwrap()
            .deref()
            .clone(),
    )
    .unwrap();

    assert_eq!(
        v0_metadata.l2_block_height + BLOCKS_TO_PRODUCE,
        v1_metadata.l2_block_height
    );
    // Assert that v1 behaves differently from v0.
    assert_ne!(
        v0_metadata.new_exec_price,
        v1_metadata.new_scaled_exec_price * v1_metadata.gas_price_factor.get()
    );
}
