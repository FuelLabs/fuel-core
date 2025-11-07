use super::*;
use crate::{
    block_range_response::RemoteBlockRangeResponse,
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
use futures::StreamExt;
use std::iter;

fn database() -> StorageTransaction<InMemoryStorage<Column>> {
    InMemoryStorage::default().into_transaction()
}

fn arb_proto_block() -> ProtoBlock {
    let block = FuelBlock::default();
    let serializer = SerializerAdapter;
    let proto_block = serializer.serialize_block(&block).unwrap();
    proto_block
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
    let aws_id = "test-id".to_string();
    let aws_secret = "test-secret".to_string();
    let aws_region = "test-region".to_string();
    let aws_bucket = "test-bucket".to_string();
    let storage = database();
    let mut adapter =
        RemoteCache::new(aws_id, aws_secret, aws_region, aws_bucket, client, storage);
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
    let aws_id = "test-id".to_string();
    let aws_secret = "test-secret".to_string();
    let aws_region = "test-region".to_string();
    let aws_bucket = "test-bucket".to_string();
    let storage = database();
    let adapter = RemoteCache::new(
        aws_id.clone(),
        aws_secret.clone(),
        aws_region.clone(),
        aws_bucket.clone(),
        client,
        storage,
    );
    let start = BlockHeight::new(999);
    let end = BlockHeight::new(1003);

    // when
    let addresses = adapter.get_block_range(start, end).await.unwrap();

    // then
    let actual = match addresses {
        BlockRangeResponse::Literal(_) => {
            panic!("Expected remote response, got literal");
        }
        BlockRangeResponse::Remote(stream) => stream.collect::<Vec<_>>().await,
    };
    let expected = (999..=1003)
        .map(|height| RemoteBlockRangeResponse {
            region: aws_region.clone(),
            bucket: aws_bucket.clone(),
            key: block_height_to_key(&BlockHeight::new(height)),
            url: "todo".to_string(),
        })
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn get_current_height__returns_highest_continuos_block() {
    // given
    let client = mock_client!(aws_sdk_s3, [&put_happy_rule()]);
    let aws_id = "test-id".to_string();
    let aws_secret = "test-secret".to_string();
    let aws_region = "test-region".to_string();
    let aws_bucket = "test-bucket".to_string();
    let storage = database();
    let mut adapter =
        RemoteCache::new(aws_id, aws_secret, aws_region, aws_bucket, client, storage);
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
    let aws_id = "test-id".to_string();
    let aws_secret = "test-secret".to_string();
    let aws_region = "test-region".to_string();
    let aws_bucket = "test-bucket".to_string();
    let mut adapter =
        RemoteCache::new(aws_id, aws_secret, aws_region, aws_bucket, client, storage);
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
    let aws_id = "test-id".to_string();
    let aws_secret = "test-secret".to_string();
    let aws_region = "test-region".to_string();
    let aws_bucket = "test-bucket".to_string();

    // given
    let db = database();
    let mut adapter =
        RemoteCache::new(aws_id, aws_secret, aws_region, aws_bucket, client, db);

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
}
