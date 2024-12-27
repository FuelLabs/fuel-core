#![allow(non_snake_case)]
// TODO: REMOVE BEFORE MERGING
#![allow(dead_code)]
#![allow(unused_imports)]

use crate::helpers::{
    TestContext,
    TestSetupBuilder,
};
use ethers::types::Opcode;
use fuel_core::{
    chain_config::{
        ChainConfig,
        StateConfig,
    },
    combined_database::CombinedDatabase,
    database::Database,
    fuel_core_graphql_api::ports::worker::OnChainDatabase,
    service::{
        Config,
        FuelService,
    },
    state::historical_rocksdb::StateRewindPolicy,
};
use fuel_core_client::client::{
    types::gas_price::LatestGasPrice,
    FuelClient,
};
use fuel_core_gas_price_service::{
    common::fuel_core_storage_adapter::storage::{
        GasPriceMetadata,
        RecordedHeights,
    },
    ports::{
        GasPriceData,
        GetLatestRecordedHeight,
        GetMetadataStorage,
    },
    v1::{
        da_source_service::block_committer_costs::fake_server::FakeServer,
        metadata::V1Metadata,
    },
};
use fuel_core_poa::Trigger;
use fuel_core_storage::{
    transactional::AtomicView,
    StorageAsRef,
};
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_asm::*,
    fuel_crypto::{
        coins_bip32::ecdsa::signature::rand_core::SeedableRng,
        SecretKey,
    },
    fuel_tx::{
        consensus_parameters::ConsensusParametersV1,
        AssetId,
        ConsensusParameters,
        Finalizable,
        Transaction,
        TransactionBuilder,
    },
    services::executor::TransactionExecutionResult,
};
use rand::Rng;
use std::{
    collections::HashMap,
    iter::repeat,
    num::NonZero,
    ops::Deref,
    sync::{
        Arc,
        Mutex,
    },
    time::Duration,
};
use test_helpers::fuel_core_driver::FuelCoreDriver;
use wideint::MathOp;

fn tx_for_gas_limit(max_fee_limit: Word) -> Transaction {
    TransactionBuilder::script(vec![], vec![])
        .max_fee_limit(max_fee_limit)
        .add_fee_input()
        .finalize()
        .into()
}

fn infinite_loop_tx<R: Rng + rand::CryptoRng>(
    max_fee_limit: Word,
    rng: &mut R,
    asset_id: Option<AssetId>,
) -> Transaction {
    let script = vec![op::jmp(RegId::ZERO)];
    let script_bytes = script.iter().flat_map(|op| op.to_bytes()).collect();
    let mut builder = TransactionBuilder::script(script_bytes, vec![]);
    let asset_id = asset_id.unwrap_or_else(|| *builder.get_params().base_asset_id());
    builder
        .max_fee_limit(max_fee_limit)
        .script_gas_limit(800_000)
        .add_unsigned_coin_input(
            SecretKey::random(rng),
            rng.gen(),
            u32::MAX as u64,
            asset_id,
            Default::default(),
        )
        .finalize()
        .into()
}

fn arb_large_tx<R: Rng + rand::CryptoRng>(
    max_fee_limit: Word,
    rng: &mut R,
    asset_id: Option<AssetId>,
) -> Transaction {
    let mut script: Vec<_> = repeat(op::noop()).take(10_000).collect();
    script.push(op::ret(RegId::ONE));
    let script_bytes = script.iter().flat_map(|op| op.to_bytes()).collect();
    let mut builder = TransactionBuilder::script(script_bytes, vec![]);
    let asset_id = asset_id.unwrap_or_else(|| *builder.get_params().base_asset_id());
    builder
        .max_fee_limit(max_fee_limit)
        .script_gas_limit(600_000)
        .add_unsigned_coin_input(
            SecretKey::random(rng),
            rng.gen(),
            u32::MAX as u64,
            asset_id,
            Default::default(),
        )
        .finalize()
        .into()
}

