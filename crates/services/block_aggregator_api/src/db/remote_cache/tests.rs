use super::*;
use crate::{
    block_range_response::RemoteS3Response,
    blocks::importer_and_db_source::{
        BlockSerializer,
        serializer_adapter::SerializerAdapter,
    },
    db::table::Column,
};
use aws_sdk_s3::operation::put_object::PutObjectOutput;
use aws_smithy_mocks::{
    Rule,
    mock,
    mock_client,
};
use fuel_core_storage::{
    structured_storage::test::InMemoryStorage,
    transactional::{
        IntoTransaction,
        StorageTransaction,
    },
};
use fuel_core_types::blockchain::block::Block as FuelBlock;
use futures::StreamExt;
use std::iter;

fn database() -> StorageTransaction<InMemoryStorage<Column>> {
    InMemoryStorage::default().into_transaction()
}

fn arb_proto_block() -> ProtoBlock {
    let block = FuelBlock::default();
    let serializer = SerializerAdapter;
    serializer.serialize_block(&block).unwrap()
}
fn put_happy_rule() -> Rule {
    mock!(Client::put_object)
        .match_requests(|req| req.bucket() == Some("test-bucket"))
        .sequence()
        .output(|| PutObjectOutput::builder().build())
        .build()
}

#[tokio::test]
async fn store_block__happy_path() {
    // given
    let client = mock_client!(aws_sdk_s3, [&put_happy_rule()]);
    let aws_bucket = "test-bucket".to_string();
    let storage = database();
    let sync_from = BlockHeight::new(0);
    let mut adapter =
        RemoteCache::new(aws_bucket, false, None, client, storage, sync_from);
    let block_height = BlockHeight::new(123);
    let block = arb_proto_block();
    let block = BlockSourceEvent::OldBlock(block_height, block);

    // when
    let res = adapter.store_block(block).await;

    // then
    assert!(res.is_ok());
}

#[tokio::test]
async fn get_block_range__happy_path() {
    // given
    let client = mock_client!(aws_sdk_s3, []);
    let aws_bucket = "test-bucket".to_string();
    let storage = database();
    let sync_from = BlockHeight::new(0);
    let adapter =
        RemoteCache::new(aws_bucket.clone(), false, None, client, storage, sync_from);
    let start = BlockHeight::new(999);
    let end = BlockHeight::new(1003);

    // when
    let addresses = adapter.get_block_range(start, end).await.unwrap();

    // then
    let actual = match addresses {
        BlockRangeResponse::Literal(_) => {
            panic!("Expected remote response, got literal");
        }
        BlockRangeResponse::S3(stream) => stream.collect::<Vec<_>>().await,
    };
    let expected = (999..=1003)
        .map(|height| {
            let key = block_height_to_key(&BlockHeight::new(height));
            let res = RemoteS3Response {
                bucket: aws_bucket.clone(),
                key,
                requester_pays: false,
                aws_endpoint: None,
            };
            (BlockHeight::new(height), res)
        })
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn get_current_height__returns_highest_continuous_block() {
    // given
    let client = mock_client!(aws_sdk_s3, [&put_happy_rule()]);
    let aws_bucket = "test-bucket".to_string();
    let storage = database();
    let sync_from = BlockHeight::new(0);
    let mut adapter =
        RemoteCache::new(aws_bucket, false, None, client, storage, sync_from);

    let expected = BlockHeight::new(123);
    let block = arb_proto_block();
    let block = BlockSourceEvent::OldBlock(expected, block);
    adapter.store_block(block).await.unwrap();

    // when
    let actual = adapter.get_current_height().await.unwrap().unwrap();

    // then
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn store_block__does_not_update_the_highest_continuous_block_if_not_contiguous() {
    // given
    let mut storage = database();
    let mut tx = storage.write_transaction();
    let starting_height = BlockHeight::from(1u32);
    tx.storage_as_mut::<LatestBlock>()
        .insert(&(), &starting_height)
        .unwrap();
    tx.commit().unwrap();
    let client = mock_client!(aws_sdk_s3, [&put_happy_rule()]);
    let aws_bucket = "test-bucket".to_string();
    let sync_from = BlockHeight::new(0);
    let mut adapter =
        RemoteCache::new(aws_bucket, false, None, client, storage, sync_from);

    let expected = BlockHeight::new(3);
    let block = arb_proto_block();
    let block = BlockSourceEvent::NewBlock(expected, block);
    adapter.store_block(block).await.unwrap();

    // when
    let expected = starting_height;
    let actual = adapter.get_current_height().await.unwrap().unwrap();
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn store_block__updates_the_highest_continuous_block_if_filling_a_gap() {
    let rules: Vec<_> = iter::repeat_with(put_happy_rule).take(10).collect();
    let client = mock_client!(aws_sdk_s3, rules.iter());
    let aws_bucket = "test-bucket".to_string();

    // given
    let db = database();
    let sync_from = BlockHeight::new(0);
    let mut adapter = RemoteCache::new(aws_bucket, false, None, client, db, sync_from);

    for height in 2..=10u32 {
        let height = BlockHeight::from(height);
        let block = arb_proto_block();
        let block = BlockSourceEvent::NewBlock(height, block.clone());
        adapter.store_block(block).await.unwrap();
    }
    // when
    let height = BlockHeight::from(1u32);
    let some_block = arb_proto_block();
    let block = BlockSourceEvent::OldBlock(height, some_block.clone());
    adapter.store_block(block).await.unwrap();

    // then
    let expected = BlockHeight::from(10u32);
    let actual = adapter.get_current_height().await.unwrap().unwrap();
    assert_eq!(expected, actual);

    assert!(adapter.synced)
}

#[tokio::test]
async fn store_block__new_block_updates_the_highest_continuous_block_if_synced() {
    let rules: Vec<_> = iter::repeat_with(put_happy_rule).take(10).collect();
    let client = mock_client!(aws_sdk_s3, rules.iter());
    let aws_bucket = "test-bucket".to_string();

    // given
    let db = database();
    let sync_from = BlockHeight::new(0);
    let mut adapter = RemoteCache::new(aws_bucket, false, None, client, db, sync_from);

    let height = BlockHeight::from(0u32);
    let some_block = arb_proto_block();
    let block = BlockSourceEvent::OldBlock(height, some_block.clone());
    adapter.store_block(block).await.unwrap();

    // when
    let height = BlockHeight::from(1u32);
    let some_block = arb_proto_block();
    let block = BlockSourceEvent::NewBlock(height, some_block.clone());
    adapter.store_block(block).await.unwrap();

    // then
    let expected = BlockHeight::from(1u32);
    let actual = adapter.get_current_height().await.unwrap().unwrap();
    assert_eq!(expected, actual);

    assert!(adapter.synced)
}

#[tokio::test]
async fn store_block__new_block_comes_first() {
    let rules: Vec<_> = iter::repeat_with(put_happy_rule).take(10).collect();
    let client = mock_client!(aws_sdk_s3, rules.iter());
    let aws_bucket = "test-bucket".to_string();

    // given
    let db = database();
    let sync_from = BlockHeight::new(0);
    let mut adapter = RemoteCache::new(aws_bucket, false, None, client, db, sync_from);

    // when
    let height = BlockHeight::from(0u32);
    let some_block = arb_proto_block();
    let block = BlockSourceEvent::NewBlock(height, some_block.clone());
    adapter.store_block(block).await.unwrap();

    // then
    let expected = BlockHeight::from(0u32);
    let actual = adapter.get_current_height().await.unwrap().unwrap();
    assert_eq!(expected, actual);

    assert!(adapter.synced);
}
