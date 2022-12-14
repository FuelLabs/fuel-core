use fuel_core::{
    database::Database,
    model::{
        FuelBlockDb,
        FuelBlockHeader,
    },
    schema::scalars::BlockId,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_interfaces::{
    common::{
        fuel_storage::StorageAsMut,
        fuel_tx,
        fuel_tx::UniqueIdentifier,
        secrecy::ExposeSecret,
        tai64::Tai64,
    },
    db::{
        FuelBlocks,
        SealedBlockConsensus,
    },
    model::{
        FuelBlockConsensus,
        FuelConsensusHeader,
    },
};
use fuel_gql_client::{
    client::{
        schema::{
            block::{
                Consensus,
                TimeParameters,
            },
            U64,
        },
        types::TransactionStatus,
        FuelClient,
        PageDirection,
        PaginationRequest,
    },
    prelude::Bytes32,
};
use itertools::{
    rev,
    Itertools,
};
use rstest::rstest;
use std::ops::Deref;

#[tokio::test]
async fn block() {
    // setup test data in the node
    let block = FuelBlockDb::default();
    let id = block.id();
    let mut db = Database::default();
    db.storage::<FuelBlocks>()
        .insert(&id.into(), &block)
        .unwrap();
    db.storage::<SealedBlockConsensus>()
        .insert(&id.into(), &FuelBlockConsensus::PoA(Default::default()))
        .unwrap();

    // setup server & client
    let srv = FuelService::from_database(db, Config::local_node())
        .await
        .unwrap();
    let client = FuelClient::from(srv.bound_address);

    // run test
    let id_bytes: Bytes32 = id.into();
    let block = client
        .block(BlockId::from(id_bytes).to_string().as_str())
        .await
        .unwrap();
    assert!(block.is_some());
}

#[tokio::test]
async fn get_genesis_block() {
    let srv = FuelService::from_database(Database::default(), Config::local_node())
        .await
        .unwrap();

    let client = FuelClient::from(srv.bound_address);
    let tx = fuel_tx::Transaction::default();
    client.submit_and_await_commit(&tx).await.unwrap();

    let block = client.block_by_height(0).await.unwrap().unwrap();
    assert_eq!(block.header.height.0, 0);
    assert!(matches!(block.consensus, Consensus::Genesis(_)));
}

#[tokio::test]
async fn produce_block() {
    let config = Config::local_node();
    let srv = FuelService::from_database(Database::default(), config.clone())
        .await
        .unwrap();

    let client = FuelClient::from(srv.bound_address);

    let tx = fuel_tx::Transaction::default();
    client.submit_and_await_commit(&tx).await.unwrap();

    let transaction_response = client
        .transaction(&format!("{:#x}", tx.id()))
        .await
        .unwrap();

    if let TransactionStatus::Success { block_id, .. } =
        transaction_response.unwrap().status
    {
        let block = client
            .block(block_id.to_string().as_str())
            .await
            .unwrap()
            .unwrap();
        let actual_pub_key = block.block_producer().unwrap();
        let block_height: u64 = block.header.height.into();
        let expected_pub_key = config
            .consensus_key
            .unwrap()
            .expose_secret()
            .deref()
            .public_key();

        assert!(1 == block_height);
        assert_eq!(actual_pub_key, expected_pub_key);
    } else {
        panic!("Wrong tx status");
    };
}

#[tokio::test]
async fn produce_block_manually() {
    let db = Database::default();

    let mut config = Config::local_node();

    config.manual_blocks_enabled = true;

    let srv = FuelService::from_database(db, config.clone())
        .await
        .unwrap();

    let client = FuelClient::from(srv.bound_address);

    let new_height = client.produce_blocks(1, None).await.unwrap();

    assert_eq!(1, new_height);
    let block = client.block_by_height(1).await.unwrap().unwrap();
    assert_eq!(block.header.height.0, 1);
    let actual_pub_key = block.block_producer().unwrap();
    let expected_pub_key = config
        .consensus_key
        .unwrap()
        .expose_secret()
        .deref()
        .public_key();
    assert_eq!(actual_pub_key, expected_pub_key);
}

#[tokio::test]
async fn produce_block_negative() {
    let db = Database::default();

    let srv = FuelService::from_database(db, Config::local_node())
        .await
        .unwrap();

    let client = FuelClient::from(srv.bound_address);

    let new_height = client.produce_blocks(5, None).await;

    assert_eq!(
        "Response errors; Manual Blocks must be enabled to use this endpoint",
        new_height.err().unwrap().to_string()
    );

    let tx = fuel_tx::Transaction::default();
    client.submit_and_await_commit(&tx).await.unwrap();

    let transaction_response = client
        .transaction(&format!("{:#x}", tx.id()))
        .await
        .unwrap();

    if let TransactionStatus::Success { block_id, .. } =
        transaction_response.unwrap().status
    {
        let block_height: u64 = client
            .block(block_id.to_string().as_str())
            .await
            .unwrap()
            .unwrap()
            .header
            .height
            .into();

        // Block height is now 6 after being advance 5
        assert!(1 == block_height);
    } else {
        panic!("Wrong tx status");
    };
}

#[tokio::test]
async fn produce_block_custom_time() {
    let db = Database::default();

    let mut config = Config::local_node();

    config.manual_blocks_enabled = true;

    let srv = FuelService::from_database(db.clone(), config)
        .await
        .unwrap();

    let client = FuelClient::from(srv.bound_address);

    let time = TimeParameters {
        start_time: U64::from(100u64),
        block_time_interval: U64::from(10u64),
    };
    let new_height = client.produce_blocks(5, Some(time)).await.unwrap();

    assert_eq!(5, new_height);

    assert_eq!(db.block_time(1).unwrap().0, 100);
    assert_eq!(db.block_time(2).unwrap().0, 110);
    assert_eq!(db.block_time(3).unwrap().0, 120);
    assert_eq!(db.block_time(4).unwrap().0, 130);
    assert_eq!(db.block_time(5).unwrap().0, 140);
}

#[tokio::test]
async fn produce_block_bad_start_time() {
    let db = Database::default();

    let mut config = Config::local_node();

    config.manual_blocks_enabled = true;

    let srv = FuelService::from_database(db.clone(), config)
        .await
        .unwrap();

    let client = FuelClient::from(srv.bound_address);

    // produce block with current timestamp
    let _ = client.produce_blocks(1, None).await.unwrap();

    // try producing block with an ealier timestamp
    let time = TimeParameters {
        start_time: U64::from(100u64),
        block_time_interval: U64::from(10u64),
    };
    let err = client
        .produce_blocks(1, Some(time))
        .await
        .expect_err("Completed unexpectedly");
    assert!(err.to_string().starts_with(
        "Response errors; The start time must be set after the latest block time"
    ));
}

#[tokio::test]
async fn produce_block_overflow_time() {
    let db = Database::default();

    let mut config = Config::local_node();

    config.manual_blocks_enabled = true;

    let srv = FuelService::from_database(db.clone(), config)
        .await
        .unwrap();

    let client = FuelClient::from(srv.bound_address);

    // produce block with current timestamp
    let _ = client.produce_blocks(1, None).await.unwrap();

    // try producing block with an ealier timestamp
    let time = TimeParameters {
        start_time: U64::from(u64::MAX),
        block_time_interval: U64::from(1u64),
    };
    let err = client
        .produce_blocks(1, Some(time))
        .await
        .expect_err("Completed unexpectedly");
    assert!(err.to_string().starts_with(
        "Response errors; The provided time parameters lead to an overflow"
    ));
}

#[rstest]
#[tokio::test]
async fn block_connection_5(
    #[values(PageDirection::Forward, PageDirection::Backward)]
    pagination_direction: PageDirection,
) {
    // blocks
    let blocks = (0..10u32)
        .map(|i| FuelBlockDb {
            header: FuelBlockHeader {
                consensus: FuelConsensusHeader {
                    height: i.into(),
                    time: Tai64(i.into()),
                    ..Default::default()
                },
                ..Default::default()
            },
            transactions: vec![],
        })
        .collect_vec();

    // setup test data in the node
    let mut db = Database::default();
    for block in blocks {
        let id = block.id();
        db.storage::<FuelBlocks>()
            .insert(&id.into(), &block)
            .unwrap();
        db.storage::<SealedBlockConsensus>()
            .insert(&id.into(), &FuelBlockConsensus::PoA(Default::default()))
            .unwrap();
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
            direction: pagination_direction,
        })
        .await
        .unwrap();

    assert!(!blocks.results.is_empty());
    assert!(blocks.cursor.is_some());

    // Blocks are typically requested in descending order (latest
    // first), but we're returning them in ascending order to keep
    // this query in line with the GraphQL API specs and other queries.
    match pagination_direction {
        PageDirection::Forward => {
            assert_eq!(
                blocks
                    .results
                    .into_iter()
                    .map(|b| b.header.height.0)
                    .collect_vec(),
                (0..5).collect_vec()
            );
        }
        PageDirection::Backward => {
            assert_eq!(
                blocks
                    .results
                    .into_iter()
                    .map(|b| b.header.height.0)
                    .collect_vec(),
                rev(5..10).collect_vec()
            );
        }
    };
}