fn arb_small_tx<R: Rng + rand::CryptoRng>(
    max_fee_limit: Word,
    rng: &mut R,
) -> Transaction {
    let mut script: Vec<_> = repeat(op::noop()).take(10).collect();
    script.push(op::ret(RegId::ONE));
    let script_bytes = script.iter().flat_map(|op| op.to_bytes()).collect();
    let mut builder = TransactionBuilder::script(script_bytes, vec![]);
    let asset_id = *builder.get_params().base_asset_id();
    builder
        .max_fee_limit(max_fee_limit)
        .script_gas_limit(22430)
        .add_unsigned_coin_input(
            SecretKey::random(rng),
            rng.gen(),
            u32::MAX as u64,
            asset_id,
            Default::default(),
        )
        .finalize()
        .into()
}

#[tokio::test]
async fn latest_gas_price__if_no_mint_tx_in_previous_block_gas_price_is_zero() {
    // given
    let node_config = Config::local_node();
    let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // when
    let LatestGasPrice { gas_price, .. } = client.latest_gas_price().await.unwrap();

    // then
    let expected = 0;
    let actual = gas_price;
    assert_eq!(expected, actual)
}

#[tokio::test]
async fn latest_gas_price__for_single_block_should_be_starting_gas_price() {
    // given
    let mut config = Config::local_node();
    let starting_gas_price = 982;
    config.starting_exec_gas_price = starting_gas_price;
    let srv = FuelService::from_database(Database::default(), config.clone())
        .await
        .unwrap();
    let client = FuelClient::from(srv.bound_address);

    // when
    let tx = tx_for_gas_limit(1);
    let _ = client.submit_and_await_commit(&tx).await.unwrap();
    let LatestGasPrice { gas_price, .. } = client.latest_gas_price().await.unwrap();

    // then
    let expected = starting_gas_price;
    let actual = gas_price;
    assert_eq!(expected, actual)
}

