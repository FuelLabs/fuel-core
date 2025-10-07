#![allow(non_snake_case)]

use crate::protobuf_types::{
    Block as ProtoBlock,
    block_aggregator_client::{
        BlockAggregatorClient as ProtoBlockAggregatorClient,
        BlockAggregatorClient,
    },
};
use crate::{
    NewBlock,
    api::{
        BlockAggregatorApi,
        BlockAggregatorQuery,
        protobuf_adapter::ProtobufAPI,
    },
    block_range_response::BlockRangeResponse,
    blocks::Block,
    protobuf_types::{
        BlockHeightRequest,
        BlockRangeRequest,
        NewBlockSubscriptionRequest,
        // block_aggregator_client::BlockAggregatorClient,
        block_response::Payload,
    },
};
use bytes::Bytes;
use fuel_core_types::fuel_types::BlockHeight;
use futures::{
    StreamExt,
    TryStreamExt,
};
use std::net::TcpListener;

fn free_local_addr() -> String {
    let listener = TcpListener::bind("[::1]:0").unwrap();
    let addr = listener.local_addr().unwrap(); // OS picks a free port
    format!("[::1]:{}", addr.port())
}

#[tokio::test]
async fn await_query__get_current_height__client_receives_expected_value() {
    // given
    let path = free_local_addr();
    let mut api = ProtobufAPI::new(path.to_string());
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // call get current height endpoint with client
    let url = format!("http://{}", path);
    let mut client = ProtoBlockAggregatorClient::connect(url.to_string())
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
    let path = free_local_addr();
    let mut api = ProtobufAPI::new(path.to_string());
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // call get current height endpoint with client
    let url = format!("http://{}", path);
    let mut client = ProtoBlockAggregatorClient::connect(url.to_string())
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
    // let block1 = Block::new(Bytes::from(vec![0u8; 100]));
    // let block2 = Block::new(Bytes::from(vec![1u8; 100]));
    let block1 = ProtoBlock {
        data: vec![0u8; 100],
    };
    let block2 = ProtoBlock {
        data: vec![1u8; 100],
    };
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
    let expected: Vec<Vec<u8>> = list.iter().map(|b| b.data.to_vec()).collect();
    let actual: Vec<Vec<u8>> = response
        .into_inner()
        .try_collect::<Vec<_>>()
        .await
        .unwrap()
        .into_iter()
        .map(|b| {
            if let Some(Payload::Literal(inner)) = b.payload {
                inner.data.to_vec()
            } else {
                panic!("unexpected response type")
            }
        })
        .collect();

    assert_eq!(expected, actual);
}

#[tokio::test]
async fn await_query__new_block_stream__client_receives_expected_value() {
    // given
    let path = free_local_addr();
    let mut api = ProtobufAPI::new(path.to_string());
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // call get current height endpoint with client
    let url = format!("http://{}", path);
    let mut client = BlockAggregatorClient::connect(url.to_string())
        .await
        .expect("could not connect to server");
    let request = NewBlockSubscriptionRequest {};
    let handle = tokio::spawn(async move {
        tracing::info!("querying with client");
        client
            .new_block_subscription(request)
            .await
            .expect("could not get height")
    });

    // when
    tracing::info!("awaiting query");
    let query = api.await_query().await.unwrap();

    // then
    let height1 = BlockHeight::new(0);
    let height2 = BlockHeight::new(1);
    // let block1 = Block::new(Bytes::from(vec![0u8; 100]));
    // let block2 = Block::new(Bytes::from(vec![1u8; 100]));
    let block1 = ProtoBlock {
        data: vec![0u8; 100],
    };
    let block2 = ProtoBlock {
        data: vec![1u8; 100],
    };
    let list = vec![(height1, block1), (height2, block2)];
    if let BlockAggregatorQuery::NewBlockSubscription { response } = query {
        tracing::info!("correct query received, sending response");
        for (height, block) in list.clone() {
            let new_block = block;
            response.send(new_block).await.unwrap();
        }
    } else {
        panic!("expected GetBlockRange query");
    }
    tracing::info!("awaiting query");
    let response = handle.await.unwrap();
    let expected: Vec<Vec<u8>> = list.iter().map(|(_, b)| b.data.to_vec()).collect();
    let actual: Vec<Vec<u8>> = response
        .into_inner()
        .try_collect::<Vec<_>>()
        .await
        .unwrap()
        .into_iter()
        .map(|b| {
            if let Some(Payload::Literal(inner)) = b.payload {
                inner.data.to_vec()
            } else {
                panic!("unexpected response type")
            }
        })
        .collect();

    assert_eq!(expected, actual);
}
