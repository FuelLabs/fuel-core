use crate::{
    bootstrap_node,
    tests_helper::{
        GenesisFuelCoreDriver,
        IGNITION_TESTNET_SNAPSHOT,
        IGNITION_V21_TESTNET_SNAPSHOT,
        LatestFuelCoreDriver,
        POA_SECRET_KEY,
        V44_TESTNET_SNAPSHOT,
        Version44FuelCoreDriver,
    },
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
use futures::StreamExt;
use libp2p_identity::secp256k1::Keypair as SecpKeypair;
use std::time::Duration;
use version_44_fuel_core_client::client::FuelClient as Version44Client;

const BLOCK_INCLUSION_TIMEOUT: Duration = Duration::from_secs(360);

fn shared_tx_params_match(
    old: &version_44_fuel_core_type::fuel_tx::TxParameters,
    new: &latest_fuel_core_type::fuel_tx::TxParameters,
) {
    assert_eq!(old.max_inputs(), new.max_inputs());
    assert_eq!(old.max_outputs(), new.max_outputs());
    assert_eq!(old.max_witnesses(), new.max_witnesses());
    assert_eq!(old.max_gas_per_tx(), new.max_gas_per_tx());
    assert_eq!(old.max_size(), new.max_size());
    assert_eq!(
        old.max_bytecode_subsections(),
        new.max_bytecode_subsections()
    );
}

fn shared_predicate_params_match(
    old: &version_44_fuel_core_type::fuel_tx::PredicateParameters,
    new: &latest_fuel_core_type::fuel_tx::PredicateParameters,
) {
    assert_eq!(old.max_predicate_length(), new.max_predicate_length());
    assert_eq!(
        old.max_predicate_data_length(),
        new.max_predicate_data_length()
    );
    assert_eq!(old.max_message_data_length(), new.max_message_data_length());
    assert_eq!(old.max_gas_per_predicate(), new.max_gas_per_predicate());
}

fn shared_script_params_match(
    old: &version_44_fuel_core_type::fuel_tx::ScriptParameters,
    new: &latest_fuel_core_type::fuel_tx::ScriptParameters,
) {
    assert_eq!(old.max_script_length(), new.max_script_length());
    assert_eq!(old.max_script_data_length(), new.max_script_data_length());
}

fn shared_contract_params_match(
    old: &version_44_fuel_core_type::fuel_tx::ContractParameters,
    new: &latest_fuel_core_type::fuel_tx::ContractParameters,
) {
    assert_eq!(old.contract_max_size(), new.contract_max_size());
    assert_eq!(old.max_storage_slots(), new.max_storage_slots());
}

fn shared_fee_params_match(
    old: &version_44_fuel_core_type::fuel_tx::FeeParameters,
    new: &latest_fuel_core_type::fuel_tx::FeeParameters,
) {
    assert_eq!(old.gas_price_factor(), new.gas_price_factor());
    assert_eq!(old.gas_per_byte(), new.gas_per_byte());
}

fn assert_shared_values_match(
    old: &version_44_fuel_core_type::fuel_tx::ConsensusParameters,
    new: &latest_fuel_core_type::fuel_tx::ConsensusParameters,
) {
    assert_eq!(old.base_asset_id().as_ref(), new.base_asset_id().as_ref());
    assert_eq!(old.block_gas_limit(), new.block_gas_limit());
    assert_eq!(old.chain_id().to_bytes(), new.chain_id().to_bytes());
    assert_eq!(
        old.privileged_address().as_ref(),
        new.privileged_address().as_ref()
    );

    shared_tx_params_match(old.tx_params(), new.tx_params());
    shared_predicate_params_match(old.predicate_params(), new.predicate_params());
    shared_script_params_match(old.script_params(), new.script_params());
    shared_contract_params_match(old.contract_params(), new.contract_params());
    shared_fee_params_match(old.fee_params(), new.fee_params());
}

#[tokio::test(flavor = "multi_thread")]
async fn latest_binary_serves_consensus_parameters_to_v44_client() {
    // given
    let latest_node = LatestFuelCoreDriver::spawn(&["--debug", "--poa-instant", "true"])
        .await
        .unwrap();
    let v44_client =
        Version44Client::from(latest_node.node.shared.graph_ql.bound_address);
    let new_consensus_parameters = latest_node
        .client
        .consensus_parameters(0)
        .await
        .unwrap()
        .unwrap();

    // when
    let old_consensus_parameters = match v44_client.consensus_parameters(0).await {
        Ok(Some(params)) => params,
        Ok(None) => panic!("v44 client returned no consensus parameters at version 0"),
        Err(error) => {
            panic!("v44 client failed to decode consensus parameters: {error:?}")
        }
    };

    // then
    assert!(matches!(
        new_consensus_parameters,
        latest_fuel_core_type::fuel_tx::ConsensusParameters::V2(_)
    ));

    assert_shared_values_match(&old_consensus_parameters, &new_consensus_parameters);

    assert_eq!(
        old_consensus_parameters.block_transaction_size_limit(),
        new_consensus_parameters.block_transaction_size_limit()
    );
    assert_ne!(
        new_consensus_parameters.block_transaction_size_limit(),
        u64::MAX
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn latest_binary_serves_ignition_snapshot_consensus_parameters_to_v44_client()
 {
    // given
    let latest_node = LatestFuelCoreDriver::spawn(&[
        "--debug",
        "--poa-instant",
        "true",
        "--snapshot",
        IGNITION_V21_TESTNET_SNAPSHOT,
    ])
    .await
    .unwrap();
    let v44_client =
        Version44Client::from(latest_node.node.shared.graph_ql.bound_address);
    let new_consensus_parameters = latest_node
        .client
        .consensus_parameters(0)
        .await
        .unwrap()
        .unwrap();

    // when
    let old_consensus_parameters = match v44_client.consensus_parameters(0).await {
        Ok(Some(params)) => params,
        Ok(None) => panic!("v44 client returned no consensus parameters at version 0"),
        Err(error) => {
            panic!("v44 client failed to decode consensus parameters: {error:?}")
        }
    };

    // then
    assert!(matches!(
        new_consensus_parameters,
        latest_fuel_core_type::fuel_tx::ConsensusParameters::V2(_)
    ));

    assert_shared_values_match(&old_consensus_parameters, &new_consensus_parameters);

    assert_eq!(
        old_consensus_parameters.block_transaction_size_limit(),
        new_consensus_parameters.block_transaction_size_limit()
    );
    assert_ne!(
        new_consensus_parameters.block_transaction_size_limit(),
        u64::MAX
    );
}

#[tokio::test(flavor = "multi_thread")]
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

#[tokio::test(flavor = "multi_thread")]
async fn latest_binary_is_backward_compatible_and_follows_blocks_created_by_genesis_binary()
 {
    let (_bootstrap_node, addr) =
        bootstrap_node(IGNITION_TESTNET_SNAPSHOT).await.unwrap();

    // Given
    let genesis_keypair = SecpKeypair::generate();
    let hexed_secret = hex::encode(genesis_keypair.secret().to_bytes());
    let _genesis_node = GenesisFuelCoreDriver::spawn(&[
        "--service-name",
        "GenesisProducer",
        "--debug",
        "--poa-interval-period",
        "1s",
        "--consensus-key",
        POA_SECRET_KEY,
        "--snapshot",
        IGNITION_TESTNET_SNAPSHOT,
        "--enable-p2p",
        "--keypair",
        hexed_secret.as_str(),
        "--reserved-nodes",
        addr.as_str(),
        "--peering-port",
        "0",
    ])
    .await
    .unwrap();

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
        addr.as_str(),
        "--peering-port",
        "0",
    ])
    .await
    .unwrap();
    let mut imported_blocks = latest_node.node.shared.block_importer.events();

    // When
    const BLOCKS_TO_PRODUCE: u32 = 10;
    // Then
    for i in 0..BLOCKS_TO_PRODUCE {
        let _ = tokio::time::timeout(BLOCK_INCLUSION_TIMEOUT, imported_blocks.next())
            .await
            .expect(format!("Timed out waiting for block import {i}").as_str())
            .expect(format!("Failed to import block {i}").as_str());
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn latest_binary_is_backward_compatible_and_follows_blocks_created_by_v44_binary() {
    let (_bootstrap_node, addr) = bootstrap_node(V44_TESTNET_SNAPSHOT).await.unwrap();

    // Given
    let v44_keypair = SecpKeypair::generate();
    let hexed_secret = hex::encode(v44_keypair.secret().to_bytes());
    let _v44_node = Version44FuelCoreDriver::spawn(&[
        "--service-name",
        "V44Producer",
        "--debug",
        "--poa-interval-period",
        "1s",
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
    ])
    .await
    .unwrap();

    // Starting node that uses latest fuel core.
    // It will connect to the v44 node and sync blocks.
    let latest_keypair = SecpKeypair::generate();
    let hexed_secret = hex::encode(latest_keypair.secret().to_bytes());
    let latest_node = LatestFuelCoreDriver::spawn(&[
        "--service-name",
        "LatestValidator",
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
    ])
    .await
    .unwrap();
    let mut imported_blocks = latest_node.node.shared.block_importer.events();

    // When
    const BLOCKS_TO_PRODUCE: u32 = 10;
    // Then
    for i in 0..BLOCKS_TO_PRODUCE {
        let _ = tokio::time::timeout(BLOCK_INCLUSION_TIMEOUT, imported_blocks.next())
            .await
            .expect(format!("Timed out waiting for block import {i}").as_str())
            .expect(format!("Failed to import block {i}").as_str());
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn latest_binary_is_backward_compatible_and_can_deserialize_errors_from_genesis_binary()
 {
    // Given
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
