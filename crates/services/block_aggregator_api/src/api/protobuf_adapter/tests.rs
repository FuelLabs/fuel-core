#![allow(non_snake_case)]

use crate::{
    api::{
        BlockAggregatorApi,
        BlockAggregatorQuery,
        protobuf_adapter::{
            BlockHeightRequest,
            BlockRangeRequest,
            ProtobufAPI,
            block_aggregator_client::BlockAggregatorClient,
        },
    },
    block_range_response::BlockRangeResponse,
    blocks::Block,
};
use bytes::Bytes;
use fuel_core_types::fuel_types::BlockHeight;
use futures::{
    StreamExt,
    TryStreamExt,
};

#[tokio::test]
async fn await_query__get_current_height__client_receives_expected_value() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    // given
    let path = "[::1]:50051";
    let mut api = ProtobufAPI::new(path.to_string());
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // call get current height endpoint with client
    let url = "http://[::1]:50051";
    let mut client = BlockAggregatorClient::connect(url.to_string())
        .await
        .expect("could not connect to server");
    let handle = tokio::spawn(async move {
        tracing::info!("querying with client");
        client
            .get_block_height(BlockHeightRequest {})
            .await
            .expect("could not get height")
    });

    // when
    tracing::info!("awaiting query");
    let query = api.await_query().await.unwrap();

    // then
    // return response through query's channel
    if let BlockAggregatorQuery::GetCurrentHeight { response } = query {
        response.send(BlockHeight::new(42)).unwrap();
    } else {
        panic!("expected GetCurrentHeight query");
    }
    let res = handle.await.unwrap();

    // assert client received expected value
    assert_eq!(res.into_inner().height, 42);
}

#[tokio::test]
async fn await_query__get_block_range__client_receives_expected_value() {
    // given
    let path = "[::1]:50051";
    let mut api = ProtobufAPI::new(path.to_string());
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // call get current height endpoint with client
    let url = "http://[::1]:50051";
    let mut client = BlockAggregatorClient::connect(url.to_string())
        .await
        .expect("could not connect to server");
    let request = BlockRangeRequest { start: 0, end: 1 };
    let handle = tokio::spawn(async move {
        tracing::info!("querying with client");
        client
            .get_block_range(request)
            .await
            .expect("could not get height")
    });

    // when
    tracing::info!("awaiting query");
    let query = api.await_query().await.unwrap();

    // then
    let block1 = Block::new(Bytes::from(vec![0u8; 100]));
    let block2 = Block::new(Bytes::from(vec![1u8; 100]));
    let list = vec![block1, block2];
    // return response through query's channel
    if let BlockAggregatorQuery::GetBlockRange {
        first,
        last,
        response,
    } = query
    {
        assert_eq!(first, BlockHeight::new(0));
        assert_eq!(last, BlockHeight::new(1));
        tracing::info!("correct query received, sending response");
        let stream = tokio_stream::iter(list.clone()).boxed();
        let range = BlockRangeResponse::Literal(stream);
        response.send(range).unwrap();
    } else {
        panic!("expected GetBlockRange query");
    }
    tracing::info!("awaiting query");
    let response = handle.await.unwrap();
    let expected: Vec<Vec<u8>> = list.iter().map(|b| b.bytes().to_vec()).collect();
    let actual: Vec<Vec<u8>> = response
        .into_inner()
        .try_collect::<Vec<_>>()
        .await
        .unwrap()
        .into_iter()
        .map(|b| b.data.to_vec())
        .collect();

    assert_eq!(expected, actual);
}
