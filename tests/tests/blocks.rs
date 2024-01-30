use fuel_core::{
    database::Database,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_client::client::{
    pagination::{
        PageDirection,
        PaginationRequest,
    },
    types::TransactionStatus,
    FuelClient,
};
use fuel_core_poa::Trigger;
use fuel_core_storage::{
    tables::{
        FuelBlocks,
        SealedBlockConsensus,
    },
    vm_storage::VmStorageRequirements,
    StorageAsMut,
};
use fuel_core_types::{
    blockchain::{
        block::CompressedBlock,
        consensus::Consensus,
    },
    fuel_tx::*,
    fuel_types::ChainId,
    secrecy::ExposeSecret,
    tai64::Tai64,
};
use itertools::{
    rev,
    Itertools,
};
use rstest::rstest;
use std::{
    ops::Deref,
    time::Duration,
};

#[tokio::test]
async fn block() {
    // setup test data in the node
    let block = CompressedBlock::default();
    let height = block.header().height();
    let mut db = Database::default();
    // setup server & client
    let srv = FuelService::from_database(db.clone(), Config::local_node())
        .await
        .unwrap();
    let client = FuelClient::from(srv.bound_address);

    db.storage::<FuelBlocks>().insert(height, &block).unwrap();
    db.storage::<SealedBlockConsensus>()
        .insert(height, &Consensus::PoA(Default::default()))
        .unwrap();

    // run test
    let block = client.block_by_height(**height).await.unwrap();
    assert!(block.is_some());
}

#[tokio::test]
async fn get_genesis_block() {
    let mut config = Config::local_node();
    config.chain_conf.initial_state.as_mut().unwrap().height = Some(13u32.into());
    let srv = FuelService::from_database(Database::default(), config)
        .await
        .unwrap();

    let client = FuelClient::from(srv.bound_address);
    let tx = Transaction::default_test_tx();
    client.submit_and_await_commit(&tx).await.unwrap();

    let block = client.block_by_height(13).await.unwrap().unwrap();
    assert_eq!(block.header.height, 13);
    assert!(matches!(
        block.consensus,
        fuel_core_client::client::types::Consensus::Genesis(_)
    ));
}

#[tokio::test]
async fn produce_block() {
    let config = Config::local_node();

    let srv = FuelService::from_database(Database::default(), config.clone())
        .await
        .unwrap();

    let client = FuelClient::from(srv.bound_address);

    let tx = Transaction::default_test_tx();
    client.submit_and_await_commit(&tx).await.unwrap();

    let transaction_response = client
        .transaction(&tx.id(&ChainId::default()))
        .await
        .unwrap();

    if let TransactionStatus::Success { block_id, .. } =
        transaction_response.unwrap().status
    {
        let block_id = block_id.parse().unwrap();
        let block = client.block(&block_id).await.unwrap().unwrap();
        let actual_pub_key = block.block_producer().unwrap();
        let block_height: u32 = block.header.height;
        let expected_pub_key = config
            .consensus_key
            .unwrap()
            .expose_secret()
            .deref()
            .public_key();

        assert!(1 == block_height);
        assert_eq!(*actual_pub_key, expected_pub_key);
    } else {
        panic!("Wrong tx status");
    };
}

#[tokio::test]
async fn produce_block_manually() {
    let db = Database::default();

    let config = Config::local_node();

    let srv = FuelService::from_database(db, config.clone())
        .await
        .unwrap();

    let client = FuelClient::from(srv.bound_address);

    let new_height = client.produce_blocks(1, None).await.unwrap();

    assert_eq!(1, *new_height);
    let block = client.block_by_height(1).await.unwrap().unwrap();
    assert_eq!(block.header.height, 1);
    let actual_pub_key = block.block_producer().unwrap();
    let expected_pub_key = config
        .consensus_key
        .unwrap()
        .expose_secret()
        .deref()
        .public_key();
    assert_eq!(*actual_pub_key, expected_pub_key);
}

#[tokio::test]
async fn produce_block_negative() {
    let db = Database::default();

    let config = Config {
        debug: false,
        ..Config::local_node()
    };
    let srv = FuelService::from_database(db, config).await.unwrap();

    let client = FuelClient::from(srv.bound_address);

    let new_height = client.produce_blocks(5, None).await;

    assert_eq!(
        "Response errors; `debug` must be enabled to use this endpoint",
        new_height.err().unwrap().to_string()
    );
}

#[tokio::test]
async fn produce_block_custom_time() {
    let db = Database::default();

    let mut config = Config::local_node();
    config.block_production = Trigger::Interval {
        block_time: Duration::from_secs(10),
    };

    let srv = FuelService::from_database(db.clone(), config)
        .await
        .unwrap();

    let client = FuelClient::from(srv.bound_address);
    let start_timestamp = Tai64::UNIX_EPOCH.0 + 100u64;
    let new_height = client
        .produce_blocks(5, Some(start_timestamp))
        .await
        .unwrap();

    assert_eq!(5, *new_height);

    assert_eq!(db.block_time(&1u32.into()).unwrap().0, start_timestamp);
    assert_eq!(db.block_time(&2u32.into()).unwrap().0, start_timestamp + 10);
    assert_eq!(db.block_time(&3u32.into()).unwrap().0, start_timestamp + 20);
    assert_eq!(db.block_time(&4u32.into()).unwrap().0, start_timestamp + 30);
    assert_eq!(db.block_time(&5u32.into()).unwrap().0, start_timestamp + 40);
}

#[tokio::test]
async fn produce_block_bad_start_time() {
    let db = Database::default();

    let config = Config::local_node();

    let srv = FuelService::from_database(db.clone(), config)
        .await
        .unwrap();

    let client = FuelClient::from(srv.bound_address);

    // produce block with current timestamp
    let _ = client.produce_blocks(1, None).await.unwrap();

    // try producing block with an earlier timestamp
    let err = client
        .produce_blocks(1, Some(100u64))
        .await
        .expect_err("Completed unexpectedly");
    assert!(err.to_string().starts_with(
        "Response errors; The block timestamp should monotonically increase"
    ));
}

#[tokio::test]
async fn produce_block_overflow_time() {
    let db = Database::default();

    let mut config = Config::local_node();

    config.block_production = Trigger::Interval {
        block_time: Duration::from_secs(10),
    };

    let srv = FuelService::from_database(db.clone(), config)
        .await
        .unwrap();

    let client = FuelClient::from(srv.bound_address);

    let err = client
        .produce_blocks(2, Some(u64::MAX))
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
    let config = Config::local_node();

    // setup server & client
    let srv = FuelService::from_database(Default::default(), config)
        .await
        .unwrap();
    let client = FuelClient::from(srv.bound_address);
    // setup test data in the node
    client.produce_blocks(9, None).await.unwrap();

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
                    .map(|b| b.header.height)
                    .collect_vec(),
                (0..5).collect_vec()
            );
        }
        PageDirection::Backward => {
            assert_eq!(
                blocks
                    .results
                    .into_iter()
                    .map(|b| b.header.height)
                    .collect_vec(),
                rev(5..10).collect_vec()
            );
        }
    };
}