#[tokio::test]
async fn produce_block__raises_gas_price() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();
    // given
    let block_gas_limit = 3_000_000;
    let chain_config = ChainConfig {
        consensus_parameters: ConsensusParameters::V1(ConsensusParametersV1 {
            block_gas_limit,
            ..Default::default()
        }),
        ..ChainConfig::local_testnet()
    };
    let mut node_config =
        Config::local_node_with_configs(chain_config, StateConfig::local_testnet());
    let starting_gas_price = 1_000_000_000;
    let percent = 10;
    let threshold = 50;
    node_config.block_producer.coinbase_recipient = Some([5; 32].into());
    node_config.starting_exec_gas_price = starting_gas_price;
    node_config.exec_gas_price_change_percent = percent;
    node_config.exec_gas_price_threshold_percent = threshold;
    node_config.block_production = Trigger::Never;
    node_config.da_p_component = 0;
    node_config.da_d_component = 0;
    node_config.max_da_gas_price_change_percent = 0;
    node_config.min_da_gas_price = 0;

    let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322u64);

    // when
    let arb_tx_count = 10;
    for i in 0..arb_tx_count {
        let tx = arb_large_tx(18902800 + i as Word, &mut rng, None);
        let _status = client.submit(&tx).await.unwrap();
    }
    // starting gas price
    let _ = client.produce_blocks(1, None).await.unwrap();
    // updated gas price
    let _ = client.produce_blocks(1, None).await.unwrap();

    // then
    let change = starting_gas_price * percent as u64 / 100;
    let expected = starting_gas_price + change;
    let latest = client.latest_gas_price().await.unwrap();
    let actual = latest.gas_price;
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn produce_block__lowers_gas_price() {
    // given
    let block_gas_limit = 3_000_000;
    let chain_config = ChainConfig {
        consensus_parameters: ConsensusParameters::V1(ConsensusParametersV1 {
            block_gas_limit,
            ..Default::default()
        }),
        ..ChainConfig::local_testnet()
    };
    let mut node_config =
        Config::local_node_with_configs(chain_config, StateConfig::local_testnet());
    let starting_gas_price = 1_000_000_000;
    let percent = 10;
    let threshold = 50;
    node_config.block_producer.coinbase_recipient = Some([5; 32].into());
    node_config.starting_exec_gas_price = starting_gas_price;
    node_config.exec_gas_price_change_percent = percent;
    node_config.exec_gas_price_threshold_percent = threshold;
    node_config.block_production = Trigger::Never;
    node_config.da_p_component = 0;
    node_config.da_d_component = 0;
    node_config.max_da_gas_price_change_percent = 0;
    node_config.min_da_gas_price = 0;

    let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322u64);

    // when
    let arb_tx_count = 5;
    for i in 0..arb_tx_count {
        let tx = arb_large_tx(18902800 + i as Word, &mut rng, None);
        let _status = client.submit(&tx).await.unwrap();
    }
    // starting gas price
    let _ = client.produce_blocks(1, None).await.unwrap();
    // updated gas price
    let _ = client.produce_blocks(1, None).await.unwrap();

    // then
    let change = starting_gas_price * percent as u64 / 100;
    let expected = starting_gas_price - change;
    let latest = client.latest_gas_price().await.unwrap();
    let actual = latest.gas_price;
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn produce_block__raises_gas_price_with_default_parameters() {
    // given
    let args = vec![
        "--debug",
        "--poa-instant",
        "false",
        "--coinbase-recipient",
        "0x1111111111111111111111111111111111111111111111111111111111111111",
        "--min-da-gas-price",
        "0",
        "--da-p-component",
        "0",
        "--da-d-component",
        "0",
        "--starting-gas-price",
        "1000",
        "--gas-price-change-percent",
        "10",
        "--max-da-gas-price-change-percent",
        "0",
    ];
    let driver = FuelCoreDriver::spawn(&args).await.unwrap();

    let starting_gas_price = 1000;
    let expected_default_percentage_increase = 10;

    let expected_gas_price =
        starting_gas_price * (100 + expected_default_percentage_increase) / 100;

    let mut rng = rand::rngs::StdRng::seed_from_u64(2322u64);

    let base_asset_id = driver
        .client
        .consensus_parameters(0)
        .await
        .unwrap()
        .unwrap()
        .base_asset_id()
        .clone();

    // when
    let arb_tx_count = 20;
    for _ in 0..arb_tx_count {
        let tx = infinite_loop_tx(200_000_000, &mut rng, Some(base_asset_id));
        let _status = driver.client.submit(&tx).await.unwrap();
    }

    // starting gas price
    let _ = driver.client.produce_blocks(1, None).await.unwrap();

    // updated gas price
    let _ = driver.client.produce_blocks(1, None).await.unwrap();
    let latest_gas_price = driver.client.latest_gas_price().await.unwrap().gas_price;

    assert_eq!(expected_gas_price, latest_gas_price);
}

