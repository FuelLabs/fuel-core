#![allow(non_snake_case)]

use fuel_core::{
    database::Database,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_block_aggregator_api::{
    blocks::old_block_source::convertor_adapter::proto_to_fuel_conversions::fuel_block_from_protobuf,
    protobuf_types::{
        Block as ProtoBlock,
        NewBlockSubscriptionRequest as ProtoNewBlockSubscriptionRequest,
        block_aggregator_client::BlockAggregatorClient as ProtoBlockAggregatorClient,
        block_response::Payload as ProtoPayload,
    },
};
use fuel_core_client::client::FuelClient;
use fuel_core_types::{
    fuel_tx::*,
    fuel_types::BlockHeight,
};
use futures::StreamExt;
use prost::Message;
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
    let rpc_url = config.rpc_config.clone().unwrap().addr;

    let srv = FuelService::from_database(Database::default(), config.clone())
        .await
        .unwrap();

    let fuel_client = FuelClient::from(srv.bound_address);

    let tx = Transaction::default_test_tx();

    let rpc_url = format!("http://{}", rpc_url);
    let mut rpc_client = ProtoBlockAggregatorClient::connect(rpc_url)
        .await
        .expect("could not connect to server");

    let request = ProtoNewBlockSubscriptionRequest {};
    let mut stream = rpc_client
        .new_block_subscription(request)
        .await
        .unwrap()
        .into_inner();
    let _ = fuel_client.submit_and_await_commit(&tx).await.unwrap();

    // when
    let next = tokio::time::timeout(std::time::Duration::from_secs(1), stream.next())
        .await
        .unwrap();
    let bytes = if let Some(ProtoPayload::Bytes(block)) = next.unwrap().unwrap().payload {
        block
    } else {
        panic!("expected literal block payload");
    };

    let proto_block = ProtoBlock::decode(bytes.as_slice()).unwrap();
    let (actual_block, receipts) =
        fuel_block_from_protobuf(proto_block, &[], Bytes32::default()).unwrap();
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
