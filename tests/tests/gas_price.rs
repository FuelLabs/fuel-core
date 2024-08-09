#![allow(non_snake_case)]

use crate::helpers::{
    TestContext,
    TestSetupBuilder,
};
use fuel_core::{
    chain_config::{
        ChainConfig,
        StateConfig,
    },
    database::Database,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_client::client::{
    types::gas_price::LatestGasPrice,
    FuelClient,
};
use fuel_core_gas_price_service::fuel_gas_price_updater::{
    fuel_core_storage_adapter::storage::GasPriceMetadata,
    UpdaterMetadata,
    V0Metadata,
};
use fuel_core_poa::Trigger;
use fuel_core_storage::{
    transactional::AtomicView,
    StorageAsRef,
};
use fuel_core_types::{
    fuel_asm::*,
    fuel_crypto::{
        coins_bip32::ecdsa::signature::rand_core::SeedableRng,
        SecretKey,
    },
    fuel_tx::{
        consensus_parameters::ConsensusParametersV1,
        ConsensusParameters,
        Finalizable,
        Transaction,
        TransactionBuilder,
    },
    services::executor::TransactionExecutionResult,
};
use rand::Rng;
use std::{
    iter::repeat,
    ops::Deref,
    time::Duration,
};
use test_helpers::fuel_core_driver::FuelCoreDriver;

fn tx_for_gas_limit(max_fee_limit: Word) -> Transaction {
    TransactionBuilder::script(vec![], vec![])
        .max_fee_limit(max_fee_limit)
        .add_random_fee_input()
        .finalize()
        .into()
}

fn arb_large_tx<R: Rng + rand::CryptoRng>(
    max_fee_limit: Word,
    rng: &mut R,
) -> Transaction {
    let mut script: Vec<_> = repeat(op::noop()).take(10_000).collect();
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
    config.starting_gas_price = starting_gas_price;
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
    node_config.starting_gas_price = starting_gas_price;
    node_config.gas_price_change_percent = percent;
    node_config.gas_price_threshold_percent = threshold;
    node_config.block_production = Trigger::Never;

    let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322u64);

    // when
    let arb_tx_count = 10;
    for i in 0..arb_tx_count {
        let tx = arb_large_tx(189028 + i as Word, &mut rng);
        let _status = client.submit(&tx).await.unwrap();
    }
    // starting gas price
    let _ = client.produce_blocks(1, None).await.unwrap();
    // updated gas price
    let _ = client.produce_blocks(1, None).await.unwrap();

    // then
    let change = starting_gas_price * percent / 100;
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
    node_config.starting_gas_price = starting_gas_price;
    node_config.gas_price_change_percent = percent;
    node_config.gas_price_threshold_percent = threshold;
    node_config.block_production = Trigger::Never;

    let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322u64);

    // when
    let arb_tx_count = 5;
    for i in 0..arb_tx_count {
        let tx = arb_large_tx(189028 + i as Word, &mut rng);
        let _status = client.submit(&tx).await.unwrap();
    }
    // starting gas price
    let _ = client.produce_blocks(1, None).await.unwrap();
    // updated gas price
    let _ = client.produce_blocks(1, None).await.unwrap();

    // then
    let change = starting_gas_price * percent / 100;
    let expected = starting_gas_price - change;
    let latest = client.latest_gas_price().await.unwrap();
    let actual = latest.gas_price;
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn estimate_gas_price__is_greater_than_actual_price_at_desired_height() {
    // given
    let mut node_config = Config::local_node();
    let starting_gas_price = 1000;
    let percent = 10;
    node_config.starting_gas_price = starting_gas_price;
    node_config.gas_price_change_percent = percent;
    // Always increase
    node_config.gas_price_threshold_percent = 0;

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

#[tokio::test]
async fn estimate_gas_price__returns_min_gas_price_if_starting_gas_price_is_zero() {
    const MIN_GAS_PRICE: u64 = 1;

    // Given
    let mut node_config = Config::local_node();
    node_config.min_gas_price = MIN_GAS_PRICE;
    node_config.starting_gas_price = 0;
    let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // When
    let result = client.estimate_gas_price(10).await.unwrap();

    // Then
    let actual = result.gas_price.0;
    assert_eq!(MIN_GAS_PRICE, actual)
}

#[tokio::test]
async fn latest_gas_price__if_node_restarts_gets_latest_value() {
    // given
    let args = vec![
        "--debug",
        "--poa-instant",
        "true",
        "--starting-gas-price",
        "1000",
        "--gas-price-change-percent",
        "10",
        "--gas-price-threshold-percent",
        "0",
    ];
    let driver = FuelCoreDriver::spawn(&args).await.unwrap();
    let starting = driver.node.shared.config.starting_gas_price;
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
}

#[tokio::test]
async fn dry_run_opt__zero_gas_price_equal_to_none_gas_price() {
    // given
    let tx = TransactionBuilder::script(
        op::ret(RegId::ONE).to_bytes().into_iter().collect(),
        vec![],
    )
    .add_random_fee_input()
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

#[tokio::test]
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

    let UpdaterMetadata::V0(V0Metadata {
        min_exec_gas_price,
        exec_gas_price_change_percent,
        l2_block_height,
        l2_block_fullness_threshold_percent,
        ..
    }) = new_metadata;
    assert_eq!(exec_gas_price_change_percent, 11);
    assert_eq!(l2_block_fullness_threshold_percent, 22);
    assert_eq!(min_exec_gas_price, 33);
    assert_eq!(l2_block_height, new_height);
}
