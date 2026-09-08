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
use libp2p::identity::secp256k1::Keypair as SecpKeypair;
use rand::{
    SeedableRng,
    rngs::StdRng,
};
use std::time::Duration;
use version_44_fuel_core_client::client::{
    FuelClient,
    types::{
        Block,
        TransactionStatus,
    },
};
use version_44_fuel_core_type::fuel_tx::field::ChargeableBody;

async fn await_v44_block(client: &FuelClient, height: u32, timeout: Duration) -> Block {
    tokio::time::timeout(timeout, async {
        loop {
            let block =
                client
                    .block_by_height(height.into())
                    .await
                    .unwrap_or_else(|error| {
                        panic!("Failed to query v44 validator block {height}: {error}")
                    });
            if let Some(block) = block {
                assert_eq!(block.header.height, height);
                return block;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!("Timed out after {timeout:?} waiting for v44 validator block {height}")
    })
}

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
    const BLOCKS_TO_PRODUCE: u32 = 10;
    const BLOCK_IMPORT_TIMEOUT: Duration = Duration::from_secs(10);
    // Capture the starting height once, then inspect every subsequent block.
    let initial_height =
        tokio::time::timeout(BLOCK_IMPORT_TIMEOUT, validator_node.client.chain_info())
            .await
            .expect("Timed out querying the v44 validator's initial height")
            .expect("Failed to query the v44 validator's initial height")
            .latest_block
            .header
            .height;
    for offset in 1..=BLOCKS_TO_PRODUCE {
        let height = initial_height.checked_add(offset).expect("Height overflow");
        let block =
            await_v44_block(&validator_node.client, height, BLOCK_IMPORT_TIMEOUT).await;
        assert_eq!(
            block.header.state_transition_bytecode_version, 29,
            "Unexpected STF version before upgrade at height {height}"
        );
    }

    // When
    let subsections =
        version_44_fuel_core_type::fuel_tx::UploadSubsection::split_bytecode(
            latest_fuel_core_upgradable_executor::WASM_BYTECODE,
            SUBSECTION_SIZE,
        )
        .unwrap();
    let mut rng = StdRng::seed_from_u64(12345);
    let amount = 100000;
    let transactions = transactions_from_subsections(&mut rng, subsections, amount);
    let root = transactions[0].body().root;
    for upload in transactions {
        let tx = version_44_fuel_core_type::fuel_tx::Transaction::Upload(upload);
        let status = tokio::time::timeout(
            BLOCK_IMPORT_TIMEOUT,
            validator_node.client.submit_and_await_commit(&tx),
        )
        .await
        .expect("Timed out committing a bytecode upload")
        .expect("Failed to submit a bytecode upload");
        assert!(
            matches!(status, TransactionStatus::Success { .. }),
            "Bytecode upload did not succeed: {status:?}"
        );
    }
    let upgrade = upgrade_transaction(
        version_44_fuel_core_type::fuel_tx::UpgradePurpose::StateTransition { root },
        &mut rng,
        amount,
    );
    let upgrade_tx = version_44_fuel_core_type::fuel_tx::Transaction::Upgrade(upgrade);
    let upgrade_status = tokio::time::timeout(
        BLOCK_IMPORT_TIMEOUT,
        validator_node.client.submit_and_await_commit(&upgrade_tx),
    )
    .await
    .expect("Timed out committing the STF upgrade")
    .expect("Failed to submit the STF upgrade");
    let upgrade_height = match upgrade_status {
        TransactionStatus::Success { block_height, .. } => u32::from(block_height),
        status => panic!("STF upgrade did not succeed: {status:?}"),
    };

    // Then
    // The upgrade executes under version 29 and stores version 30 for the next
    // block's header. Anchor to its committed height, not a moving chain tip, so
    // delayed polling cannot skip an incorrect first block after activation.
    let upgrade_block =
        await_v44_block(&validator_node.client, upgrade_height, BLOCK_IMPORT_TIMEOUT)
            .await;
    assert_eq!(
        upgrade_block.header.state_transition_bytecode_version, 29,
        "Unexpected STF version in the upgrade block at height {upgrade_height}"
    );
    for offset in 1..=BLOCKS_TO_PRODUCE {
        let height = upgrade_height.checked_add(offset).expect("Height overflow");
        // Big timeout because we need to compile the state transition function.
        let block =
            await_v44_block(&validator_node.client, height, Duration::from_secs(360))
                .await;
        assert_eq!(
            block.header.state_transition_bytecode_version, 30,
            "Unexpected STF version after activation at height {height}"
        );
    }
    drop(validator_node.kill().await);
    drop(_v44_node.kill().await);
}