#[tokio::test]
async fn estimate_gas_price__is_greater_than_actual_price_at_desired_height() {
    // given
    let mut node_config = Config::local_node();
    let starting_gas_price = 1000;
    let percent = 10;
    node_config.starting_exec_gas_price = starting_gas_price;
    node_config.exec_gas_price_change_percent = percent;
    // Always increase
    node_config.exec_gas_price_threshold_percent = 0;

    let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // when
    let arbitrary_horizon = 10;

    let estimate = client.estimate_gas_price(arbitrary_horizon).await.unwrap();
    let _ = client.produce_blocks(1, None).await.unwrap();
    for _ in 0..arbitrary_horizon {
        let _ = client.produce_blocks(1, None).await.unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // then
    let latest = client.latest_gas_price().await.unwrap();
    let real = latest.gas_price;
    let estimated = u64::from(estimate.gas_price);
    assert!(estimated >= real);
}

// TODO: this behavior is changing with https://github.com/FuelLabs/fuel-core/pull/2501
// #[tokio::test]
// async fn estimate_gas_price__returns_min_gas_price_if_starting_gas_price_is_zero() {
//     const MIN_GAS_PRICE: u64 = 1;
//
//     // Given
//     let mut node_config = Config::local_node();
//     node_config.min_exec_gas_price = MIN_GAS_PRICE;
//     node_config.starting_exec_gas_price = 0;
//     let srv = FuelService::new_node(node_config.clone()).await.unwrap();
//     let client = FuelClient::from(srv.bound_address);
//
//     // When
//     let result = client.estimate_gas_price(10).await.unwrap();
//
//     // Then
//     let actual = result.gas_price.0;
//     assert_eq!(MIN_GAS_PRICE, actual)
// }

// This test passed before this PR, but doesn't now
#[tokio::test(flavor = "multi_thread")]
async fn latest_gas_price__if_node_restarts_gets_latest_value() {
    // given
    let args = vec![
        "--debug",
        "--poa-instant",
        "true",
        "--starting-gas-price",
        "1000",
        "--gas-price-change-percent",
        "100",
        "--gas-price-threshold-percent",
        "0",
    ];
    let driver = FuelCoreDriver::spawn(&args).await.unwrap();
    let starting = driver.node.shared.config.starting_exec_gas_price;
    let arb_blocks_to_produce = 10;
    for _ in 0..arb_blocks_to_produce {
        driver.client.produce_blocks(1, None).await.unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let latest_gas_price = driver.client.latest_gas_price().await.unwrap();
    let LatestGasPrice { gas_price, .. } = latest_gas_price;
    let expected = gas_price;
    assert_ne!(expected, starting);

    // when
    let temp_dir = driver.kill().await;
    let recovered_driver = FuelCoreDriver::spawn_with_directory(temp_dir, &args)
        .await
        .unwrap();

    // then
    let new_latest_gas_price = recovered_driver.client.latest_gas_price().await.unwrap();
    let LatestGasPrice { gas_price, .. } = new_latest_gas_price;
    let actual = gas_price;
    assert_eq!(expected, actual);
    recovered_driver.kill().await;
}

#[tokio::test]
async fn dry_run_opt__zero_gas_price_equal_to_none_gas_price() {
    // given
    let tx = TransactionBuilder::script(
        op::ret(RegId::ONE).to_bytes().into_iter().collect(),
        vec![],
    )
    .add_fee_input()
    .script_gas_limit(1000)
    .max_fee_limit(600000)
    .finalize_as_transaction();

    let mut test_builder = TestSetupBuilder::new(2322u64);
    test_builder.starting_gas_price = 1;
    let TestContext {
        client,
        srv: _dont_drop,
        ..
    } = test_builder.finalize().await;

    // when
    let TransactionExecutionResult::Success {
        total_fee,
        total_gas,
        ..
    } = client
        .dry_run_opt(&[tx.clone()], Some(false), None)
        .await
        .unwrap()
        .pop()
        .unwrap()
        .result
    else {
        panic!("dry run should have succeeded");
    };

    let TransactionExecutionResult::Success {
        total_fee: total_fee_zero_gas_price,
        total_gas: total_gas_zero_gas_price,
        ..
    } = client
        .dry_run_opt(&[tx], Some(false), Some(0))
        .await
        .unwrap()
        .pop()
        .unwrap()
        .result
    else {
        panic!("dry run should have succeeded");
    };

    // then
    assert_ne!(total_fee, total_fee_zero_gas_price);
    assert_eq!(total_fee_zero_gas_price, 0);

    assert_eq!(total_gas, total_gas_zero_gas_price);
}

#[tokio::test(flavor = "multi_thread")]
async fn startup__can_override_gas_price_values_by_changing_config() {
    // given
    let args = vec![
        "--debug",
        "--poa-instant",
        "true",
        "--gas-price-change-percent",
        "0",
        "--gas-price-threshold-percent",
        "0",
        "--min-gas-price",
        "0",
    ];
    let driver = FuelCoreDriver::spawn(&args).await.unwrap();
    driver.client.produce_blocks(1, None).await.unwrap();
    let temp_dir = driver.kill().await;

    // when
    let new_args = vec![
        "--debug",
        "--poa-instant",
        "true",
        "--gas-price-change-percent",
        "11",
        "--gas-price-threshold-percent",
        "22",
        "--min-gas-price",
        "33",
    ];
    let recovered_driver = FuelCoreDriver::spawn_with_directory(temp_dir, &new_args)
        .await
        .unwrap();

    // then
    recovered_driver
        .client
        .produce_blocks(1, None)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;
    let new_height = 2;

    let recovered_database = &recovered_driver.node.shared.database;
    let recovered_view = recovered_database.gas_price().latest_view().unwrap();
    let new_metadata = recovered_view
        .storage::<GasPriceMetadata>()
        .get(&new_height.into())
        .unwrap()
        .unwrap()
        .deref()
        .clone();

    let V1Metadata {
        l2_block_height, ..
    } = new_metadata.try_into().unwrap();
    assert_eq!(l2_block_height, new_height);
    recovered_driver.kill().await;
}

use fuel_core_gas_price_service::v1::da_source_service::block_committer_costs::RawDaBlockCosts;
use fuel_core_storage::iter::IterDirection;

#[test]
fn produce_block__l1_committed_block_affects_gas_price() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    // set up chain with single unrecorded block
    let mut args = vec![
        "--debug",
        "--poa-instant",
        "true",
        "--min-da-gas-price",
        "100",
    ];

    let mut default_args = args.clone();
    default_args.extend([
        "--da-p-component",
        "0",
        "--da-d-component",
        "0",
        "--starting-gas-price",
        "0",
        "--gas-price-change-percent",
        "0",
        "--max-da-gas-price-change-percent",
        "0",
    ]);

    let (first_gas_price, temp_dir) = rt.block_on(async {
        let driver = FuelCoreDriver::spawn(&default_args).await.unwrap();
        driver.client.produce_blocks(1, None).await.unwrap();
        let first_gas_price: u64 = driver
            .client
            .estimate_gas_price(0)
            .await
            .unwrap()
            .gas_price
            .into();
        tokio::time::sleep(Duration::from_millis(100)).await;
        let temp_dir = driver.kill().await;
        (first_gas_price, temp_dir)
    });

    assert_eq!(100u64, first_gas_price);

    let mut mock = FakeServer::new();
    let url = mock.url();
    let costs = RawDaBlockCosts {
        id: 1,
        start_height: 1,
        end_height: 1,
        da_block_height: DaBlockHeight(100),
        cost: 100,
        size: 100,
    };
    mock.add_response(costs);

    // add the da committer url to the args
    args.extend(&[
        "--da-committer-url",
        &url,
        "--da-poll-interval",
        "1",
        "--da-p-component",
        "1",
        "--max-da-gas-price-change-percent",
        "100",
    ]);

    // when
    let new_gas_price = rt
        .block_on(async {
            let driver = FuelCoreDriver::spawn_with_directory(temp_dir, &args)
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_millis(20)).await;
            // Won't accept DA costs until l2_height is > 1
            driver.client.produce_blocks(1, None).await.unwrap();
            // Wait for DaBlockCosts to be accepted
            tokio::time::sleep(Duration::from_millis(2)).await;
            // Produce new block to update gas price
            driver.client.produce_blocks(1, None).await.unwrap();
            tokio::time::sleep(Duration::from_millis(20)).await;
            driver.client.estimate_gas_price(0).await.unwrap().gas_price
        })
        .into();

    // then
    assert!(first_gas_price < new_gas_price);
    rt.shutdown_timeout(tokio::time::Duration::from_millis(100));
}

