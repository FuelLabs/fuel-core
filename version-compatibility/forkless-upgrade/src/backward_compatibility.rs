use crate::{
    bootstrap_node,
    tests_helper::{
        GenesisFuelCoreDriver,
        IGNITION_TESTNET_SNAPSHOT,
        LatestFuelCoreDriver,
        POA_SECRET_KEY,
        V44_TESTNET_SNAPSHOT,
        Version44FuelCoreDriver,
    },
};
use cynic::QueryBuilder;
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
    identity::secp256k1::Keypair as SecpKeypair,
};
use serde_json::{
    Value,
    json,
};
use std::time::Duration;

const BLOCK_INCLUSION_TIMEOUT: Duration = Duration::from_secs(360);

const V44_CONSENSUS_PARAMETERS_QUERY: &str = r#"
    query ConsensusParametersByVersionQueryLegacy($version: Int!) {
      consensusParameters(version: $version) {
        version
        txParams {
          version
          maxInputs
          maxOutputs
          maxWitnesses
          maxGasPerTx
          maxSize
          maxBytecodeSubsections
        }
        predicateParams {
          version
          maxPredicateLength
          maxPredicateDataLength
          maxMessageDataLength
          maxGasPerPredicate
        }
        scriptParams {
          version
          maxScriptLength
          maxScriptDataLength
        }
        contractParams {
          version
          contractMaxSize
          maxStorageSlots
        }
        feeParams {
          version
          gasPriceFactor
          gasPerByte
        }
        baseAssetId
        blockGasLimit
        blockTransactionSizeLimit
        chainId
        gasCosts {
          version
          add
          addi
          and
          andi
          bal
          bhei
          bhsh
          burn
          cb
          cfsi
          div
          divi
          eck1
          ecr1
          ed19
          eq
          exp
          expi
          flag
          gm
          gt
          gtf
          ji
          jmp
          jne
          jnei
          jnzi
          jmpf
          jmpb
          jnzf
          jnzb
          jnef
          jneb
          lb
          log
          lt
          lw
          mint
          mlog
          modOp
          modi
          moveOp
          movi
          mroo
          mul
          muli
          mldv
          niop
          noop
          not
          or
          ori
          poph
          popl
          pshh
          pshl
          ret
          rvrt
          sb
          sll
          slli
          srl
          srli
          srw
          sub
          subi
          sw
          sww
          time
          tr
          tro
          wdcm
          wqcm
          wdop
          wqop
          wdml
          wqml
          wddv
          wqdv
          wdmd
          wqmd
          wdam
          wqam
          wdmm
          wqmm
          xor
          xori
          ecop
          alocDependentCost {
            __typename
            ... on LightOperation {
              base
              unitsPerGas
            }
            ... on HeavyOperation {
              base
              gasPerUnit
            }
          }
          bsiz {
            __typename
            ... on LightOperation {
              base
              unitsPerGas
            }
            ... on HeavyOperation {
              base
              gasPerUnit
            }
          }
          bldd {
            __typename
            ... on LightOperation {
              base
              unitsPerGas
            }
            ... on HeavyOperation {
              base
              gasPerUnit
            }
          }
          cfe {
            __typename
            ... on LightOperation {
              base
              unitsPerGas
            }
            ... on HeavyOperation {
              base
              gasPerUnit
            }
          }
          cfeiDependentCost {
            __typename
            ... on LightOperation {
              base
              unitsPerGas
            }
            ... on HeavyOperation {
              base
              gasPerUnit
            }
          }
          call {
            __typename
            ... on LightOperation {
              base
              unitsPerGas
            }
            ... on HeavyOperation {
              base
              gasPerUnit
            }
          }
          ccp {
            __typename
            ... on LightOperation {
              base
              unitsPerGas
            }
            ... on HeavyOperation {
              base
              gasPerUnit
            }
          }
          croo {
            __typename
            ... on LightOperation {
              base
              unitsPerGas
            }
            ... on HeavyOperation {
              base
              gasPerUnit
            }
          }
          csiz {
            __typename
            ... on LightOperation {
              base
              unitsPerGas
            }
            ... on HeavyOperation {
              base
              gasPerUnit
            }
          }
          ed19DependentCost {
            __typename
            ... on LightOperation {
              base
              unitsPerGas
            }
            ... on HeavyOperation {
              base
              gasPerUnit
            }
          }
          k256 {
            __typename
            ... on LightOperation {
              base
              unitsPerGas
            }
            ... on HeavyOperation {
              base
              gasPerUnit
            }
          }
          ldc {
            __typename
            ... on LightOperation {
              base
              unitsPerGas
            }
            ... on HeavyOperation {
              base
              gasPerUnit
            }
          }
          logd {
            __typename
            ... on LightOperation {
              base
              unitsPerGas
            }
            ... on HeavyOperation {
              base
              gasPerUnit
            }
          }
          mcl {
            __typename
            ... on LightOperation {
              base
              unitsPerGas
            }
            ... on HeavyOperation {
              base
              gasPerUnit
            }
          }
          mcli {
            __typename
            ... on LightOperation {
              base
              unitsPerGas
            }
            ... on HeavyOperation {
              base
              gasPerUnit
            }
          }
          mcp {
            __typename
            ... on LightOperation {
              base
              unitsPerGas
            }
            ... on HeavyOperation {
              base
              gasPerUnit
            }
          }
          mcpi {
            __typename
            ... on LightOperation {
              base
              unitsPerGas
            }
            ... on HeavyOperation {
              base
              gasPerUnit
            }
          }
          meq {
            __typename
            ... on LightOperation {
              base
              unitsPerGas
            }
            ... on HeavyOperation {
              base
              gasPerUnit
            }
          }
          retd {
            __typename
            ... on LightOperation {
              base
              unitsPerGas
            }
            ... on HeavyOperation {
              base
              gasPerUnit
            }
          }
          s256 {
            __typename
            ... on LightOperation {
              base
              unitsPerGas
            }
            ... on HeavyOperation {
              base
              gasPerUnit
            }
          }
          scwq {
            __typename
            ... on LightOperation {
              base
              unitsPerGas
            }
            ... on HeavyOperation {
              base
              gasPerUnit
            }
          }
          smo {
            __typename
            ... on LightOperation {
              base
              unitsPerGas
            }
            ... on HeavyOperation {
              base
              gasPerUnit
            }
          }
          srwq {
            __typename
            ... on LightOperation {
              base
              unitsPerGas
            }
            ... on HeavyOperation {
              base
              gasPerUnit
            }
          }
          swwq {
            __typename
            ... on LightOperation {
              base
              unitsPerGas
            }
            ... on HeavyOperation {
              base
              gasPerUnit
            }
          }
          epar {
            __typename
            ... on LightOperation {
              base
              unitsPerGas
            }
            ... on HeavyOperation {
              base
              gasPerUnit
            }
          }
          contractRoot {
            __typename
            ... on LightOperation {
              base
              unitsPerGas
            }
            ... on HeavyOperation {
              base
              gasPerUnit
            }
          }
          stateRoot {
            __typename
            ... on LightOperation {
              base
              unitsPerGas
            }
            ... on HeavyOperation {
              base
              gasPerUnit
            }
          }
          vmInitialization {
            __typename
            ... on LightOperation {
              base
              unitsPerGas
            }
            ... on HeavyOperation {
              base
              gasPerUnit
            }
          }
          newStoragePerByte
        }
        privilegedAddress
      }
    }
