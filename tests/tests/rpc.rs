#![allow(non_snake_case)]

use crate::blocks::full_block::ClientExt;
use fuel_block_aggregator_api::api::protobuf_adapter::block_aggregator_client::BlockAggregatorClient;
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
use std::net::{
    SocketAddr,
    TcpListener,
};

//     // given
//     let path = free_local_addr();
//     let mut api = ProtobufAPI::new(path.to_string());
//     tokio::time::sleep(std::time::Duration::from_millis(100)).await;
//
//     // call get current height endpoint with client
//     let url = format!("http://{}", path);
//     let mut client = BlockAggregatorClient::connect(url.to_string())
//         .await
//         .expect("could not connect to server");
//     let request = BlockRangeRequest { start: 0, end: 1 };
//     let handle = tokio::spawn(async move {
//         tracing::info!("querying with client");
//         client
//             .get_block_range(request)
//             .await
//             .expect("could not get height")
//     });
//
//     // when
//     tracing::info!("awaiting query");
//     let query = api.await_query().await.unwrap();
//
//     // then
//     let block1 = Block::new(Bytes::from(vec![0u8; 100]));
//     let block2 = Block::new(Bytes::from(vec![1u8; 100]));
//     let list = vec![block1, block2];
//     // return response through query's channel
//     if let BlockAggregatorQuery::GetBlockRange {
//         first,
//         last,
//         response,
//     } = query
//     {
//         assert_eq!(first, BlockHeight::new(0));
//         assert_eq!(last, BlockHeight::new(1));
//         tracing::info!("correct query received, sending response");
//         let stream = tokio_stream::iter(list.clone()).boxed();
//         let range = BlockRangeResponse::Literal(stream);
//         response.send(range).unwrap();
//     } else {
//         panic!("expected GetBlockRange query");
//     }
//     tracing::info!("awaiting query");
//     let response = handle.await.unwrap();
//     let expected: Vec<Vec<u8>> = list.iter().map(|b| b.bytes().to_vec()).collect();
//     let actual: Vec<Vec<u8>> = response
//         .into_inner()
//         .try_collect::<Vec<_>>()
//         .await
//         .unwrap()
//         .into_iter()
//         .map(|b| b.data.to_vec())
//         .collect();
//
//     assert_eq!(expected, actual);

fn free_local_addr() -> SocketAddr {
    let listener = TcpListener::bind("[::1]:0").unwrap();
    listener.local_addr().unwrap() // OS picks a free port
}

#[tokio::test]
async fn get_block_range__can_get_serialized_block_from_rpc() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::ERROR)
        .try_init();
    let mut config = Config::local_node();
    config.rpc_config.addr = free_local_addr();
    let rpc_url = config.rpc_config.addr.clone();

    let srv = FuelService::from_database(Database::default(), config.clone())
        .await
        .unwrap();

    tracing::error!("starting graphql client");
    let graphql_client = FuelClient::from(srv.bound_address);

    let tx = Transaction::default_test_tx();
    tracing::error!("submitting transaction to create block");
    let _ = graphql_client.submit_and_await_commit(&tx).await.unwrap();

    let mut rpc_client = BlockAggregatorClient::connect(rpc_url.to_string())
        .await
        .expect("could not connect to server");

    tracing::error!("fetching expected block from graphql");
    let expected_block = graphql_client
        .full_block_by_height(1)
        .await
        .unwrap()
        .unwrap();
    let header = expected_block.header;

    // when
    let request = fuel_block_aggregator_api::api::protobuf_adapter::BlockRangeRequest {
        start: 1,
        end: 1,
    };
    tracing::error!("sending request: {:?}", request);
    let actual_bytes = rpc_client
        .get_block_range(request.clone())
        .await
        .unwrap()
        .into_inner()
        .next()
        .await
        .unwrap()
        .unwrap()
        .data;
    let actual_block: Block<Transaction> = postcard::from_bytes(&actual_bytes).unwrap();

    // then
    assert_eq!(
        *actual_block.header().height(),
        BlockHeight::from(header.height.0)
    );
}
