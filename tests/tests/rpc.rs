#![allow(non_snake_case)]

use fuel_core::{
    chain_config::{
        LastBlockConfig,
        StateConfig,
    },
    database::Database,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_client::client::{
    FuelClient,
    pagination::{
        PageDirection,
        PaginationRequest,
    },
    types::TransactionStatus,
};
use fuel_core_poa::Trigger;
use fuel_core_storage::{
    StorageAsMut,
    tables::{
        FuelBlocks,
        SealedBlockConsensus,
    },
    transactional::WriteTransaction,
    vm_storage::VmStorageRequirements,
};
use fuel_core_types::{
    blockchain::{
        block::CompressedBlock,
        consensus::Consensus,
    },
    fuel_tx::*,
    secrecy::ExposeSecret,
    signer::SignMode,
    tai64::Tai64,
};
use futures::StreamExt;
use itertools::{
    Itertools,
    rev,
};
use rstest::rstest;
use std::{
    ops::Deref,
    time::Duration,
};
use test_helpers::send_graph_ql_query;

use rand::{
    SeedableRng,
    rngs::StdRng,
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
#[tokio::test]
async fn get_block_range__can_get_serialized_block_from_rpc() {
    let config = Config::local_node();

    let srv = FuelService::from_database(Database::default(), config.clone())
        .await
        .unwrap();

    let graphql_client = FuelClient::from(srv.bound_address);

    let tx = Transaction::default_test_tx();
    let status = graphql_client.submit_and_await_commit(&tx).await.unwrap();

    let mut rpc_client = BlockAggregatorClient::connect(url.to_string())
        .await
        .expect("could not connect to server");
}
