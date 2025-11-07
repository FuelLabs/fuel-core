use super::*;
use crate::{
    block_range_response::RemoteBlockRangeResponse,
    blocks::importer_and_db_source::{
        BlockSerializer,
        serializer_adapter::SerializerAdapter,
    },
    db::table::Column,
};
use aws_sdk_s3::{
    operation::{
        get_object::GetObjectOutput,
        put_object::PutObjectOutput,
    },
    primitives::ByteStream,
};
use aws_smithy_mocks::{
    RuleMode,
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

fn database() -> StorageTransaction<InMemoryStorage<Column>> {
    InMemoryStorage::default().into_transaction()
}

fn arb_proto_block() -> ProtoBlock {
    let block = FuelBlock::default();
    let mut serializer = SerializerAdapter;
    let proto_block = serializer.serialize_block(&block).unwrap();
    proto_block
}

#[tokio::test]
async fn store_block__happy_path() {
    let put_happy_rule = mock!(Client::put_object)
        .match_requests(|req| req.bucket() == Some("test-bucket"))
        .sequence()
        .output(|| PutObjectOutput::builder().build())
        .build();
    // given
    let client = mock_client!(aws_sdk_s3, [&put_happy_rule]);
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
    let mut adapter = RemoteCache::new(
        aws_id.clone(),
        aws_secret.clone(),
        aws_region.clone(),
        aws_bucket.clone(),
        client,
        storage,
    );
    let start = BlockHeight::new(999);
    let end = BlockHeight::new(1003);
    let block = arb_proto_block();

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
    let put_happy_rule = mock!(Client::put_object)
        .match_requests(|req| req.bucket() == Some("test-bucket"))
        .sequence()
        .output(|| PutObjectOutput::builder().build())
        .build();
    // given
    let client = mock_client!(aws_sdk_s3, [&put_happy_rule]);
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
