#![allow(non_snake_case)]

use crate::{
    api::{
        BlockAggregatorApi,
        BlockAggregatorQuery,
        protobuf_adapter::ProtobufAPI,
    },
    block_range_response::{
        BlockRangeResponse,
        RemoteS3Response,
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
use fuel_core_protobuf::remote_block_response::Location;
use fuel_core_types::{
    blockchain::block::Block as FuelBlock,
    fuel_types::BlockHeight,
};
use futures::{
    StreamExt,
    TryStreamExt,
};

fn free_local_addr() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    format!("127.0.0.1:{}", addr.port())
}

#[tokio::test]
async fn await_query__get_current_height__client_receives_expected_value() {
    // given
    let path = free_local_addr();
    let mut api = ProtobufAPI::new(path.to_string(), 100).unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // call get current height endpoint with client
    let url = format!("http://{}", path);
    let mut client = ProtoBlockAggregatorClient::connect(url.to_string())
        .await
        .expect("could not connect to server");
    let handle = tokio::spawn(async move {
        tracing::info!("querying with client");
        client
            .get_synced_block_height(BlockHeightRequest {})
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
    let mut api = ProtobufAPI::new(path.to_string(), 100).unwrap();
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
    let block_height_1 = fuel_block_1.header().height();
    let block_height_2 = block_height_1.succ().unwrap();
    fuel_block_2.header_mut().set_block_height(block_height_2);
    let block1 = serializer_adapter
        .serialize_block(&fuel_block_1, &[])
        .expect("could not serialize block");
    let block2 = serializer_adapter
        .serialize_block(&fuel_block_2, &[])
        .expect("could not serialize block");
    let list = vec![(*block_height_1, block1), (block_height_2, block2)];
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
    let actual: Vec<(BlockHeight, ProtoBlock)> = response
        .into_inner()
        .try_collect::<Vec<_>>()
        .await
        .unwrap()
        .into_iter()
        .map(|b| {
            if let Some(Payload::Literal(inner)) = b.payload {
                (BlockHeight::new(b.height), inner)
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
    let mut api = ProtobufAPI::new(path.to_string(), 100).unwrap();
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
    let list: Vec<_> = [(BlockHeight::new(1), "1"), (BlockHeight::new(2), "2")]
        .iter()
        .map(|(height, key)| {
            let bucket = "test-bucket".to_string();
            let key = key.to_string();
            let res = RemoteS3Response {
                bucket,
                key,
                requester_pays: false,
                aws_endpoint: None,
            };
            (*height, res)
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
        let range = BlockRangeResponse::S3(stream);
        response.send(range).unwrap();
    } else {
        panic!("expected GetBlockRange query");
    }
    tracing::info!("awaiting query");
    let response = handle.await.unwrap();
    let expected = list;
    let actual: Vec<(BlockHeight, RemoteS3Response)> = response
        .into_inner()
        .try_collect::<Vec<_>>()
        .await
        .unwrap()
        .into_iter()
        .map(|b| {
            if let Some(Payload::Remote(inner)) = b.payload {
                let height = BlockHeight::new(b.height);
                let location = inner.location.unwrap();
                let Location::S3(s3) = location else {
                    panic!("unexpected location type")
                };
                let res = RemoteS3Response {
                    bucket: s3.bucket,
                    key: s3.key,
                    requester_pays: false,
                    aws_endpoint: None,
                };
                (height, res)
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
    let mut api = ProtobufAPI::new(path.to_string(), 100).unwrap();
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
        .serialize_block(&fuel_block_1, &[])
        .expect("could not serialize block");
    let block2 = serializer_adapter
        .serialize_block(&fuel_block_2, &[])
        .expect("could not serialize block");
    let list = vec![(height1, block1), (height2, block2)];
    if let BlockAggregatorQuery::NewBlockSubscription { response } = query {
        tracing::info!("correct query received, sending response");
        for (height, block) in list.clone() {
            response.send((height, block)).await.unwrap();
        }
    } else {
        panic!("expected GetBlockRange query");
    }
    tracing::info!("awaiting query");
    let response = handle.await.unwrap();
    let expected = list;
    let actual: Vec<(BlockHeight, ProtoBlock)> = response
        .into_inner()
        .try_collect::<Vec<_>>()
        .await
        .unwrap()
        .into_iter()
        .map(|b| {
            if let Some(Payload::Literal(inner)) = b.payload {
                (BlockHeight::new(b.height), inner)
            } else {
                panic!("unexpected response type")
            }
        })
        .collect();

    assert_eq!(expected, actual);
}
