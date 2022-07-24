use chrono::{TimeZone, Utc};
use fuel_core::{
    config::Config,
    database::Database,
    model::{FuelBlockDb, FuelBlockHeader},
    schema::scalars::BlockId,
    service::FuelService,
};
use fuel_core_interfaces::common::fuel_tx;
use fuel_core_interfaces::common::{fuel_storage::Storage, fuel_types};
use fuel_gql_client::client::types::TransactionStatus;
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
async fn produce_block() {
    let db = Database::default();

    let mut config = Config::local_node();

    config.manual_blocks_enabled = true;

    let srv = FuelService::from_database(db, config).await.unwrap();

    let client = FuelClient::from(srv.bound_address);

    let new_height = client.produce_blocks(5).await.unwrap();

    assert_eq!(5, new_height);

    let tx = fuel_tx::Transaction::default();

    client.submit(&tx).await.unwrap();

    let transaction_response = client
        .transaction(&format!("{:#x}", tx.id()))
        .await
        .unwrap();

    if let TransactionStatus::Success { block_id, .. } = transaction_response.unwrap().status {
        let block_height: u64 = client
            .block(block_id.to_string().as_str())
            .await
            .unwrap()
            .unwrap()
            .height
            .into();

        // Block height is now 6 after being advance 5
        assert!(6 == block_height);
    } else {
        panic!("Wrong tx status");
    };
}

#[tokio::test]
async fn produce_block_negative() {
    let db = Database::default();

    let srv = FuelService::from_database(db, Config::local_node())
        .await
        .unwrap();

    let client = FuelClient::from(srv.bound_address);

    let new_height = client.produce_blocks(5).await;

    assert_eq!(
        "Response errors; Manual Blocks must be enabled to use this endpoint",
        new_height.err().unwrap().to_string()
    );

    let tx = fuel_tx::Transaction::default();

    client.submit(&tx).await.unwrap();

    let transaction_response = client
        .transaction(&format!("{:#x}", tx.id()))
        .await
        .unwrap();

    if let TransactionStatus::Success { block_id, .. } = transaction_response.unwrap().status {
        let block_height: u64 = client
            .block(block_id.to_string().as_str())
            .await
            .unwrap()
            .unwrap()
            .height
            .into();

        // Block height is now 6 after being advance 5
        assert!(1 == block_height);
    } else {
        panic!("Wrong tx status");
    };
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
