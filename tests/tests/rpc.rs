#![allow(non_snake_case)]

use aws_config::{
    BehaviorVersion,
    default_provider::credentials::DefaultCredentialsChain,
};
use aws_sdk_s3::Client;
use flate2::read::GzDecoder;
use fuel_core::{
    database::Database,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_block_aggregator_api::{
    blocks::importer_and_db_source::serializer_adapter::proto_to_fuel_conversions::fuel_block_from_protobuf,
    db::remote_cache::block_height_to_key,
    integration::StorageMethod,
    protobuf_types::{
        Block as ProtoBlock,
        BlockHeightRequest as ProtoBlockHeightRequest,
        BlockRangeRequest as ProtoBlockRangeRequest,
        NewBlockSubscriptionRequest as ProtoNewBlockSubscriptionRequest,
        RemoteBlockResponse as ProtoRemoteBlockResponse,
        RemoteS3Bucket,
        block_aggregator_client::BlockAggregatorClient as ProtoBlockAggregatorClient,
        block_response::Payload as ProtoPayload,
        remote_block_response::Location,
    },
};
use fuel_core_client::client::FuelClient;
use fuel_core_types::{
    fuel_tx::*,
    fuel_types::BlockHeight,
};
use futures::StreamExt;
use prost::bytes::Bytes;
use std::io::Read;
use test_helpers::client_ext::ClientExt;
use tokio::time::sleep;

#[tokio::test(flavor = "multi_thread")]
async fn get_block_range__can_get_serialized_block_from_rpc__literal() {
    let config = Config::local_node();
    let rpc_url = config.rpc_config.addr;

    let srv = FuelService::from_database(Database::default(), config.clone())
        .await
        .unwrap();

    let graphql_client = FuelClient::from(srv.bound_address);

    let tx = Transaction::default_test_tx();
    let _ = graphql_client.submit_and_await_commit(&tx).await.unwrap();

    let rpc_url = format!("http://{}", rpc_url);
    let mut rpc_client = ProtoBlockAggregatorClient::connect(rpc_url)
        .await
        .expect("could not connect to server");

    // when
    let request = ProtoBlockRangeRequest { start: 1, end: 1 };
    let proto_block = if let Some(ProtoPayload::Literal(block)) = rpc_client
        .get_block_range(request)
        .await
        .unwrap()
        .into_inner()
        .next()
        .await
        .unwrap()
        .unwrap()
        .payload
    {
        block
    } else {
        panic!("expected literal block payload");
    };

    let (actual_block, receipts) =
        fuel_block_from_protobuf(proto_block, &[], Bytes32::default()).unwrap();
    let actual_height = actual_block.header().height();

    // then
    let expected_height = BlockHeight::new(1);
    assert_eq!(&expected_height, actual_height);

    assert!(
        matches!(
            receipts[1],
            Receipt::ScriptResult {
                result: ScriptExecutionResult::Success,
                ..
            }
        ),
        "should have a script result receipt, received: {:?}",
        receipts
    );
    assert!(
        matches!(receipts[0], Receipt::Return { .. }),
        "should have a return receipt, received: {:?}",
        receipts
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn get_block_height__can_get_value_from_rpc() {
    let mut config = Config::local_node();
    let rpc_url = config.rpc_config.addr;

    // given
    let srv = FuelService::from_database(Database::default(), config.clone())
        .await
        .unwrap();

    let graphql_client = FuelClient::from(srv.bound_address);

    let tx = Transaction::default_test_tx();
    let _ = graphql_client.submit_and_await_commit(&tx).await.unwrap();

    let rpc_url = format!("http://{}", rpc_url);
    let mut rpc_client = ProtoBlockAggregatorClient::connect(rpc_url)
        .await
        .expect("could not connect to server");

    // when
    sleep(std::time::Duration::from_secs(1)).await;
    let request = ProtoBlockHeightRequest {};
    let expected_height = Some(1);
    let actual_height = rpc_client
        .get_synced_block_height(request)
        .await
        .unwrap()
        .into_inner()
        .height;

    // then
    assert_eq!(expected_height, actual_height);
}

#[tokio::test(flavor = "multi_thread")]
async fn new_block_subscription__can_get_expect_block() {
    let mut config = Config::local_node();

    let rpc_url = config.rpc_config.addr;

    let srv = FuelService::from_database(Database::default(), config.clone())
        .await
        .unwrap();

    let graphql_client = FuelClient::from(srv.bound_address);

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
    let _ = graphql_client.submit_and_await_commit(&tx).await.unwrap();

    // when
    let next = tokio::time::timeout(std::time::Duration::from_secs(1), stream.next())
        .await
        .unwrap();
    let proto_block =
        if let Some(ProtoPayload::Literal(block)) = next.unwrap().unwrap().payload {
            block
        } else {
            panic!("expected literal block payload");
        };

    let (actual_block, receipts) =
        fuel_block_from_protobuf(proto_block, &[], Bytes32::default()).unwrap();
    let actual_height = actual_block.header().height();

    // then
    let expected_height = BlockHeight::new(1);
    assert_eq!(&expected_height, actual_height);

    assert!(
        matches!(
            receipts[1],
            Receipt::ScriptResult {
                result: ScriptExecutionResult::Success,
                ..
            }
        ),
        "should have a script result receipt, received: {:?}",
        receipts
    );
    assert!(
        matches!(receipts[0], Receipt::Return { .. }),
        "should have a return receipt, received: {:?}",
        receipts
    );
}