#[test]
fn run__if_metadata_is_behind_l2_then_will_catch_up() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();

    // given
    // produce 100 blocks
    let args = vec![
        "--debug",
        "--poa-instant",
        "true",
        "--min-da-gas-price",
        "100",
    ];
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _temp_dir = rt.block_on(async {
        let driver = FuelCoreDriver::spawn(&args).await.unwrap();
        driver.client.produce_blocks(100, None).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        driver.kill().await
    });

    // // rollback 50 blocks
    // let temp_dir = rt.block_on(async {
    //     let driver = FuelCoreDriver::spawn_with_directory(temp_dir, &args)
    //         .await
    //         .unwrap();
    //     for _ in 0..50 {
    //         driver
    //             .node
    //             .shared
    //             .database
    //             .gas_price()
    //             .rollback_last_block()
    //             .unwrap();
    //         let gas_price_db_height = driver
    //             .node
    //             .shared
    //             .database
    //             .gas_price()
    //             .latest_height()
    //             .unwrap();
    //         tracing::info!("gas price db height: {:?}", gas_price_db_height);
    //     }
    //     driver.kill().await
    // });
    //
    // // when
    // // restart node
    // rt.block_on(async {
    //     let driver = FuelCoreDriver::spawn_with_directory(temp_dir, &args)
    //         .await
    //         .unwrap();
    //     let onchain_db_height = driver
    //         .node
    //         .shared
    //         .database
    //         .on_chain()
    //         .latest_height_from_metadata()
    //         .unwrap()
    //         .unwrap();
    //     let gas_price_db_height = driver
    //         .node
    //         .shared
    //         .database
    //         .gas_price()
    //         .latest_height()
    //         .unwrap();
    //     assert_eq!(onchain_db_height, gas_price_db_height);
    // });
}