mod full_block {
    use super::*;
    use cynic::QueryBuilder;
    use fuel_core_client::client::{
        schema::{
            block::{
                BlockByHeightArgs,
                Consensus,
                Header,
            },
            schema,
            tx::OpaqueTransaction,
            BlockId,
            U32,
        },
        FuelClient,
    };

    #[derive(cynic::QueryFragment, Debug)]
    #[cynic(
        schema_path = "../crates/client/assets/schema.sdl",
        graphql_type = "Query",
        variables = "BlockByHeightArgs"
    )]
    pub struct FullBlockByHeightQuery {
        #[arguments(height: $height)]
        pub block: Option<FullBlock>,
    }

    #[derive(cynic::QueryFragment, Debug)]
    #[cynic(
        schema_path = "../crates/client/assets/schema.sdl",
        graphql_type = "Block"
    )]
    pub struct FullBlock {
        pub id: BlockId,
        pub header: Header,
        pub consensus: Consensus,
        pub transactions: Vec<OpaqueTransaction>,
    }

    #[async_trait::async_trait]
    pub trait ClientExt {
        async fn full_block_by_height(
            &self,
            height: u32,
        ) -> std::io::Result<Option<FullBlock>>;
    }

    #[async_trait::async_trait]
    impl ClientExt for FuelClient {
        async fn full_block_by_height(
            &self,
            height: u32,
        ) -> std::io::Result<Option<FullBlock>> {
            let query = FullBlockByHeightQuery::build(BlockByHeightArgs {
                height: Some(U32(height)),
            });

            let block = self.query(query).await?.block;

            Ok(block)
        }
    }

    #[tokio::test]
    async fn get_full_block_with_tx() {
        let srv = FuelService::from_database(Database::default(), Config::local_node())
            .await
            .unwrap();

        let client = FuelClient::from(srv.bound_address);
        let tx = Transaction::default_test_tx();
        client.submit_and_await_commit(&tx).await.unwrap();

        let block = client.full_block_by_height(1).await.unwrap().unwrap();
        assert_eq!(block.header.height.0, 1);
        assert_eq!(block.transactions.len(), 2 /* mint + our tx */);
    }
}
