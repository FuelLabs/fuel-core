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

fn free_local_addr() -> SocketAddr {
    let listener = TcpListener::bind("[::1]:0").unwrap();
    listener.local_addr().unwrap() // OS picks a free port
}

#[tokio::test(flavor = "multi_thread")]
async fn get_block_range__can_get_serialized_block_from_rpc() {
    // let _ = tracing_subscriber::fmt()
    //     .with_max_level(tracing::Level::ERROR)
    //     .try_init();
    let mut config = Config::local_node();
    config.rpc_config.addr = free_local_addr();
    let rpc_url = config.rpc_config.addr.clone();

    let srv = FuelService::from_database(Database::default(), config.clone())
        .await
        .unwrap();

    let graphql_client = FuelClient::from(srv.bound_address);

    let tx = Transaction::default_test_tx();
    let _ = graphql_client.submit_and_await_commit(&tx).await.unwrap();

    let rpc_url = format!("http://{}", rpc_url);
    let mut rpc_client = BlockAggregatorClient::connect(rpc_url)
        .await
        .expect("could not connect to server");

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
        BlockHeight::from(header.height.0),
        *actual_block.header().height()
    );
    // check txs
    assert_eq!(2, actual_block.transactions().len());
}
