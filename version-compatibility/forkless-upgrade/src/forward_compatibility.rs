//! Changes in the API break forward compatibility. In this case,
//! we need to remove old tests(usually, we need to create a new test per each release)
//! and write a new test(only one) to track new forward compatibility.

use crate::tests_helper::{
    default_multiaddr,
    transactions_from_subsections,
    upgrade_transaction,
    GenesisFuelCoreDriver,
    IGNITION_SNAPSHOT,
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
async fn latest_state_transition_function_is_forward_compatible_with_genesis_binary() {
    // The test has a genesis block producer and one genesis validator.
    // Genesis nodes execute several blocks by using the genesis state transition function.
    // At some point, we upgrade the network to use the latest state transition function.
    // The network should be able to generate several new blocks with a new version.
    // Genesis block producer and validator should process all blocks.
    //
    // These actions test that old nodes could use a new state transition function,
    // and it is forward compatible.
    //
    // To simplify the upgrade of the network `utxo_validation` is `false`.

    let genesis_keypair = SecpKeypair::generate();
    let hexed_secret = hex::encode(genesis_keypair.secret().to_bytes());
    let genesis_port = "40333";
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

    // Starting a genesis validator node.
    // It will connect to the genesis node and sync blocks.
    let latest_keypair = SecpKeypair::generate();
    let hexed_secret = hex::encode(latest_keypair.secret().to_bytes());
    let validator_node = GenesisFuelCoreDriver::spawn(&[
        "--service-name",
        "GenesisValidator",
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

    // Given
    let mut imported_blocks = validator_node.node.shared.block_importer.events();
    const BLOCKS_TO_PRODUCE: u32 = 10;
    genesis_node
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
    genesis_node
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