"#;

async fn fetch_raw_v44_consensus_parameters_response(
    address: impl core::fmt::Display,
) -> Value {
    let response = reqwest::Client::new()
        .post(format!("http://{address}/v1/graphql"))
        .json(&json!({
            "query": V44_CONSENSUS_PARAMETERS_QUERY,
            "variables": { "version": 0 }
        }))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    serde_json::from_str(&response).unwrap()
}

async fn fetch_raw_latest_consensus_parameters_response(
    address: impl core::fmt::Display,
) -> Value {
    let operation =
        latest_fuel_core_client::client::schema::upgrades::ConsensusParametersByVersionQuery::build(
            latest_fuel_core_client::client::schema::upgrades::ConsensusParametersByVersionArgs {
                version: 0,
            },
        );
    let response = reqwest::Client::new()
        .post(format!("http://{address}/v1/graphql"))
        .json(&json!({
            "query": operation.query,
            "variables": operation.variables,
        }))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    serde_json::from_str(&response).unwrap()
}

fn assert_consensus_parameters_graphql_compatibility(
    v44_graphql: &Value,
    latest_graphql: &Value,
    new_consensus_parameters: &latest_fuel_core_type::fuel_tx::ConsensusParameters,
) {
    assert!(
        v44_graphql.get("errors").is_none(),
        "unexpected v44 GraphQL errors: {v44_graphql}"
    );
    assert!(
        latest_graphql.get("errors").is_none(),
        "unexpected latest GraphQL errors: {latest_graphql}"
    );

    let v44_graphql_consensus = &v44_graphql["data"]["consensusParameters"];
    let latest_graphql_consensus = &latest_graphql["data"]["consensusParameters"];

    assert_eq!(v44_graphql_consensus["version"], "V1");
    assert_eq!(v44_graphql_consensus["scriptParams"]["version"], "V1");
    assert!(
        v44_graphql_consensus["scriptParams"]["maxStorageSlotLength"].is_null()
    );
    assert!(v44_graphql_consensus["gasCosts"]["storageReadCold"].is_null());

    assert_eq!(latest_graphql_consensus["version"], "V1");
    assert_eq!(latest_graphql_consensus["scriptParams"]["version"], "V2");
    assert_eq!(
        latest_graphql_consensus["blockGasLimit"],
        new_consensus_parameters.block_gas_limit().to_string()
    );
    assert_eq!(
        latest_graphql_consensus["blockTransactionSizeLimit"],
        new_consensus_parameters
            .block_transaction_size_limit()
            .to_string()
    );
    assert!(
        latest_graphql_consensus["scriptParams"]["maxStorageSlotLength"].is_string()
    );
    assert!(
        latest_graphql_consensus["gasCosts"]["storageReadCold"]["__typename"].is_string()
    );
    assert!(
        latest_graphql_consensus["gasCosts"]["storageReadHot"]["__typename"].is_string()
    );
    assert!(
        latest_graphql_consensus["gasCosts"]["storageWrite"]["__typename"].is_string()
    );
    assert!(
        latest_graphql_consensus["gasCosts"]["storageClear"]["__typename"].is_string()
    );
}

