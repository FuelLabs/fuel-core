#![allow(non_snake_case)]

use crate::{
    api::protobuf_adapter::{
        MockBlocksAggregatorApi,
        new_service,
    },
    block_range_response::{
        BlockRangeResponse,
        RemoteS3Response,
    },
    blocks::old_block_source::{
        BlockConvector,
        convertor_adapter::ConvertorAdapter,
    },
    protobuf_types::{
        Block as ProtoBlock,
        BlockHeightRequest,
        BlockRangeRequest,
        NewBlockSubscriptionRequest,
        block_aggregator_client::BlockAggregatorClient as ProtoBlockAggregatorClient,
        block_response::Payload,
    },
};
use fuel_core_protobuf::remote_block_response::Location;
use fuel_core_services::Service;
use fuel_core_types::{
    blockchain::block::Block as FuelBlock,
    fuel_types::BlockHeight,
};
use futures::{
    StreamExt,
    TryStreamExt,
};
use std::{
    net::SocketAddr,
    sync::Arc,
};
use tokio::sync::broadcast;

fn free_local_addr() -> SocketAddr {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap()
}

#[tokio::test]
async fn await_query__get_current_height__client_receives_expected_value() {
    let socket = free_local_addr();
    let mut api = MockBlocksAggregatorApi::default();

    // Given
    api.expect_get_current_height()
        .times(1)
        .returning(|| Ok(Some(BlockHeight::new(42))));

    let service = new_service(socket, api);
    service.start_and_await().await.unwrap();

    // call get current height endpoint with client
    let url = format!("http://{}", socket);
    let mut client = ProtoBlockAggregatorClient::connect(url.to_string())
        .await
        .expect("could not connect to server");

    // When
    let result = client.get_synced_block_height(BlockHeightRequest {}).await;

    // Then
    let value = result.expect("could not get height");

    // assert client received expected value
    assert_eq!(value.into_inner().height, Some(42));
}

#[tokio::test]
async fn await_query__get_block_range__client_receives_expected_value__literal() {
    let socket = free_local_addr();
    let mut api = MockBlocksAggregatorApi::default();

    // Given
    let convertor_adapter = ConvertorAdapter;
    let fuel_block_1 = FuelBlock::default();
    let mut fuel_block_2 = FuelBlock::default();
    let block_height_1 = fuel_block_1.header().height();
    let block_height_2 = block_height_1.succ().unwrap();
    fuel_block_2.header_mut().set_block_height(block_height_2);
    let block1 = convertor_adapter
        .convert_block(&fuel_block_1, &[])
        .expect("could not serialize block");
    let block2 = convertor_adapter
        .convert_block(&fuel_block_2, &[])
        .expect("could not serialize block");
    let list = vec![(*block_height_1, block1), (block_height_2, block2)];
    let expected = list.clone();
    api.expect_get_block_range()
        .times(1)
        .returning(move |_: u32, _: u32| {
            let response = BlockRangeResponse::Literal(
                futures::stream::iter(list.clone().into_iter()).boxed(),
            );
            Ok(response)
        });

    let service = new_service(socket, api);
    service.start_and_await().await.unwrap();

    // call get current height endpoint with client
    let url = format!("http://{}", socket);
    let mut client = ProtoBlockAggregatorClient::connect(url.to_string())
        .await
        .expect("could not connect to server");
    let request = BlockRangeRequest { start: 0, end: 1 };

    // When
    let result = client.get_block_range(request).await;

    // Then
    let response = result.unwrap();
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
    let socket = free_local_addr();
    let mut api = MockBlocksAggregatorApi::default();

    // Given
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
    let expected = list.clone();
    api.expect_get_block_range()
        .times(1)
        .returning(move |_: u32, _: u32| {
            let response = BlockRangeResponse::S3(
                futures::stream::iter(list.clone().into_iter()).boxed(),
            );
            Ok(response)
        });

    let service = new_service(socket, api);
    service.start_and_await().await.unwrap();

    // call get current height endpoint with client
    let url = format!("http://{}", socket);
    let mut client = ProtoBlockAggregatorClient::connect(url.to_string())
        .await
        .expect("could not connect to server");
    let request = BlockRangeRequest { start: 0, end: 1 };

    // When
    let result = client.get_block_range(request).await;

    // Then
    let response = result.unwrap();
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
    let socket = free_local_addr();
    let mut api = MockBlocksAggregatorApi::default();

    let (sender, receiver) = broadcast::channel(100);

    // Given
    let convertor_adapter = ConvertorAdapter;
    let fuel_block_1 = FuelBlock::default();
    let mut fuel_block_2 = FuelBlock::default();
    let block_height_1 = fuel_block_1.header().height();
    let block_height_2 = block_height_1.succ().unwrap();
    fuel_block_2.header_mut().set_block_height(block_height_2);
    let block1 = convertor_adapter
        .convert_block(&fuel_block_1, &[])
        .expect("could not serialize block");
    let block2 = convertor_adapter
        .convert_block(&fuel_block_2, &[])
        .expect("could not serialize block");
    let list = vec![(*block_height_1, block1), (block_height_2, block2)];

    api.expect_new_block_subscription()
        .times(1)
        .returning(move || {
            let stream =
                tokio_stream::wrappers::BroadcastStream::new(receiver.resubscribe())
                    .map(|result| result.map_err(|err| anyhow::anyhow!(err)));
            stream.boxed()
        });

    let service = new_service(socket, api);
    service.start_and_await().await.unwrap();

    // call get current height endpoint with client
    let url = format!("http://{}", socket);
    let mut client = ProtoBlockAggregatorClient::connect(url.to_string())
        .await
        .expect("could not connect to server");
    let request = NewBlockSubscriptionRequest {};

    // When
    let result = client.new_block_subscription(request).await;
    // Send blocks
    for (height, block) in list.clone() {
        sender.send((height, Arc::new(block))).unwrap();
    }

    // Then
    let stream = result.unwrap().into_inner();

    let actual: Vec<(BlockHeight, ProtoBlock)> = stream
        .take(2)
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

    assert_eq!(list, actual);
}
