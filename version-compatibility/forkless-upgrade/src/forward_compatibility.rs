//! Changes in the API break forward compatibility. In this case,
//! we need to remove old tests(usually, we need to create a new test per each release)
//! and write a new test(only one) to track new forward compatibility.

use crate::tests_helper::{
    default_multiaddr,
    transactions_from_subsections,
    upgrade_transaction,
    Version36FuelCoreDriver,
    IGNITION_TESTNET_SNAPSHOT,
    POA_SECRET_KEY,
    SUBSECTION_SIZE,
};
use fuel_tx::{
    field::ChargeableBody,
    UpgradePurpose,
    UploadSubsection,
};
use libp2p::{
    futures::StreamExt,
    identity::{
        secp256k1::Keypair as SecpKeypair,
        Keypair,
    },
    PeerId,
};
use rand::{
    rngs::StdRng,
    SeedableRng,
};
use std::time::Duration;

#[tokio::test]
async fn latest_state_transition_function_is_forward_compatible_with_v36_binary() {
    // The test has a v36 block producer and one v36 validator.
    // v36 nodes execute several blocks by using the v36 state transition function.
    // At some point, we upgrade the network to use the latest state transition function.
    // The network should be able to generate several new blocks with a new version.
    // v36 block producer and validator should process all blocks.
    //
    // These actions test that old nodes could use a new state transition function,
    // and it is forward compatible.
    //
    // To simplify the upgrade of the network `utxo_validation` is `false`.

    let v36_keypair = SecpKeypair::generate();
    let hexed_secret = hex::encode(v36_keypair.secret().to_bytes());
    let v36_port = "40333";
    let v36_node = Version36FuelCoreDriver::spawn(&[
        "--service-name",
        "V36Producer",
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
        v36_port,
    ])
    .await
    .unwrap();
    let public_key = Keypair::from(v36_keypair).public();
    let v36_peer_id = PeerId::from_public_key(&public_key);
    let v36_multiaddr = default_multiaddr(v36_port, v36_peer_id);

    // Starting a v36 validator node.
    // It will connect to the v36 node and sync blocks.
    let latest_keypair = SecpKeypair::generate();
    let hexed_secret = hex::encode(latest_keypair.secret().to_bytes());
    let validator_node = Version36FuelCoreDriver::spawn(&[
        "--service-name",
        "V36Validator",
        "--debug",
        "--poa-instant",
        "false",
        "--snapshot",
        IGNITION_TESTNET_SNAPSHOT,
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

    // Given
    let mut imported_blocks = validator_node.node.shared.block_importer.events();
    const BLOCKS_TO_PRODUCE: u32 = 10;
    v36_node
        .client
        .produce_blocks(BLOCKS_TO_PRODUCE, None)
        .await
        .unwrap();
    for i in 0..BLOCKS_TO_PRODUCE {
        let block =
            tokio::time::timeout(Duration::from_secs(120), imported_blocks.next())
                .await
                .expect(format!("Timed out waiting for block import {i}").as_str())
                .expect(format!("Failed to import block {i}").as_str());
        assert_eq!(
            block
                .sealed_block
                .entity
                .header()
                .state_transition_bytecode_version,
            0
        );
    }
    drop(imported_blocks);

    // When
    let subsections = UploadSubsection::split_bytecode(
        latest_fuel_core_upgradable_executor::WASM_BYTECODE,
        SUBSECTION_SIZE,
    )
    .unwrap();
    let mut rng = StdRng::seed_from_u64(12345);
    let amount = 100000;
    let transactions = transactions_from_subsections(&mut rng, subsections, amount);
    let root = transactions[0].body().root;
    for upload in transactions {
        let tx = upload.into();
        validator_node
            .client
            .submit_and_await_commit(&tx)
            .await
            .unwrap();
    }
    let upgrade =
        upgrade_transaction(UpgradePurpose::StateTransition { root }, &mut rng, amount);
    validator_node
        .client
        .submit_and_await_commit(&upgrade.into())
        .await
        .unwrap();

    // Then
    let mut imported_blocks = validator_node.node.shared.block_importer.events();
    v36_node
        .client
        .produce_blocks(BLOCKS_TO_PRODUCE, None)
        .await
        .unwrap();
    for i in 0..BLOCKS_TO_PRODUCE {
        let block =
            tokio::time::timeout(Duration::from_secs(120), imported_blocks.next())
                .await
                .expect(format!("Timed out waiting for block import {i}").as_str())
                .expect(format!("Failed to import block {i}").as_str());
        assert_eq!(
            block
                .sealed_block
                .entity
                .header()
                .state_transition_bytecode_version,
            1
        );
    }
}
