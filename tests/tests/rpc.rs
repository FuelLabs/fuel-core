#![allow(non_snake_case)]

use fuel_core::{
    database::Database,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_client::client::FuelClient;
use fuel_core_types::{
    fuel_tx::*,
    fuel_types::BlockHeight,
};
use futures::StreamExt;
use tokio::time::sleep;

#[tokio::test(flavor = "multi_thread")]
async fn get_block_range__can_get_serialized_block_from_rpc__literal() {
    let config = Config::local_node_with_rpc();

    let srv = FuelService::from_database(Database::default(), config.clone())
        .await
        .unwrap();

    let fuel_client = FuelClient::new_with_rpc(
        srv.bound_address.to_string(),
        srv.rpc_address.unwrap().to_string(),
    )
    .await
    .unwrap();

    let tx = Transaction::default_test_tx();
    let _ = fuel_client.submit_and_await_commit(&tx).await.unwrap();

    // when
    let next = fuel_client
        .get_block_range(BlockHeight::new(1), BlockHeight::new(1))
        .await
        .unwrap()
        .next()
        .await
        .unwrap();
    let (actual_block, receipts) = next.unwrap();
    let actual_height = actual_block.header().height();

    // then
    let expected_height = BlockHeight::new(1);
    assert_eq!(&expected_height, actual_height);

    assert!(
        matches!(
            receipts[0][1],
            Receipt::ScriptResult {
                result: ScriptExecutionResult::Success,
                ..
            }
        ),
        "should have a script result receipt, received: {:?}",
        receipts
    );
    assert!(
        matches!(receipts[0][0], Receipt::Return { .. }),
        "should have a return receipt, received: {:?}",
        receipts
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn get_aggregated_height__can_get_value_from_rpc() {
    let config = Config::local_node_with_rpc();

    // given
    let srv = FuelService::from_database(Database::default(), config.clone())
        .await
        .unwrap();

    let fuel_client = FuelClient::new_with_rpc(
        srv.bound_address.to_string(),
        srv.rpc_address.unwrap().to_string(),
    )
    .await
    .unwrap();

    let tx = Transaction::default_test_tx();
    let _ = fuel_client.submit_and_await_commit(&tx).await.unwrap();

    sleep(std::time::Duration::from_secs(1)).await;
    let expected_height = BlockHeight::new(1);

    // when
    let actual_height = fuel_client.get_aggregated_height().await.unwrap();

    // then
    assert_eq!(expected_height, actual_height);
}

#[tokio::test(flavor = "multi_thread")]
async fn new_block_subscription__can_get_expect_block() {
    let config = Config::local_node_with_rpc();

    let srv = FuelService::from_database(Database::default(), config.clone())
        .await
        .unwrap();

    let fuel_client = FuelClient::new_with_rpc(
        srv.bound_address.to_string(),
        srv.rpc_address.unwrap().to_string(),
    )
    .await
    .unwrap();

    let tx = Transaction::default_test_tx();

    let mut stream = fuel_client.new_block_subscription().await.unwrap();
    let _ = fuel_client.submit_and_await_commit(&tx).await.unwrap();
    let expected_height = BlockHeight::new(1);

    // when
    let (actual_block, receipts) =
        tokio::time::timeout(std::time::Duration::from_secs(1), &mut stream.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();

    // then
    let actual_height = actual_block.header().height();
    assert_eq!(&expected_height, actual_height);
    assert!(
        matches!(
            receipts[0][1],
            Receipt::ScriptResult {
                result: ScriptExecutionResult::Success,
                ..
            }
        ),
        "should have a script result receipt, received: {:?}",
        receipts
    );
    assert!(
        matches!(receipts[0][0], Receipt::Return { .. }),
        "should have a return receipt, received: {:?}",
        receipts
    );
}
