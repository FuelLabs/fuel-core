use chrono::{TimeZone, Utc};
use fuel_core::{
    database::Database,
    model::{FuelBlockDb, FuelBlockHeader},
    schema::scalars::BlockId,
    service::{Config, FuelService},
};
use fuel_core_interfaces::common::{fuel_storage::Storage, fuel_types};
use fuel_gql_client::client::{FuelClient, PageDirection, PaginationRequest};
use itertools::{rev, Itertools};

#[tokio::test]
async fn block() {
    // setup test data in the node
    let block = FuelBlockDb::default();
    let id = block.id();
    let mut db = Database::default();
    Storage::<fuel_types::Bytes32, FuelBlockDb>::insert(&mut db, &id, &block).unwrap();

    // setup server & client
    let srv = FuelService::from_database(db, Config::local_node())
        .await
        .unwrap();
    let client = FuelClient::from(srv.bound_address);

    // run test
    let block = client
        .block(BlockId::from(id).to_string().as_str())
        .await
        .unwrap();
    assert!(block.is_some());
}

#[tokio::test]
async fn block_connection_first_5() {
    // blocks
    let blocks = (0..10u32)
        .map(|i| FuelBlockDb {
            headers: FuelBlockHeader {
                height: i.into(),
                time: Utc.timestamp(i.into(), 0),
                ..Default::default()
            },
            transactions: vec![],
        })
        .collect_vec();

    // setup test data in the node
    let mut db = Database::default();
    for block in blocks {
        let id = block.id();
        Storage::<fuel_types::Bytes32, FuelBlockDb>::insert(&mut db, &id, &block).unwrap();
    }

    // setup server & client
    let srv = FuelService::from_database(db, Config::local_node())
        .await
        .unwrap();
    let client = FuelClient::from(srv.bound_address);

    // run test
    let blocks = client
        .blocks(PaginationRequest {
            cursor: None,
            results: 5,
            direction: PageDirection::Forward,
        })
        .await
        .unwrap();
    assert!(!blocks.results.is_empty());
    assert!(blocks.cursor.is_some());
    // assert "first" 5 blocks are returned in descending order (latest first)
    assert_eq!(
        blocks.results.into_iter().map(|b| b.height.0).collect_vec(),
        rev(0..5).collect_vec()
    );
}

#[tokio::test]
async fn block_connection_last_5() {
    // blocks
    let blocks = (0..10u32)
        .map(|i| FuelBlockDb {
            headers: FuelBlockHeader {
                height: i.into(),
                time: Utc.timestamp(i.into(), 0),
                ..Default::default()
            },
            transactions: vec![],
        })
        .collect_vec();

    // setup test data in the node
    let mut db = Database::default();
    for block in blocks {
        let id = block.id();
        Storage::<fuel_types::Bytes32, FuelBlockDb>::insert(&mut db, &id, &block).unwrap();
    }

    // setup server & client
    let srv = FuelService::from_database(db, Config::local_node())
        .await
        .unwrap();
    let client = FuelClient::from(srv.bound_address);

    // run test
    let blocks = client
        .blocks(PaginationRequest {
            cursor: None,
            results: 5,
            direction: PageDirection::Backward,
        })
        .await
        .unwrap();
    assert!(!blocks.results.is_empty());
    assert!(blocks.cursor.is_some());
    // assert "last" 5 blocks are returned in descending order (latest first)
    assert_eq!(
        blocks.results.into_iter().map(|b| b.height.0).collect_vec(),
        rev(5..10).collect_vec()
    );
}
