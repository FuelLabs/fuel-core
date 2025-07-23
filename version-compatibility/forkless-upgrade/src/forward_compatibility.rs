//! Changes in the API break forward compatibility. In this case,
//! we need to remove old tests(usually, we need to create a new test per each release)
//! and write a new test(only one) to track new forward compatibility.

use crate::{
    bootstrap_node,
    tests_helper::{
        POA_SECRET_KEY,
        SUBSECTION_SIZE,
        V44_TESTNET_SNAPSHOT,
        Version44FuelCoreDriver,
        transactions_from_subsections,
        upgrade_transaction,
    },
};
use latest_fuel_core_type::fuel_tx::field::ChargeableBody;
use libp2p::{
    futures::StreamExt,
    identity::secp256k1::Keypair as SecpKeypair,
};
use rand::{
    SeedableRng,
    rngs::StdRng,
};
use std::time::Duration;

#[tokio::test(flavor = "multi_thread")]
async fn latest_state_transition_function_is_forward_compatible_with_v44_binary() {
    let (_bootstrap_node, addr) = bootstrap_node(V44_TESTNET_SNAPSHOT).await.unwrap();

    // The test has a v44 block producer and one v44 validator.
    // v44 nodes execute several blocks by using the v44 state transition function.
    // At some point, we upgrade the network to use the latest state transition function.
    // The network should be able to generate several new blocks with a new version.
    // v44 block producer and validator should process all blocks.
    //
    // These actions test that old nodes could use a new state transition function,
    // and it is forward compatible.
    //
    // To simplify the upgrade of the network `utxo_validation` is `false`.

    let v44_keypair = SecpKeypair::generate();
    let hexed_secret = hex::encode(v44_keypair.secret().to_bytes());
    let _v44_node = Version44FuelCoreDriver::spawn(&[
        "--service-name",
        "V44Producer",
        "--debug",
        "--poa-interval-period",
        "50ms",
        "--consensus-key",
        POA_SECRET_KEY,
        "--snapshot",
        V44_TESTNET_SNAPSHOT,
        "--enable-p2p",
        "--keypair",
        hexed_secret.as_str(),
        "--reserved-nodes",
        addr.as_str(),
        "--peering-port",
        "0",
        "--heartbeat-idle-duration=0ms",
    ])
    .await
    .unwrap();

    // Starting a v44 validator node.
    // It will connect to the v44 node and sync blocks.
    let latest_keypair = SecpKeypair::generate();
    let hexed_secret = hex::encode(latest_keypair.secret().to_bytes());
    let validator_node = Version44FuelCoreDriver::spawn(&[
        "--service-name",
        "V44Validator",
        "--debug",
        "--poa-instant",
        "false",
        "--snapshot",
        V44_TESTNET_SNAPSHOT,
        "--enable-p2p",
        "--keypair",
        hexed_secret.as_str(),
        "--reserved-nodes",
        addr.as_str(),
        "--peering-port",
        "0",
        "--heartbeat-idle-duration=0ms",
    ])
    .await
    .unwrap();

    // Given
    let mut imported_blocks = validator_node.node.shared.block_importer.events();
    const BLOCKS_TO_PRODUCE: u32 = 10;
    for i in 0..BLOCKS_TO_PRODUCE {
        tracing::warn!("beep");
        let block = tokio::time::timeout(Duration::from_secs(10), imported_blocks.next())
            .await
            .expect(format!("Timed out waiting for block import {i}").as_str())
            .expect(format!("Failed to import block {i}").as_str());
        tracing::warn!("boop");
        assert_eq!(
            block
                .sealed_block
                .entity
                .header()
                .state_transition_bytecode_version(),
            29
        );
    }
    drop(imported_blocks);

    // When
    let subsections = latest_fuel_core_type::fuel_tx::UploadSubsection::split_bytecode(
        latest_fuel_core_upgradable_executor::WASM_BYTECODE,
        SUBSECTION_SIZE,
    )
    .unwrap();
    let mut rng = StdRng::seed_from_u64(12345);
    let amount = 100000;
    let transactions = transactions_from_subsections(&mut rng, subsections, amount);
    let root = transactions[0].body().root;
    for upload in transactions {
        let tx = latest_fuel_core_type::fuel_tx::Transaction::Upload(upload);
        validator_node
            .client
            .submit_and_await_commit(&tx)
            .await
            .unwrap();
    }
    let upgrade = upgrade_transaction(
        latest_fuel_core_type::fuel_tx::UpgradePurpose::StateTransition { root },
        &mut rng,
        amount,
    );
    let upgrade_tx = latest_fuel_core_type::fuel_tx::Transaction::Upgrade(upgrade);
    validator_node
        .client
        .submit_and_await_commit(&upgrade_tx)
        .await
        .unwrap();

    // Then
    let mut imported_blocks = validator_node.node.shared.block_importer.events();
    for i in 0..BLOCKS_TO_PRODUCE {
        // Big timeout because we need to compile the state transition function.
        let block =
            tokio::time::timeout(Duration::from_secs(360), imported_blocks.next())
                .await
                .expect(format!("Timed out waiting for block import {i}").as_str())
                .expect(format!("Failed to import block {i}").as_str());
        assert_eq!(
            block
                .sealed_block
                .entity
                .header()
                .state_transition_bytecode_version(),
            30
        );
    }
}