fn decode_v44_consensus_parameters(
    response: &Value,
) -> version_44_fuel_core_type::fuel_tx::ConsensusParameters {
    let query: version_44_fuel_core_client::client::schema::upgrades::ConsensusParametersByVersionQuery =
        serde_json::from_value(response["data"].clone()).unwrap();
    query.consensus_parameters.unwrap().try_into().unwrap()
}

fn decode_latest_consensus_parameters(
    response: &Value,
) -> latest_fuel_core_type::fuel_tx::ConsensusParameters {
    let query: latest_fuel_core_client::client::schema::upgrades::ConsensusParametersByVersionQuery =
        serde_json::from_value(response["data"].clone()).unwrap();
    query.consensus_parameters.unwrap().try_into().unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn latest_binary_serves_consensus_parameters_to_v44_client() {
    let latest_node = LatestFuelCoreDriver::spawn(&["--debug", "--poa-instant", "true"])
        .await
        .unwrap();
    let v44_graphql = fetch_raw_v44_consensus_parameters_response(
        latest_node.node.shared.graph_ql.bound_address,
    )
    .await;
    let latest_graphql = fetch_raw_latest_consensus_parameters_response(
        latest_node.node.shared.graph_ql.bound_address,
    )
    .await;

    let old_consensus_parameters = decode_v44_consensus_parameters(&v44_graphql);
    let new_consensus_parameters = decode_latest_consensus_parameters(&latest_graphql);

    assert_consensus_parameters_graphql_compatibility(
        &v44_graphql,
        &latest_graphql,
        &new_consensus_parameters,
    );

    assert!(matches!(
        new_consensus_parameters,
        latest_fuel_core_type::fuel_tx::ConsensusParameters::V2(_)
    ));

    assert_eq!(
        old_consensus_parameters.block_gas_limit(),
        new_consensus_parameters.block_gas_limit()
    );
    assert_eq!(
        old_consensus_parameters.tx_params().max_gas_per_tx(),
        new_consensus_parameters.tx_params().max_gas_per_tx()
    );
    assert_eq!(
        old_consensus_parameters.script_params().max_script_length(),
        new_consensus_parameters.script_params().max_script_length()
    );
    assert_eq!(
        old_consensus_parameters
            .script_params()
            .max_script_data_length(),
        new_consensus_parameters
            .script_params()
            .max_script_data_length()
    );
    assert_eq!(
        old_consensus_parameters.block_transaction_size_limit(),
        u64::MAX
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
