#![allow(non_snake_case)]

use crate::{
    api::{
        BlockAggregatorApi,
        BlockAggregatorQuery,
        protobuf_adapter::ProtobufAPI,
    },
    block_range_response::{
        BlockRangeResponse,
        RemoteBlockRangeResponse,
    },
    blocks::importer_and_db_source::{
        BlockSerializer,
        serializer_adapter::SerializerAdapter,
    },
    protobuf_types::{
        Block as ProtoBlock,
        BlockHeightRequest,
        BlockRangeRequest,
        NewBlockSubscriptionRequest,
        block_aggregator_client::{
            BlockAggregatorClient as ProtoBlockAggregatorClient,
            BlockAggregatorClient,
        },
        block_response::Payload,
    },
};
use fuel_core_types::{
    blockchain::block::Block as FuelBlock,
    fuel_types::BlockHeight,
};
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
        response.send(Some(BlockHeight::new(42))).unwrap();
    } else {
        panic!("expected GetCurrentHeight query");
    }
    let res = handle.await.unwrap();

    // assert client received expected value
    assert_eq!(res.into_inner().height, Some(42));
}

#[tokio::test]
async fn await_query__get_block_range__client_receives_expected_value__literal() {
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
    let serializer_adapter = SerializerAdapter;
    let fuel_block_1 = FuelBlock::default();
    let mut fuel_block_2 = FuelBlock::default();
    let block_height_2 = fuel_block_1.header().height().succ().unwrap();
    fuel_block_2.header_mut().set_block_height(block_height_2);
    let block1 = serializer_adapter
        .serialize_block(&fuel_block_1)
        .expect("could not serialize block");
    let block2 = serializer_adapter
        .serialize_block(&fuel_block_2)
        .expect("could not serialize block");
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
    let expected = list;
    let actual: Vec<ProtoBlock> = response
        .into_inner()
        .try_collect::<Vec<_>>()
        .await
        .unwrap()
        .into_iter()
        .map(|b| {
            if let Some(Payload::Literal(inner)) = b.payload {
                inner
            } else {
                panic!("unexpected response type")
            }
        })
        .collect();

    assert_eq!(expected, actual);
}
#[tokio::test]
async fn await_query__get_block_range__client_receives_expected_value__remote() {
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
    let list: Vec<_> = vec!["1", "2"]
        .iter()
        .map(|height| {
            let region = "test-region".to_string();
            let bucket = "test-bucket".to_string();
            let key = height.to_string();
            let url = "good.url".to_string();
            RemoteBlockRangeResponse {
                region,
                bucket,
                key,
                url,
            }
        })
        .collect();
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
        let range = BlockRangeResponse::Remote(stream);
        response.send(range).unwrap();
    } else {
        panic!("expected GetBlockRange query");
    }
    tracing::info!("awaiting query");
    let response = handle.await.unwrap();
    let expected = list;
    let actual: Vec<RemoteBlockRangeResponse> = response
        .into_inner()
        .try_collect::<Vec<_>>()
        .await
        .unwrap()
        .into_iter()
        .map(|b| {
            if let Some(Payload::Remote(inner)) = b.payload {
                RemoteBlockRangeResponse {
                    region: inner.region,
                    bucket: inner.bucket,
                    key: inner.key,
                    url: inner.url,
                }
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
    let serializer_adapter = SerializerAdapter;
    let mut fuel_block_1 = FuelBlock::default();
    fuel_block_1.header_mut().set_block_height(height1);
    let mut fuel_block_2 = FuelBlock::default();
    fuel_block_2.header_mut().set_block_height(height2);
    let block1 = serializer_adapter
        .serialize_block(&fuel_block_1)
        .expect("could not serialize block");
    let block2 = serializer_adapter
        .serialize_block(&fuel_block_2)
        .expect("could not serialize block");
    let list = vec![block1, block2];
    if let BlockAggregatorQuery::NewBlockSubscription { response } = query {
        tracing::info!("correct query received, sending response");
        for block in list.clone() {
            response.send(block).await.unwrap();
        }
    } else {
        panic!("expected GetBlockRange query");
    }
    tracing::info!("awaiting query");
    let response = handle.await.unwrap();
    let expected = list;
    let actual: Vec<ProtoBlock> = response
        .into_inner()
        .try_collect::<Vec<_>>()
        .await
        .unwrap()
        .into_iter()
        .map(|b| {
            if let Some(Payload::Literal(inner)) = b.payload {
                inner
            } else {
                panic!("unexpected response type")
            }
        })
        .collect();

    assert_eq!(expected, actual);
}