#[test]
fn produce_block__algorithm_recovers_from_divergent_profit() {
    _produce_block__algorithm_recovers_from_divergent_profit(110);
}

fn _produce_block__algorithm_recovers_from_divergent_profit(block_delay: usize) {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322u64);

    // given
    let mut mock = FakeServer::new();
    let url = mock.url();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let block_gas_limit = 3_000_000;
    let chain_config = ChainConfig {
        consensus_parameters: ConsensusParameters::V1(ConsensusParametersV1 {
            block_gas_limit,
            ..Default::default()
        }),
        ..ChainConfig::local_testnet()
    };
    let mut node_config =
        Config::local_node_with_configs(chain_config, StateConfig::local_testnet());
    let starting_gas_price = 10_000_000;
    node_config.block_producer.coinbase_recipient = Some([5; 32].into());
    node_config.min_da_gas_price = starting_gas_price;
    node_config.max_da_gas_price_change_percent = 15;
    node_config.block_production = Trigger::Never;
    node_config.da_committer_url = Some(url.clone());
    node_config.da_poll_interval = Some(100);
    node_config.da_p_component = 224_000;
    node_config.da_d_component = 2_690_000;
    // node_config.da_p_component = 1;
    // node_config.da_d_component = 10;
    node_config.block_activity_threshold = 0;

    let (srv, client) = rt.block_on(async {
        let srv = FuelService::new_node(node_config.clone()).await.unwrap();
        let client = FuelClient::from(srv.bound_address);

        for _b in 0..block_delay {
            produce_a_block(&client, &mut rng).await;
        }
        let _ = client.produce_blocks(1, None).await.unwrap();

        let height = srv.shared.database.gas_price().latest_height().unwrap();
        let metadata = srv
            .shared
            .database
            .gas_price()
            .get_metadata(&height)
            .unwrap()
            .and_then(|x| x.v1().cloned())
            .unwrap();
        tracing::info!("metadata: {:?}", metadata);
        assert_ne!(metadata.last_profit, 0);
        (srv, client)
    });

    let half_of_blocks = block_delay as u32 / 2;
    let count = half_of_blocks;
    let block_bytes = 1000;
    let total_size_bytes = block_bytes * count as u32;
    let gas = 16 * total_size_bytes as u128;
    let cost_gwei = gas * 1; // blob gas price 1 gwei
    let cost = cost_gwei * 1_000_000_000; // Wei
    mock.add_response(RawDaBlockCosts {
        id: 1,
        start_height: 1,
        end_height: half_of_blocks,
        da_block_height: DaBlockHeight(100),
        cost,
        size: total_size_bytes,
    });

    let mut profits = Vec::new();
    let mut gas_prices = Vec::new();
    rt.block_on(async {
        tokio::time::sleep(Duration::from_millis(200)).await;
        client.produce_blocks(1, None).await.unwrap();
        client.produce_blocks(1, None).await.unwrap();
        let height = srv.shared.database.gas_price().latest_height().unwrap();
        let metadata = srv
            .shared
            .database
            .gas_price()
            .get_metadata(&height)
            .unwrap()
            .and_then(|x| x.v1().cloned())
            .unwrap();
        tracing::info!("metadata: {:?}", metadata);
        profits.push(metadata.last_profit);
        gas_prices.push(metadata.new_scaled_da_gas_price / metadata.gas_price_factor);
    });

    let tries = 1000;

    let mut success = false;
    let mut success_iteration = i32::MAX;
    rt.block_on(async {
        for i in 0..tries {
            produce_a_block(&client, &mut rng).await;
            let metadata = srv
                .shared
                .database
                .gas_price()
                .get_metadata(&srv.shared.database.gas_price().latest_height().unwrap())
                .unwrap()
                .and_then(|x| x.v1().cloned())
                .unwrap();
            let profit = metadata.last_profit;
            tracing::info!("metadata: {:?}", metadata);
            profits.push(profit);
            gas_prices.push(metadata.new_scaled_da_gas_price / metadata.gas_price_factor);
            if profit > 0 && !success {
                success = true;
                success_iteration = i as i32;
            }
        }
    });
    let changes = profits.windows(2).map(|x| x[1] - x[0]).collect::<Vec<_>>();
    let gas_price_changes = gas_prices
        .windows(2)
        .map(|x| x[1] as i128 - x[0] as i128)
        .collect::<Vec<_>>();
    if !success {
        panic!(
            "Could not recover from divergent profit after {} tries.\n Profits: {:?}.\n Changes: {:?}.\n Gas prices: {:?}\n Gas price changes: {:?}",
            tries, profits, changes, gas_prices, gas_price_changes
        );
    } else {
        tracing::info!("Success on try {}/{}", success_iteration, tries);
        let profits_as_gwei = profits
            .iter()
            .map(|x| *x as f64 / 1_000_000_000.0)
            .collect::<Vec<_>>();
        let changes_as_gwei = changes
            .iter()
            .map(|x| *x as f64 / 1_000_000_000.0)
            .collect::<Vec<_>>();
        let gas_prices_as_gwei = gas_prices
            .iter()
            .map(|x| *x as f64 / 1_000_000_000.0)
            .collect::<Vec<_>>();
        let gas_price_changes_as_gwei = gas_price_changes
            .iter()
            .map(|x| *x as f64 / 1_000_000_000.0)
            .collect::<Vec<_>>();
        tracing::info!("Profits as gwei: {:?}", profits_as_gwei);
        tracing::info!("Changes as gwei: {:?}", changes_as_gwei);
        tracing::info!("Gas prices as gwei: {:?}", gas_prices_as_gwei);
        tracing::info!("Gas price changes as gwei: {:?}", gas_price_changes_as_gwei);
    }
}

async fn produce_a_block<R: Rng + rand::CryptoRng>(client: &FuelClient, rng: &mut R) {
    let arb_tx_count = 2;
    for i in 0..arb_tx_count {
        let large_fee_limit = u32::MAX as u64 - i;
        // let tx = arb_large_tx(large_fee_limit, rng);
        let tx = arb_small_tx(large_fee_limit, rng);
        // let tx = arb_large_tx(189028 + i as Word, rng);
        let _status = client.submit(&tx).await.unwrap();
    }
    let _ = client.produce_blocks(1, None).await.unwrap();
}
