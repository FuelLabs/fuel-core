#![allow(non_snake_case)]

use crate::helpers::{
    TestContext,
    TestSetupBuilder,
};
use fuel_core::database::Database;
use std::time::Duration;

use fuel_core::service::{
    Config,
    FuelService,
};
use fuel_core_client::client::{
    types::gas_price::LatestGasPrice,
    FuelClient,
};
use fuel_core_types::{
    fuel_asm::*,
    fuel_tx::{
        Finalizable,
        Transaction,
        TransactionBuilder,
    },
    services::executor::TransactionExecutionResult,
};

fn tx_for_gas_limit(max_fee_limit: Word) -> Transaction {
    TransactionBuilder::script(vec![], vec![])
        .max_fee_limit(max_fee_limit)
        .add_random_fee_input()
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
async fn produce_block__updates_gas_price() {
    let _ = tracing_subscriber::fmt::try_init();
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
async fn produce_block__matches_actual_updated_gas_price() {
    let _ = tracing_subscriber::fmt::try_init();
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
    assert_eq!(estimated, real);
}

#[tokio::test]
async fn latest_gas_price__if_node_restarts_gets_latest_value() {
    todo!();
    // // given
    // let node_config = Config::local_node();
    // let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    // let client = FuelClient::from(srv.bound_address);
    //
    // // when
    // let LatestGasPrice { gas_price, .. } = client.latest_gas_price().await.unwrap();
    //
    // // then
    // let expected = 0;
    // let actual = gas_price;
    // assert_eq!(expected, actual)
}

#[tokio::test]
async fn dry_run_opt_with_zero_gas_price() {
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

    assert_ne!(total_fee, total_fee_zero_gas_price);
    assert_eq!(total_fee_zero_gas_price, 0);

    assert_eq!(total_gas, total_gas_zero_gas_price);
}
