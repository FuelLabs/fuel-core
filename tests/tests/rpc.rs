#![allow(non_snake_case)]

use fuel_block_aggregator_api::protobuf_types::{
    BlockHeightRequest as ProtoBlockHeightRequest,
    BlockRangeRequest as ProtoBlockRangeRequest,
    NewBlockSubscriptionRequest as ProtoNewBlockSubscriptionRequest,
    block::VersionedBlock as ProtoVersionedBlock,
    block_aggregator_client::BlockAggregatorClient as ProtoBlockAggregatorClient,
    block_response::Payload as ProtoPayload,
    header::VersionedHeader as ProtoVersionedHeader,
};
use fuel_core::{
    database::Database,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_client::client::FuelClient;
use fuel_core_types::{
    blockchain::block::Block,
    fuel_tx::*,
    fuel_types::BlockHeight,
};
use futures::StreamExt;
use test_helpers::client_ext::ClientExt;

#[tokio::test(flavor = "multi_thread")]
async fn get_block_range__can_get_serialized_block_from_rpc() {
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

    let expected_block = graphql_client
        .full_block_by_height(1)
        .await
        .unwrap()
        .unwrap();
    let expected_header = expected_block.header;

    // when
    let request = ProtoBlockRangeRequest { start: 1, end: 1 };
    let actual_block = if let Some(ProtoPayload::Literal(block)) = rpc_client
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

    let actual_height = if let ProtoVersionedBlock::V1(v1_block) =
        actual_block.versioned_block.unwrap()
    {
        if let ProtoVersionedHeader::V1(v1_header) =
            v1_block.header.unwrap().versioned_header.unwrap()
        {
            v1_header.height
        } else {
            panic!("expected V1 header");
        }
    } else {
        panic!("expected V1 block");
    };
    // then
    assert_eq!(expected_header.height.0, actual_height);
    // check txs
    // let actual_tx = actual_block.transactions().first().unwrap();
    // let expected_opaque_tx = expected_block.transactions.first().unwrap().to_owned();
    // let expected_tx: Transaction = expected_opaque_tx.try_into().unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn get_block_height__can_get_value_from_rpc() {
    let config = Config::local_node();
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
    let request = ProtoBlockHeightRequest {};
    let expected_height = 1;
    let actual_height = rpc_client
        .get_block_height(request)
        .await
        .unwrap()
        .into_inner()
        .height;

    // then
    assert_eq!(expected_height, actual_height);
}

#[tokio::test(flavor = "multi_thread")]
async fn new_block_subscription__can_get_expect_block() {
    let config = Config::local_node();
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
    let actual_bytes =
        if let Some(ProtoPayload::Literal(block)) = next.unwrap().unwrap().payload {
            block.data
        } else {
            panic!("expected literal block payload");
        };

    // then
    let expected_block = graphql_client
        .full_block_by_height(1)
        .await
        .unwrap()
        .unwrap();
    let header = expected_block.header;
    let actual_block: Block<Transaction> = postcard::from_bytes(&actual_bytes).unwrap();
    assert_eq!(
        BlockHeight::from(header.height.0),
        *actual_block.header().height()
    );
    // check txs
    let actual_tx = actual_block.transactions().first().unwrap();
    let expected_opaque_tx = expected_block.transactions.first().unwrap().to_owned();
    let expected_tx: Transaction = expected_opaque_tx.try_into().unwrap();

    assert_eq!(&expected_tx, actual_tx);
}
