#![allow(non_snake_case)]

use crate::helpers::{
    TestContext,
    TestSetupBuilder,
};
use fuel_core::database::Database;

use fuel_core::service::{
    Config,
    FuelService,
};
use fuel_core_client::client::{
    schema::gas_price::EstimateGasPrice,
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
async fn estimate_gas_price__should_be_estimated_correctly() {
    // given
    let mut node_config = Config::local_node();
    let starting_gas_price = 1000;
    let percent = 10;
    node_config.starting_gas_price = starting_gas_price;
    node_config.gas_price_change_percent = percent;

    let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // when
    let arbitrary_horizon = 10;

    let EstimateGasPrice { gas_price } =
        client.estimate_gas_price(arbitrary_horizon).await.unwrap();

    // then
    let mut expected = starting_gas_price;
    for _ in 0..arbitrary_horizon {
        expected = expected + (expected * percent / 100);
    }

    let actual = u64::from(gas_price);
    assert_eq!(expected, actual);
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
